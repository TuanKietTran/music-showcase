//! Real S3 implementation of `ObjectApi` using the official AWS SDK and the default credential chain only
//! (no static keys anywhere). Defence in depth (docs/aws.md): with `MUSIC_S3_WRITER_ROLE_ARN` set, every write or delete
//! runs under credentials from `sts:AssumeRole` carrying the session tag `accountId=<owner of the key>`, so the writer
//! role's IAM policy (`accounts/${aws:PrincipalTag/accountId}/*`) independently limits the effective permission to the
//! caller's own prefix, even if the application's own ABAC had a bug.
use crate::s3store::{ObjectApi, S3Store, DEFAULT_PART_SIZE};
use crate::storage::{parse_key, StoreError};
use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_s3::{error::DisplayErrorContext, primitives::ByteStream, types::{CompletedMultipartUpload, CompletedPart, ServerSideEncryption}, Client};
use std::{collections::HashMap, io, sync::Mutex, time::{Duration, Instant, SystemTime}};
use tokio::sync::mpsc;

pub struct SdkObjects { base: Client, conf: aws_config::SdkConfig, sts: aws_sdk_sts::Client, bucket: String, kms_key: String, writer_role: Option<String>, sessions: Mutex<HashMap<String, (Client, Instant)>> }

fn io_err<E: std::error::Error + Send + Sync + 'static>(e: E) -> StoreError { StoreError::Io(DisplayErrorContext(&e).to_string()) }
fn sse() -> ServerSideEncryption { ServerSideEncryption::AwsKms }

/// Connect from the environment, then fail startup unless the bucket's default encryption is SSE-KMS or SSE-S3.
pub async fn connect(max_object_bytes: u64) -> Result<S3Store<SdkObjects>, String> {
    let need = |n: &str| std::env::var(n).ok().filter(|v| !v.is_empty()).ok_or_else(|| format!("{} is required when MUSIC_STORAGE=s3", n));
    let writer_role = std::env::var("MUSIC_S3_WRITER_ROLE_ARN").ok().filter(|v| !v.is_empty());
    connect_with(need("MUSIC_S3_BUCKET")?, need("MUSIC_S3_REGION")?, need("MUSIC_S3_KMS_KEY_ARN")?, writer_role, max_object_bytes).await
}

pub async fn connect_with(bucket: String, region: String, kms_key: String, writer_role: Option<String>, max_object_bytes: u64) -> Result<S3Store<SdkObjects>, String> {
    let conf = aws_config::defaults(BehaviorVersion::latest()).region(Region::new(region.clone())).load().await;
    let base = Client::new(&conf);
    let enc = base.get_bucket_encryption().bucket(&bucket).send().await.map_err(|e| format!("cannot read bucket encryption of '{}': {}", bucket, DisplayErrorContext(&e)))?;
    let algo = enc.server_side_encryption_configuration().and_then(|c| c.rules().first()).and_then(|r| r.apply_server_side_encryption_by_default()).map(|d| d.sse_algorithm().clone());
    match algo { Some(ServerSideEncryption::AwsKms) | Some(ServerSideEncryption::Aes256) => {}, other => return Err(format!("bucket '{}' default encryption must be SSE-KMS or SSE-S3, found {:?}", bucket, other)) }
    let sts = aws_sdk_sts::Client::new(&conf);
    Ok(S3Store { client: SdkObjects { base, conf, sts, bucket: bucket.clone(), kms_key, writer_role, sessions: Mutex::new(HashMap::new()) }, bucket, region, encrypted: true, max_object_bytes, part_size: DEFAULT_PART_SIZE })
}

impl SdkObjects {
    /// Client whose effective permissions are limited to the key's owner prefix (session tag), cached ~13 minutes.
    async fn writer(&self, key: &str) -> Result<Client, StoreError> {
        let role = match &self.writer_role { Some(r) => r, None => return Ok(self.base.clone()) };
        let (owner, _, _) = parse_key(key)?;
        if let Some((c, exp)) = self.sessions.lock().unwrap().get(owner) { if *exp > Instant::now() { return Ok(c.clone()); } }
        let tag = aws_sdk_sts::types::Tag::builder().key("accountId").value(owner).build().map_err(io_err)?;
        let out = self.sts.assume_role().role_arn(role).role_session_name(format!("music-{}", owner.chars().take(50).collect::<String>())).duration_seconds(900).tags(tag).send().await.map_err(io_err)?;
        let c = out.credentials().ok_or_else(|| StoreError::Io("assume-role returned no credentials".into()))?;
        let expiry = SystemTime::try_from(*c.expiration()).ok();
        let creds = Credentials::new(c.access_key_id(), c.secret_access_key(), Some(c.session_token().to_string()), expiry, "music-account-session");
        let client = Client::from_conf(aws_sdk_s3::config::Builder::from(&self.conf).credentials_provider(creds).build());
        self.sessions.lock().unwrap().insert(owner.to_string(), (client.clone(), Instant::now() + Duration::from_secs(780)));
        Ok(client)
    }
}

