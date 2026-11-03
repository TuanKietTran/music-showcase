//! JWT verification for Amazon Cognito (or any OIDC issuer using the same claim names).
//! Validates signature (RS256 via the issuer's JWKS), `iss`, `exp`, and `aud` (ID token) / `client_id` (access token),
//! then maps claims to subject attributes: sub -> id, custom:role -> role (else the highest cognito:groups entry),
//! custom:status (default active) -> status, custom:quota_bytes -> quotaBytes. Unknown values fail closed.
use crate::accounts::{valid_id, Account, ROLES, STATUSES};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde_json::Value;
use std::{collections::HashMap, sync::RwLock, time::{Duration, Instant}};

pub struct Oidc { issuer: String, client_id: String, jwks_url: String, default_quota: u64, http: reqwest::Client, keys: RwLock<(HashMap<String, DecodingKey>, Option<Instant>)> }

impl Oidc {
    /// MUSIC_COGNITO_REGION + MUSIC_COGNITO_USER_POOL_ID + MUSIC_COGNITO_CLIENT_ID, or MUSIC_OIDC_ISSUER + MUSIC_OIDC_CLIENT_ID.
    pub fn from_env() -> Option<Oidc> {
        let e = |n: &str| std::env::var(n).ok().filter(|v| !v.is_empty());
        let client_id = e("MUSIC_COGNITO_CLIENT_ID").or_else(|| e("MUSIC_OIDC_CLIENT_ID"))?;
        let issuer = match (e("MUSIC_COGNITO_REGION"), e("MUSIC_COGNITO_USER_POOL_ID")) {
            (Some(r), Some(p)) => format!("https://cognito-idp.{}.amazonaws.com/{}", r, p),
            _ => e("MUSIC_OIDC_ISSUER")?.trim_end_matches('/').to_string(),
        };
        if !issuer.starts_with("https://") { eprintln!("error: OIDC issuer must be https"); std::process::exit(1); }
        let quota = e("MUSIC_DEFAULT_QUOTA_BYTES").and_then(|v| v.parse().ok()).unwrap_or(268_435_456);
        println!("jwt auth enabled for issuer {} (client {})", issuer, client_id);
        Some(Oidc::new(issuer, client_id, quota))
    }
    pub fn new(issuer: String, client_id: String, default_quota: u64) -> Oidc {
        let jwks_url = format!("{}/.well-known/jwks.json", issuer);
        Oidc { issuer, client_id, jwks_url, default_quota, http: reqwest::Client::builder().timeout(Duration::from_secs(5)).build().expect("http client"), keys: RwLock::new((HashMap::new(), None)) }
    }
    #[cfg(test)]
    pub fn with_keys(issuer: &str, client_id: &str, keys: HashMap<String, DecodingKey>) -> Oidc { let o = Oidc::new(issuer.into(), client_id.into(), 1000); *o.keys.write().unwrap() = (keys, Some(Instant::now())); o }

    async fn refresh(&self) {
        if let Some(t) = self.keys.read().unwrap().1 { if t.elapsed() < Duration::from_secs(60) { return; } } // at most one fetch per minute
        let fetched: Option<Value> = async { self.http.get(&self.jwks_url).send().await.ok()?.json().await.ok() }.await;
        let mut guard = self.keys.write().unwrap();
        guard.1 = Some(Instant::now());
        if let Some(j) = fetched {
            let mut m = HashMap::new();
            for k in j["keys"].as_array().into_iter().flatten() {
                if k["kty"] == "RSA" { if let (Some(kid), Some(n), Some(e)) = (k["kid"].as_str(), k["n"].as_str(), k["e"].as_str()) { if let Ok(dk) = DecodingKey::from_rsa_components(n, e) { m.insert(kid.to_string(), dk); } } }
            }
            if !m.is_empty() { guard.0 = m; }
        }
    }
    async fn key(&self, kid: &str) -> Option<DecodingKey> {
        if let Some(k) = self.keys.read().unwrap().0.get(kid) { return Some(k.clone()); }
        self.refresh().await;
        self.keys.read().unwrap().0.get(kid).cloned()
    }
    pub async fn verify(&self, token: &str) -> Option<Account> {
        let header = decode_header(token).ok()?;
        if header.alg != Algorithm::RS256 { return None; } // never accept none/HS256 (algorithm confusion)
        let key = self.key(header.kid.as_deref()?).await?;
        let mut v = Validation::new(Algorithm::RS256);
        v.set_issuer(&[&self.issuer]); v.set_required_spec_claims(&["exp", "iss", "sub"]); v.validate_aud = false; v.leeway = 30;
        let claims = decode::<Value>(token, &key, &v).ok()?.claims;
        match claims["token_use"].as_str() {
            Some("id") if claims["aud"].as_str() == Some(self.client_id.as_str()) => {}
            Some("access") if claims["client_id"].as_str() == Some(self.client_id.as_str()) => {}
            _ => return None,
        }
        claims_to_account(&claims, self.default_quota)
    }
}

