//! S3 backend logic, generic over a tiny object API so it can be tested with an in-memory fake.
//! The real implementation is `s3_sdk::SdkObjects` (feature `aws`).
use crate::storage::{parse_key, Attributes, Body, StoreError};
use std::{future::Future, io};
use tokio::{io::AsyncReadExt, sync::mpsc};

pub const DEFAULT_PART_SIZE: usize = 8 * 1024 * 1024; // objects above this use multipart; S3 minimum part size is 5 MiB

pub trait ObjectApi: Send + Sync + 'static {
    fn put_object(&self, key: &str, content_type: &str, body: Vec<u8>) -> impl Future<Output = Result<(), StoreError>> + Send;
    fn create_multipart(&self, key: &str, content_type: &str) -> impl Future<Output = Result<String, StoreError>> + Send;
    fn upload_part(&self, key: &str, upload_id: &str, part_number: i32, body: Vec<u8>) -> impl Future<Output = Result<String, StoreError>> + Send;
    fn complete_multipart(&self, key: &str, upload_id: &str, parts: Vec<(i32, String)>) -> impl Future<Output = Result<(), StoreError>> + Send;
    fn abort_multipart(&self, key: &str, upload_id: &str) -> impl Future<Output = Result<(), StoreError>> + Send;
    fn get(&self, key: &str, range: Option<(u64, u64)>) -> impl Future<Output = Result<mpsc::Receiver<io::Result<Vec<u8>>>, StoreError>> + Send;
    #[allow(dead_code)]
    fn head(&self, key: &str) -> impl Future<Output = Result<Option<u64>, StoreError>> + Send;
    fn delete_object(&self, key: &str) -> impl Future<Output = Result<(), StoreError>> + Send;
}

pub struct S3Store<C: ObjectApi> { pub client: C, pub bucket: String, pub region: String, pub encrypted: bool, pub max_object_bytes: u64, pub part_size: usize }

