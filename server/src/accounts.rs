//! Accounts (docs/abac.md section 3): subject attributes plus bearer-token lookup by SHA-256.
use crate::http::constant_time_eq;
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub name: String,
    pub role: String,   // admin | uploader | listener
    pub status: String, // active | suspended
    pub quota_bytes: u64,
    /// Only the built-in bootstrap admin: quotaBytes 0 means "no limit".
    pub unlimited: bool,
}

pub const ROLES: [&str; 3] = ["admin", "uploader", "listener"];
pub const STATUSES: [&str; 2] = ["active", "suspended"];

/// Ids are `[a-z0-9_-]{1,64}` (the contract said 32; 64 lets an OIDC `sub` UUID be used verbatim).
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

pub fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data).iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn bootstrap_admin() -> Account {
    Account { id: "admin".into(), name: "Admin".into(), role: "admin".into(), status: "active".into(), quota_bytes: 0, unlimited: true }
}

pub struct Accounts {
    entries: Vec<(Account, String)>, // account + lowercase hex token hash
    bootstrap_hash: String,
}

impl Accounts {
    pub fn new(bootstrap_token: &str, entries: Vec<(Account, String)>) -> Accounts {
        Accounts { entries, bootstrap_hash: sha256_hex(bootstrap_token.as_bytes()) }
    }
    /// Hash the presented token and compare against every account in constant time (no early exit).
    pub fn lookup(&self, token: &str) -> Option<Account> {
        let h = sha256_hex(token.as_bytes());
        let mut found: Option<&Account> = None;
        let admin = bootstrap_admin();
        if constant_time_eq(h.as_bytes(), self.bootstrap_hash.as_bytes()) { found = Some(&admin); }
        let mut hit: Option<Account> = found.cloned();
        for (acct, stored) in &self.entries {
            if constant_time_eq(h.as_bytes(), stored.as_bytes()) && hit.is_none() { hit = Some(acct.clone()); }
        }
        hit
    }
    pub fn len(&self) -> usize { self.entries.len() }
}

/// Parse accounts.tsv: id, name, role, status, quotaBytes, tokenSha256. Errors carry the line number.
pub fn parse_accounts(text: &str) -> Result<Vec<(Account, String)>, String> {
    let mut out: Vec<(Account, String)> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let n = i + 1;
        if line.trim().is_empty() || line.trim_start().starts_with('#') { continue; }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 6 { return Err(format!("line {}: expected 6 tab-separated fields, got {}", n, f.len())); }
        let id = f[0];
        if !valid_id(id) { return Err(format!("line {}: invalid id", n)); }
        if id == "system" || id == "admin" { return Err(format!("line {}: id '{}' is reserved", n, id)); }
        if out.iter().any(|(a, _)| a.id == id) { return Err(format!("line {}: duplicate id", n)); }
        if !ROLES.contains(&f[2]) { return Err(format!("line {}: invalid role", n)); }
        if !STATUSES.contains(&f[3]) { return Err(format!("line {}: invalid status", n)); }
        let quota: u64 = f[4].parse().map_err(|_| format!("line {}: invalid quotaBytes", n))?;
        let hash = f[5].to_ascii_lowercase();
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) { return Err(format!("line {}: tokenSha256 must be 64 hex chars", n)); }
        out.push((Account { id: id.into(), name: f[1].into(), role: f[2].into(), status: f[3].into(), quota_bytes: quota, unlimited: false }, hash));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    const H1: &str = "2bb80d537b1da3e38bd30361aa855686bde0eacd7162fef6a25fe97bf527a25b"; // sha256("secret")
    #[test] fn hash_matches_known_vector() { assert_eq!(sha256_hex(b"secret"), H1); }
    #[test] fn parses_good_file_with_comments() {
        let text = format!("# comment\n\nalice\tAlice\tuploader\tactive\t1000\t{}\nbob\tBob\tlistener\tsuspended\t0\t{}\n", H1, H1.to_uppercase());
        let v = parse_accounts(&text).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].0.role, "uploader"); assert_eq!(v[0].0.quota_bytes, 1000);
        assert_eq!(v[1].1, H1); // hash lowercased
    }
    #[test] fn rejects_bad_lines_with_line_number() {
        let bad = |t: &str| parse_accounts(t).unwrap_err();
        assert!(bad(&format!("alice\tA\tuploader\tactive\t1\n\nbob\tB\tx\tactive\t1\t{}", H1)).starts_with("line 1"));
        assert!(bad(&format!("\nbob\tB\tx\tactive\t1\t{}", H1)).starts_with("line 2"));
        assert!(bad(&format!("admin\tA\tadmin\tactive\t1\t{}", H1)).contains("reserved"));
        assert!(bad(&format!("system\tA\tadmin\tactive\t1\t{}", H1)).contains("reserved"));
        assert!(bad(&format!("A_B\tA\tlistener\tactive\t1\t{}", H1)).contains("invalid id"));
        assert!(bad(&format!("a\tA\tlistener\tweird\t1\t{}", H1)).contains("status"));
        assert!(bad(&format!("a\tA\tlistener\tactive\t-1\t{}", H1)).contains("quota"));
        assert!(bad("a\tA\tlistener\tactive\t1\tnothex").contains("64 hex"));
        assert!(bad(&format!("a\tA\tlistener\tactive\t1\t{}\na\tB\tlistener\tactive\t1\t{}", H1, H1)).contains("duplicate"));
    }
    #[test] fn lookup_by_token_hash_and_bootstrap() {
        let entries = parse_accounts(&format!("alice\tAlice\tuploader\tactive\t5\t{}", H1)).unwrap();
        let accts = Accounts::new("boot-token", entries);
        assert_eq!(accts.lookup("secret").unwrap().id, "alice");
        let admin = accts.lookup("boot-token").unwrap();
        assert!(admin.id == "admin" && admin.unlimited && admin.role == "admin");
        assert!(accts.lookup("nope").is_none());
        assert!(accts.lookup("").is_none());
    }
    #[test] fn id_rules() { assert!(valid_id("a-b_9")); assert!(!valid_id("")); assert!(!valid_id("A")); assert!(!valid_id("a/b")); assert!(!valid_id(&"a".repeat(65))); assert!(valid_id(&"a".repeat(64))); }
}