pub fn claims_to_account(c: &Value, default_quota: u64) -> Option<Account> {
    let id = c["sub"].as_str()?.to_ascii_lowercase();
    if !valid_id(&id) { return None; }
    let groups: Vec<&str> = c["cognito:groups"].as_array().map(|g| g.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
    let role = match c["custom:role"].as_str() {
        Some(r) if ROLES.contains(&r) => r.to_string(),
        Some(_) => "listener".to_string(), // unknown role value: least privilege
        None => ROLES.iter().find(|r| groups.contains(r)).unwrap_or(&"listener").to_string(), // ROLES is ordered admin > uploader > listener
    };
    let status = match c["custom:status"].as_str() { None => "active".to_string(), Some(s) if STATUSES.contains(&s) => s.to_string(), Some(_) => "suspended".to_string() }; // unknown status: fail closed
    let quota_bytes = match &c["custom:quota_bytes"] { Value::Null => default_quota, Value::String(s) => s.parse().unwrap_or(0), Value::Number(n) => n.as_u64().unwrap_or(0), _ => 0 };
    let name = c["cognito:username"].as_str().or(c["name"].as_str()).or(c["email"].as_str()).unwrap_or(&id).to_string();
    Some(Account { id, name, role, status, quota_bytes, unlimited: false })
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use rsa::{pkcs1::EncodeRsaPrivateKey, traits::PublicKeyParts, RsaPrivateKey};
    use serde_json::json;
    const ISS: &str = "https://cognito-idp.us-east-1.amazonaws.com/us-east-1_TEST";
    const SUB: &str = "6f1c2a3b-aaaa-bbbb-cccc-0123456789ab";
    fn b64url(b: &[u8]) -> String { const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"; let mut o = String::new(); for ch in b.chunks(3) { let n = ch.iter().enumerate().fold(0u32, |a, (i, x)| a | (*x as u32) << (16 - 8 * i)); for i in 0..=ch.len() { o.push(T[((n >> (18 - 6 * i)) & 63) as usize] as char); } } o }
    struct Kp { enc: EncodingKey, dec: DecodingKey }
    fn keypair() -> Kp {
        let mut rng = rsa::rand_core::OsRng;
        let sk = RsaPrivateKey::new(&mut rng, 2048).unwrap();
        let pem = sk.to_pkcs1_pem(rsa::pkcs8::LineEnding::LF).unwrap();
        let pk = sk.to_public_key();
        Kp { enc: EncodingKey::from_rsa_pem(pem.as_bytes()).unwrap(), dec: DecodingKey::from_rsa_components(&b64url(&pk.n().to_bytes_be()), &b64url(&pk.e().to_bytes_be())).unwrap() }
    }
    fn now() -> u64 { crate::media::now_secs() }
    fn sign(kp: &Kp, kid: &str, claims: Value) -> String { let mut h = Header::new(Algorithm::RS256); h.kid = Some(kid.into()); encode(&h, &claims, &kp.enc).unwrap() }
    fn id_claims() -> Value { json!({"sub": SUB, "iss": ISS, "aud": "client1", "token_use": "id", "exp": now() + 600, "custom:role": "uploader", "custom:quota_bytes": "5000", "cognito:username": "alice"}) }
    fn oidc(kp: &Kp) -> Oidc { let mut m = HashMap::new(); m.insert("k1".to_string(), kp.dec.clone()); Oidc::with_keys(ISS, "client1", m) }

    #[test] fn maps_claims_to_subject_attributes() {
        let a = claims_to_account(&id_claims(), 1000).unwrap();
        assert_eq!((a.id.as_str(), a.role.as_str(), a.status.as_str(), a.quota_bytes, a.name.as_str()), (SUB, "uploader", "active", 5000, "alice"));
        let d = claims_to_account(&json!({"sub": SUB}), 1234).unwrap(); // defaults: listener, active, default quota
        assert_eq!((d.role.as_str(), d.status.as_str(), d.quota_bytes), ("listener", "active", 1234));
        let g = claims_to_account(&json!({"sub": SUB, "cognito:groups": ["listener", "admin"]}), 1).unwrap(); assert_eq!(g.role, "admin"); // groups fallback, highest wins
        assert_eq!(claims_to_account(&json!({"sub": SUB, "custom:role": "root"}), 1).unwrap().role, "listener"); // unknown role -> least privilege
        assert_eq!(claims_to_account(&json!({"sub": SUB, "custom:status": "banned"}), 1).unwrap().status, "suspended"); // unknown status -> fail closed
        assert_eq!(claims_to_account(&json!({"sub": SUB, "custom:status": "suspended"}), 1).unwrap().status, "suspended");
        assert_eq!(claims_to_account(&json!({"sub": SUB, "custom:quota_bytes": "abc"}), 99).unwrap().quota_bytes, 0);
        assert!(claims_to_account(&json!({"sub": "../x"}), 1).is_none()); assert!(claims_to_account(&json!({}), 1).is_none());
    }
    #[tokio::test] async fn accepts_valid_id_token_and_rejects_every_tampering() {
        let kp = keypair(); let o = oidc(&kp);
        assert_eq!(o.verify(&sign(&kp, "k1", id_claims())).await.unwrap().id, SUB);
        let mut c = id_claims(); c["exp"] = json!(now() - 3600); assert!(o.verify(&sign(&kp, "k1", c)).await.is_none(), "expired");
        let mut c = id_claims(); c["iss"] = json!("https://evil.example/pool"); assert!(o.verify(&sign(&kp, "k1", c)).await.is_none(), "wrong issuer");
        let mut c = id_claims(); c["aud"] = json!("other-client"); assert!(o.verify(&sign(&kp, "k1", c)).await.is_none(), "wrong audience");
        let mut c = id_claims(); c["token_use"] = json!("refresh"); assert!(o.verify(&sign(&kp, "k1", c)).await.is_none(), "wrong token_use");
        let other = keypair(); assert!(o.verify(&sign(&other, "k1", id_claims())).await.is_none(), "signed by a different key");
        let t = sign(&kp, "k1", id_claims()); let mut parts: Vec<String> = t.split('.').map(String::from).collect(); parts[1] = b64url(br#"{"sub":"x","iss":"https://cognito-idp.us-east-1.amazonaws.com/us-east-1_TEST","aud":"client1","token_use":"id","exp":9999999999,"custom:role":"admin"}"#);
        assert!(o.verify(&parts.join(".")).await.is_none(), "payload swapped, signature kept");
        let none = format!("{}.{}.", b64url(br#"{"alg":"none","typ":"JWT","kid":"k1"}"#), parts[1]); assert!(o.verify(&none).await.is_none(), "alg none");
        let hs = encode(&Header { kid: Some("k1".into()), ..Header::new(Algorithm::HS256) }, &id_claims(), &EncodingKey::from_secret(b"x")).unwrap(); assert!(o.verify(&hs).await.is_none(), "HS256 confusion");
        assert!(o.verify("a.b.c").await.is_none()); assert!(o.verify("garbage").await.is_none());
    }
    #[tokio::test] async fn accepts_access_token_by_client_id() {
        let kp = keypair(); let o = oidc(&kp);
        let ok = json!({"sub": SUB, "iss": ISS, "client_id": "client1", "token_use": "access", "exp": now() + 600, "cognito:groups": ["uploader"]});
        let a = o.verify(&sign(&kp, "k1", ok)).await.unwrap(); assert_eq!(a.role, "uploader");
        let bad = json!({"sub": SUB, "iss": ISS, "client_id": "nope", "token_use": "access", "exp": now() + 600}); assert!(o.verify(&sign(&kp, "k1", bad)).await.is_none());
    }
}