/// Fill `buf` from `first` + `rest` up to `want` bytes (never past `total`). Returns bytes added.
async fn fill<R: tokio::io::AsyncRead + Unpin + Send>(buf: &mut Vec<u8>, want: usize, rest: &mut R) -> Result<(), StoreError> {
    while buf.len() < want {
        let mut chunk = vec![0u8; (want - buf.len()).min(65536)];
        let n = rest.read(&mut chunk).await.map_err(|e| StoreError::Io(e.to_string()))?;
        if n == 0 { return Err(StoreError::Io("body ended early".into())); }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(())
}

impl<C: ObjectApi> S3Store<C> {
    pub fn attributes(&self) -> Attributes {
        Attributes { backend: "s3", encrypted: self.encrypted, max_object_bytes: self.max_object_bytes, region: Some(self.region.clone()), bucket: Some(self.bucket.clone()) }
    }
    pub async fn put<R: tokio::io::AsyncRead + Unpin + Send>(&self, key: &str, content_type: &str, first: &[u8], rest: &mut R, total: u64) -> Result<(), StoreError> {
        parse_key(key)?;
        if total <= self.part_size as u64 {
            let mut buf = first.to_vec(); fill(&mut buf, total as usize, rest).await?;
            return self.client.put_object(key, content_type, buf).await;
        }
        let upload_id = self.client.create_multipart(key, content_type).await?;
        let run = async {
            let mut parts: Vec<(i32, String)> = Vec::new();
            let mut pending = first.to_vec();
            let mut sent: u64 = 0; let mut n = 1i32;
            while sent < total {
                // At most one part (part_size bytes) is held in memory at a time.
                let want = (total - sent).min(self.part_size as u64) as usize;
                fill(&mut pending, want, rest).await?;
                let body: Vec<u8> = pending.drain(..want).collect();
                let etag = self.client.upload_part(key, &upload_id, n, body).await?;
                parts.push((n, etag)); sent += want as u64; n += 1;
            }
            self.client.complete_multipart(key, &upload_id, parts).await
        }.await;
        if let Err(e) = run {
            // Never leave orphan parts behind (the bucket lifecycle rule is only a backstop).
            let _ = self.client.abort_multipart(key, &upload_id).await;
            return Err(e);
        }
        Ok(())
    }
    pub async fn open(&self, key: &str, range: Option<(u64, u64)>) -> Result<Body, StoreError> {
        parse_key(key)?;
        Ok(Body::Chan(self.client.get(key, range).await?))
    }
    pub async fn delete(&self, key: &str) -> Result<(), StoreError> { parse_key(key)?; self.client.delete_object(key).await }
    #[allow(dead_code)]
    pub async fn stat(&self, key: &str) -> Result<Option<u64>, StoreError> { parse_key(key)?; self.client.head(key).await }
}

#[cfg(test)]
pub use fake::FakeS3;
#[cfg(test)]
mod fake {
    use super::*;
    use std::{collections::HashMap, sync::{atomic::{AtomicI32, Ordering}, Mutex}};
    #[derive(Default)]
    pub struct State {
        pub objects: HashMap<String, (Vec<u8>, String)>,
        pub uploads: HashMap<String, (String, String, Vec<(i32, Vec<u8>)>)>, // id -> (key, content type, parts)
        pub next: u32, pub aborted: u32, pub put_calls: u32, pub completed: u32,
    }
    #[derive(Default)]
    pub struct FakeS3 { pub state: Mutex<State>, pub fail_part: AtomicI32, pub fail_put: AtomicI32 }
    impl FakeS3 {
        pub fn open_uploads(&self) -> usize { self.state.lock().unwrap().uploads.len() }
        pub fn object(&self, key: &str) -> Option<(Vec<u8>, String)> { self.state.lock().unwrap().objects.get(key).cloned() }
    }
    impl ObjectApi for FakeS3 {
        async fn put_object(&self, key: &str, ct: &str, body: Vec<u8>) -> Result<(), StoreError> {
            if self.fail_put.load(Ordering::SeqCst) == 1 { return Err(StoreError::Io("injected put failure".into())); }
            let mut s = self.state.lock().unwrap(); s.put_calls += 1; s.objects.insert(key.into(), (body, ct.into())); Ok(())
        }
        async fn create_multipart(&self, key: &str, ct: &str) -> Result<String, StoreError> {
            let mut s = self.state.lock().unwrap(); s.next += 1; let id = format!("up{}", s.next);
            s.uploads.insert(id.clone(), (key.into(), ct.into(), vec![])); Ok(id)
        }
        async fn upload_part(&self, _key: &str, id: &str, n: i32, body: Vec<u8>) -> Result<String, StoreError> {
            if self.fail_part.load(Ordering::SeqCst) == n { return Err(StoreError::Io("injected part failure".into())); }
            let mut s = self.state.lock().unwrap();
            s.uploads.get_mut(id).ok_or(StoreError::NotFound)?.2.push((n, body)); Ok(format!("etag{}", n))
        }
        async fn complete_multipart(&self, _key: &str, id: &str, parts: Vec<(i32, String)>) -> Result<(), StoreError> {
            let mut s = self.state.lock().unwrap();
            let (key, ct, mut stored) = s.uploads.remove(id).ok_or(StoreError::NotFound)?;
            if parts.len() != stored.len() { return Err(StoreError::Io("part list mismatch".into())); }
            stored.sort_by_key(|p| p.0);
            let data: Vec<u8> = stored.into_iter().flat_map(|p| p.1).collect();
            s.completed += 1; s.objects.insert(key, (data, ct)); Ok(())
        }
        async fn abort_multipart(&self, _key: &str, id: &str) -> Result<(), StoreError> {
            let mut s = self.state.lock().unwrap(); s.uploads.remove(id); s.aborted += 1; Ok(())
        }
        async fn get(&self, key: &str, range: Option<(u64, u64)>) -> Result<mpsc::Receiver<io::Result<Vec<u8>>>, StoreError> {
            let data = self.state.lock().unwrap().objects.get(key).map(|o| o.0.clone()).ok_or(StoreError::NotFound)?;
            let (a, b) = range.unwrap_or((0, data.len().saturating_sub(1) as u64));
            let slice = if data.is_empty() || a as usize >= data.len() { vec![] } else { data[a as usize..=(b as usize).min(data.len() - 1)].to_vec() };
            let (tx, rx) = mpsc::channel(4);
            tokio::spawn(async move { for c in slice.chunks(7) { if tx.send(Ok(c.to_vec())).await.is_err() { break; } } });
            Ok(rx)
        }
        async fn head(&self, key: &str) -> Result<Option<u64>, StoreError> { Ok(self.state.lock().unwrap().objects.get(key).map(|o| o.0.len() as u64)) }
        async fn delete_object(&self, key: &str) -> Result<(), StoreError> { self.state.lock().unwrap().objects.remove(key); Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{object_key, Storage};
    use std::sync::atomic::Ordering;
    const ID: &str = "0123456789abcdef";
    fn store(part: usize) -> S3Store<FakeS3> { S3Store { client: FakeS3::default(), bucket: "b".into(), region: "r".into(), encrypted: true, max_object_bytes: 1 << 20, part_size: part } }
    fn data(n: usize) -> Vec<u8> { (0..n).map(|i| (i % 251) as u8).collect() }
    #[tokio::test] async fn small_object_is_a_single_put_with_sniffed_content_type() {
        let s = store(10); let key = object_key("alice", ID, "mp3").unwrap(); let d = data(10);
        let mut rest: &[u8] = &d[4..];
        s.put(&key, "audio/mpeg", &d[..4], &mut rest, 10).await.unwrap();
        assert_eq!(s.client.object(&key), Some((d, "audio/mpeg".to_string())));
        assert_eq!(s.client.state.lock().unwrap().put_calls, 1);
        assert_eq!(s.client.open_uploads(), 0);
    }
    #[tokio::test] async fn large_object_uses_multipart_and_reassembles() {
        let s = store(10); let key = object_key("alice", ID, "flac").unwrap(); let d = data(35);
        let mut rest: &[u8] = &d[6..];
        s.put(&key, "audio/flac", &d[..6], &mut rest, 35).await.unwrap();
        assert_eq!(s.client.object(&key).unwrap().0, d);
        let st = s.client.state.lock().unwrap(); assert_eq!((st.completed, st.put_calls, st.uploads.len()), (1, 0, 0));
    }
    #[tokio::test] async fn failed_part_aborts_the_multipart_upload() {
        let s = store(10); s.client.fail_part.store(3, Ordering::SeqCst);
        let key = object_key("alice", ID, "wav").unwrap(); let d = data(35);
        let mut rest: &[u8] = &d[2..];
        assert!(s.put(&key, "audio/wav", &d[..2], &mut rest, 35).await.is_err());
        assert_eq!(s.client.open_uploads(), 0, "no orphan parts");
        assert_eq!(s.client.state.lock().unwrap().aborted, 1);
        assert_eq!(s.client.object(&key), None);
    }
    #[tokio::test] async fn dropped_connection_mid_body_aborts_too() {
        let s = store(10); let key = object_key("alice", ID, "ogg").unwrap(); let d = data(25);
        let mut rest: &[u8] = &d[2..15]; // body ends before Content-Length
        assert!(s.put(&key, "audio/ogg", &d[..2], &mut rest, 35).await.is_err());
        assert_eq!(s.client.open_uploads(), 0);
        assert_eq!(s.client.state.lock().unwrap().aborted, 1);
        assert_eq!(s.client.object(&key), None);
    }
    #[tokio::test] async fn failed_single_put_leaves_nothing() {
        let s = store(100); s.client.fail_put.store(1, Ordering::SeqCst);
        let key = object_key("alice", ID, "m4a").unwrap(); let mut rest: &[u8] = &[2, 3];
        assert!(s.put(&key, "audio/mp4", &[1], &mut rest, 3).await.is_err());
        assert_eq!(s.client.object(&key), None);
    }
    #[tokio::test] async fn rejects_non_contract_keys_before_any_call() {
        let s = store(10); let mut rest: &[u8] = &[];
        for bad in ["accounts/alice/../bob/0123456789abcdef.mp3", "evil/0123456789abcdef.mp3", "accounts/alice/x.mp3"] {
            assert_eq!(s.put(bad, "x", &[1], &mut rest, 1).await, Err(StoreError::BadKey));
            assert_eq!(s.delete(bad).await, Err(StoreError::BadKey));
            assert_eq!(s.stat(bad).await, Err(StoreError::BadKey));
            assert!(s.open(bad, None).await.is_err());
        }
        let st = s.client.state.lock().unwrap(); assert_eq!((st.put_calls, st.next), (0, 0));
    }
    #[tokio::test] async fn open_with_range_stat_and_delete() {
        let s = Storage::Fake(store(10)); let key = object_key("alice", ID, "mp3").unwrap(); let d = data(30);
        let mut rest: &[u8] = &d[1..];
        s.put(&key, "audio/mpeg", &d[..1], &mut rest, 30).await.unwrap();
        assert_eq!(s.stat(&key).await.unwrap(), Some(30));
        let mut b = s.open(&key, Some((3, 12))).await.unwrap(); let mut got = vec![];
        while let Some(c) = b.next_chunk().await { got.extend(c.unwrap()); }
        assert_eq!(got, d[3..13]);
        s.delete(&key).await.unwrap();
        assert_eq!(s.stat(&key).await.unwrap(), None);
        assert_eq!(s.open(&key, None).await.err(), Some(StoreError::NotFound));
        let a = s.attributes(); assert_eq!((a.backend, a.encrypted, a.bucket.as_deref(), a.region.as_deref()), ("s3", true, Some("b"), Some("r")));
    }
}