impl ObjectApi for SdkObjects {
    async fn put_object(&self, key: &str, content_type: &str, body: Vec<u8>) -> Result<(), StoreError> {
        self.writer(key).await?.put_object().bucket(&self.bucket).key(key).content_type(content_type).server_side_encryption(sse()).ssekms_key_id(&self.kms_key).body(ByteStream::from(body)).send().await.map_err(io_err)?;
        Ok(())
    }
    async fn create_multipart(&self, key: &str, content_type: &str) -> Result<String, StoreError> {
        let o = self.writer(key).await?.create_multipart_upload().bucket(&self.bucket).key(key).content_type(content_type).server_side_encryption(sse()).ssekms_key_id(&self.kms_key).send().await.map_err(io_err)?;
        o.upload_id().map(str::to_string).ok_or_else(|| StoreError::Io("no upload id".into()))
    }
    async fn upload_part(&self, key: &str, upload_id: &str, n: i32, body: Vec<u8>) -> Result<String, StoreError> {
        let o = self.writer(key).await?.upload_part().bucket(&self.bucket).key(key).upload_id(upload_id).part_number(n).body(ByteStream::from(body)).send().await.map_err(io_err)?;
        o.e_tag().map(str::to_string).ok_or_else(|| StoreError::Io("no etag".into()))
    }
    async fn complete_multipart(&self, key: &str, upload_id: &str, parts: Vec<(i32, String)>) -> Result<(), StoreError> {
        let parts = parts.into_iter().map(|(n, etag)| CompletedPart::builder().part_number(n).e_tag(etag).build()).collect::<Vec<_>>();
        self.writer(key).await?.complete_multipart_upload().bucket(&self.bucket).key(key).upload_id(upload_id).multipart_upload(CompletedMultipartUpload::builder().set_parts(Some(parts)).build()).send().await.map_err(io_err)?;
        Ok(())
    }
    async fn abort_multipart(&self, key: &str, upload_id: &str) -> Result<(), StoreError> {
        self.writer(key).await?.abort_multipart_upload().bucket(&self.bucket).key(key).upload_id(upload_id).send().await.map_err(io_err)?;
        Ok(())
    }
    async fn get(&self, key: &str, range: Option<(u64, u64)>) -> Result<mpsc::Receiver<io::Result<Vec<u8>>>, StoreError> {
        let mut req = self.base.get_object().bucket(&self.bucket).key(key);
        if let Some((a, b)) = range { req = req.range(format!("bytes={}-{}", a, b)); }
        let out = match req.send().await { Ok(o) => o, Err(e) if e.as_service_error().map(|s| s.is_no_such_key()).unwrap_or(false) => return Err(StoreError::NotFound), Err(e) => return Err(io_err(e)) };
        let (tx, rx) = mpsc::channel(4);
        let mut body = out.body;
        tokio::spawn(async move {
            loop {
                match body.try_next().await {
                    Ok(Some(chunk)) => { if tx.send(Ok(chunk.to_vec())).await.is_err() { break; } }
                    Ok(None) => break,
                    Err(e) => { let _ = tx.send(Err(io::Error::other(e.to_string()))).await; break; }
                }
            }
        });
        Ok(rx)
    }
    async fn head(&self, key: &str) -> Result<Option<u64>, StoreError> {
        match self.base.head_object().bucket(&self.bucket).key(key).send().await {
            Ok(o) => Ok(Some(o.content_length().unwrap_or(0).max(0) as u64)),
            Err(e) if e.as_service_error().map(|s| s.is_not_found()).unwrap_or(false) => Ok(None),
            Err(e) => Err(io_err(e)),
        }
    }
    async fn delete_object(&self, key: &str) -> Result<(), StoreError> {
        self.writer(key).await?.delete_object().bucket(&self.bucket).key(key).send().await.map_err(io_err)?; // versioned bucket: writes a delete marker
        Ok(())
    }
}

/// Opt-in integration test against a real bucket. Skipped unless MUSIC_TEST_S3_BUCKET is set:
///   MUSIC_TEST_S3_BUCKET=... MUSIC_TEST_S3_REGION=... MUSIC_TEST_S3_KMS_KEY_ARN=... [MUSIC_TEST_S3_WRITER_ROLE_ARN=...] cargo test --features aws s3_integration -- --nocapture
/// Uses a random account id under `accounts/`; everything it writes is deleted (versioned buckets keep a delete marker + noncurrent versions).
#[cfg(test)]
mod integration {
    use super::*;
    use crate::storage::{object_key, random_hex};
    use std::pin::Pin;
    use tokio::io::AsyncRead;

    struct FailAfter { left: usize }
    impl AsyncRead for FailAfter {
        fn poll_read(mut self: Pin<&mut Self>, _cx: &mut std::task::Context<'_>, buf: &mut tokio::io::ReadBuf<'_>) -> std::task::Poll<io::Result<()>> {
            if self.left == 0 { return std::task::Poll::Ready(Err(io::Error::other("injected network failure"))); }
            let n = self.left.min(buf.remaining()).min(1 << 20); buf.put_slice(&vec![7u8; n]); self.left -= n; std::task::Poll::Ready(Ok(()))
        }
    }

    #[tokio::test]
    async fn s3_integration() {
        let bucket = match std::env::var("MUSIC_TEST_S3_BUCKET") { Ok(b) if !b.is_empty() => b, _ => { eprintln!("skipped: MUSIC_TEST_S3_BUCKET not set"); return; } };
        let (region, kms) = (std::env::var("MUSIC_TEST_S3_REGION").expect("MUSIC_TEST_S3_REGION"), std::env::var("MUSIC_TEST_S3_KMS_KEY_ARN").expect("MUSIC_TEST_S3_KMS_KEY_ARN"));
        let mut st = connect_with(bucket.clone(), region, kms, std::env::var("MUSIC_TEST_S3_WRITER_ROLE_ARN").ok().filter(|v| !v.is_empty()), 50 << 20).await.expect("connect (bucket default encryption must be SSE-KMS/SSE-S3)");
        st.part_size = 5 << 20; // S3 minimum part size
        let owner = format!("it-{}", random_hex(4).unwrap());
        // small object: single PutObject, content type from the sniffed format
        let k1 = object_key(&owner, &random_hex(8).unwrap(), "mp3").unwrap();
        let small: Vec<u8> = b"ID3\x04\x00\x00\x00\x00\x00\x00".iter().copied().chain((0..4000u32).map(|i| (i % 251) as u8)).collect();
        let mut rest: &[u8] = &small[10..]; st.put(&k1, "audio/mpeg", &small[..10], &mut rest, small.len() as u64).await.unwrap();
        assert_eq!(st.stat(&k1).await.unwrap(), Some(small.len() as u64));
        let head = st.client.base.head_object().bucket(&bucket).key(&k1).send().await.unwrap();
        assert_eq!(head.content_type(), Some("audio/mpeg")); assert_eq!(head.server_side_encryption(), Some(&ServerSideEncryption::AwsKms));
        let mut b = st.open(&k1, Some((100, 199))).await.unwrap(); let mut got = vec![]; while let Some(c) = b.next_chunk().await { got.extend(c.unwrap()); }
        assert_eq!(got, small[100..200]);
        // multipart (11 MiB -> 5 + 5 + 1 MiB parts)
        let k2 = object_key(&owner, &random_hex(8).unwrap(), "flac").unwrap();
        let big: Vec<u8> = (0..(11u32 << 20)).map(|i| (i % 253) as u8).collect();
        let mut rest: &[u8] = &big[4..]; st.put(&k2, "audio/flac", &big[..4], &mut rest, big.len() as u64).await.unwrap();
        assert_eq!(st.stat(&k2).await.unwrap(), Some(big.len() as u64));
        // failure mid-upload aborts the multipart upload: no orphan parts remain
        let k3 = object_key(&owner, &random_hex(8).unwrap(), "wav").unwrap();
        let mut failing = FailAfter { left: 7 << 20 };
        assert!(st.put(&k3, "audio/wav", &[1, 2, 3, 4], &mut failing, 12 << 20).await.is_err());
        let pending = st.client.base.list_multipart_uploads().bucket(&bucket).prefix(format!("accounts/{}/", owner)).send().await.unwrap();
        assert!(pending.uploads().is_empty(), "orphan multipart uploads: {:?}", pending.uploads().len());
        assert_eq!(st.stat(&k3).await.unwrap(), None);
        // delete removes the current object
        for k in [&k1, &k2] { st.delete(k).await.unwrap(); assert_eq!(st.stat(k).await.unwrap(), None); }
        // traversal keys never reach S3
        assert_eq!(st.stat("accounts/../x").await, Err(StoreError::BadKey));
    }
}
