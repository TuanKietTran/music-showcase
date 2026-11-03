//! Storage backends (docs/abac.md section 4). Keys are `accounts/<ownerId>/<trackId>.<ext>` on every backend,
//! so the same layout is the S3 key layout and an AWS prefix policy per account works unchanged.
use crate::accounts::valid_id;
#[cfg(any(feature = "aws", test))]
use crate::s3store::S3Store;
use std::{io, os::unix::fs::DirBuilderExt, path::PathBuf};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, sync::mpsc};

pub const FORMATS: [&str; 5] = ["mp3", "ogg", "flac", "m4a", "wav"];

#[derive(Debug, PartialEq, Eq)]
pub enum StoreError { NotFound, BadKey, Io(String) }
impl From<io::Error> for StoreError {
    fn from(e: io::Error) -> StoreError { if e.kind() == io::ErrorKind::NotFound { StoreError::NotFound } else { StoreError::Io(e.to_string()) } }
}

pub fn valid_track_id(id: &str) -> bool { id.len() == 16 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) }

/// Build a key from validated parts. Anything that would not round-trip through `parse_key` is refused.
pub fn object_key(owner: &str, track_id: &str, ext: &str) -> Result<String, StoreError> {
    if !valid_id(owner) || !valid_track_id(track_id) || !FORMATS.contains(&ext) { return Err(StoreError::BadKey); }
    Ok(format!("accounts/{}/{}.{}", owner, track_id, ext))
}
/// Strict inverse of `object_key`: `(owner, trackId, ext)`. Rejects `..`, extra segments, odd characters, absolute paths.
pub fn parse_key(key: &str) -> Result<(&str, &str, &str), StoreError> {
    let rest = key.strip_prefix("accounts/").ok_or(StoreError::BadKey)?;
    let (owner, file) = rest.split_once('/').ok_or(StoreError::BadKey)?;
    let (id, ext) = file.split_once('.').ok_or(StoreError::BadKey)?;
    if !valid_id(owner) || !valid_track_id(id) || !FORMATS.contains(&ext) { return Err(StoreError::BadKey); }
    Ok((owner, id, ext))
}

/// Streaming read handle: a local file window or a channel fed by an S3 `GetObject` body.
pub enum Body { File { file: tokio::fs::File, remaining: u64 }, Chan(mpsc::Receiver<io::Result<Vec<u8>>>) }
impl Body {
    pub async fn next_chunk(&mut self) -> Option<io::Result<Vec<u8>>> {
        match self {
            Body::File { file, remaining } => {
                if *remaining == 0 { return None; }
                let mut buf = vec![0u8; (*remaining).min(16384) as usize];
                match file.read(&mut buf).await {
                    Ok(0) => None,
                    Ok(n) => { buf.truncate(n); *remaining -= n as u64; Some(Ok(buf)) }
                    Err(e) => Some(Err(e)),
                }
            }
            Body::Chan(rx) => rx.recv().await,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Attributes { pub backend: &'static str, pub encrypted: bool, pub max_object_bytes: u64, pub region: Option<String>, pub bucket: Option<String> }

pub fn random_hex(bytes: usize) -> io::Result<String> {
    use std::io::Read;
    let mut buf = vec![0u8; bytes];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut buf)?;
    Ok(buf.iter().map(|b| format!("{:02x}", b)).collect())
}

pub struct LocalStore { pub root: PathBuf, pub max_object_bytes: u64 }
impl LocalStore {
    fn path(&self, key: &str) -> Result<PathBuf, StoreError> { parse_key(key)?; Ok(self.root.join(key)) }
    async fn put<R: tokio::io::AsyncRead + Unpin + Send>(&self, key: &str, first: &[u8], rest: &mut R, total: u64) -> Result<(), StoreError> {
        let dest = self.path(key)?;
        let dir = dest.parent().ok_or(StoreError::BadKey)?.to_path_buf();
        { let d = dir.clone(); tokio::task::spawn_blocking(move || std::fs::DirBuilder::new().recursive(true).mode(0o700).create(d)).await.map_err(|e| StoreError::Io(e.to_string()))??; }
        let tmp = dir.join(format!(".tmp-{}", random_hex(8)?));
        let res = async {
            let mut f = tokio::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp).await?;
            f.write_all(first).await?;
            let mut written = first.len() as u64;
            let mut buf = vec![0u8; 65536];
            while written < total {
                let want = (total - written).min(buf.len() as u64) as usize;
                let n = rest.read(&mut buf[..want]).await?;
                if n == 0 { return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "body ended early")); }
                f.write_all(&buf[..n]).await?; written += n as u64;
            }
            f.flush().await?; drop(f);
            tokio::fs::rename(&tmp, &dest).await
        }.await;
        if let Err(e) = res { let _ = tokio::fs::remove_file(&tmp).await; return Err(e.into()); }
        Ok(())
    }
    async fn open(&self, key: &str, range: Option<(u64, u64)>) -> Result<Body, StoreError> {
        let path = self.path(key)?;
        let mut file = tokio::fs::File::open(&path).await?;
        let len = file.metadata().await?.len();
        let (start, end) = range.unwrap_or((0, len.saturating_sub(1)));
        if len == 0 || start >= len { return Ok(Body::File { file, remaining: 0 }); }
        let end = end.min(len - 1);
        tokio::io::AsyncSeekExt::seek(&mut file, io::SeekFrom::Start(start)).await?;
        Ok(Body::File { file, remaining: end - start + 1 })
    }
}

pub enum Storage {
    Local(LocalStore),
    #[cfg(feature = "aws")]
    S3(S3Store<crate::s3_sdk::SdkObjects>),
    #[cfg(test)]
    Fake(S3Store<crate::s3store::FakeS3>),
}

impl Storage {
    pub fn local(root: PathBuf, max_object_bytes: u64) -> Storage { Storage::Local(LocalStore { root, max_object_bytes }) }
    pub fn attributes(&self) -> Attributes {
        match self {
            Storage::Local(l) => Attributes { backend: "local", encrypted: false, max_object_bytes: l.max_object_bytes, region: None, bucket: None },
            #[cfg(feature = "aws")]
            Storage::S3(s) => s.attributes(),
            #[cfg(test)]
            Storage::Fake(s) => s.attributes(),
        }
    }
    /// Write `total` bytes: `first` (already read for sniffing) followed by the rest of `rest`. `content_type` comes from the sniffed format.
    pub async fn put<R: tokio::io::AsyncRead + Unpin + Send>(&self, key: &str, content_type: &str, first: &[u8], rest: &mut R, total: u64) -> Result<(), StoreError> {
        match self {
            Storage::Local(l) => { let _ = content_type; l.put(key, first, rest, total).await }
            #[cfg(feature = "aws")]
            Storage::S3(s) => s.put(key, content_type, first, rest, total).await,
            #[cfg(test)]
            Storage::Fake(s) => s.put(key, content_type, first, rest, total).await,
        }
    }
    pub async fn open(&self, key: &str, range: Option<(u64, u64)>) -> Result<Body, StoreError> {
        let _ = &range;
        match self {
            Storage::Local(l) => l.open(key, range).await,
            #[cfg(feature = "aws")]
            Storage::S3(s) => s.open(key, range).await,
            #[cfg(test)]
            Storage::Fake(s) => s.open(key, range).await,
        }
    }
    /// Idempotent: deleting a missing object is Ok.
    pub async fn delete(&self, key: &str) -> Result<(), StoreError> {
        match self {
            Storage::Local(l) => match tokio::fs::remove_file(l.path(key)?).await { Ok(()) => Ok(()), Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()), Err(e) => Err(e.into()) },
            #[cfg(feature = "aws")]
            Storage::S3(s) => s.delete(key).await,
            #[cfg(test)]
            Storage::Fake(s) => s.delete(key).await,
        }
    }
    pub async fn stat(&self, key: &str) -> Result<Option<u64>, StoreError> {
        match self {
            Storage::Local(l) => match tokio::fs::metadata(l.path(key)?).await { Ok(m) => Ok(Some(m.len())), Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None), Err(e) => Err(e.into()) },
            #[cfg(feature = "aws")]
            Storage::S3(s) => s.stat(key).await,
            #[cfg(test)]
            Storage::Fake(s) => s.stat(key).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "0123456789abcdef";
    fn tmpdir(tag: &str) -> PathBuf { let p = std::env::temp_dir().join(format!("music-storage-{}-{}", tag, random_hex(4).unwrap())); std::fs::create_dir_all(&p).unwrap(); p }
    #[test] fn key_layout_is_fixed() { assert_eq!(object_key("alice", ID, "mp3").unwrap(), "accounts/alice/0123456789abcdef.mp3"); }
    #[test] fn key_roundtrip_and_traversal_rejected() {
        assert_eq!(parse_key("accounts/alice/0123456789abcdef.flac").unwrap(), ("alice", ID, "flac"));
        for bad in ["accounts/../etc/passwd", "accounts/alice/../bob/0123456789abcdef.mp3", "accounts/alice/0123456789abcdef.mp3/x", "/accounts/alice/0123456789abcdef.mp3",
                    "accounts//0123456789abcdef.mp3", "accounts/alice/0123456789abcdeg.mp3", "accounts/alice/0123456789abcdef.exe", "accounts/ali ce/0123456789abcdef.mp3",
                    "accounts/alice/0123456789abcdef", "accounts/Alice/0123456789abcdef.mp3", "other/alice/0123456789abcdef.mp3", "accounts/alice/..%2f0123456789abcdef.mp3", "accounts/alice\0/0123456789abcdef.mp3"] {
            assert_eq!(parse_key(bad), Err(StoreError::BadKey), "{}", bad);
        }
        assert_eq!(object_key("../x", ID, "mp3"), Err(StoreError::BadKey));
        assert_eq!(object_key("a", "short", "mp3"), Err(StoreError::BadKey));
        assert_eq!(object_key("a", ID, "exe"), Err(StoreError::BadKey));
    }
    #[tokio::test] async fn local_put_open_range_stat_delete() {
        let root = tmpdir("l");
        let st = Storage::local(root.clone(), 1000);
        let key = object_key("alice", ID, "mp3").unwrap();
        let data: Vec<u8> = (0..200u8).collect();
        let mut rest: &[u8] = &data[10..];
        st.put(&key, "audio/mpeg", &data[..10], &mut rest, 200).await.unwrap();
        assert_eq!(st.stat(&key).await.unwrap(), Some(200));
        let mut b = st.open(&key, Some((5, 14))).await.unwrap();
        let mut got = Vec::new(); while let Some(c) = b.next_chunk().await { got.extend(c.unwrap()); }
        assert_eq!(got, data[5..15]);
        let mut b = st.open(&key, None).await.unwrap();
        let mut got = Vec::new(); while let Some(c) = b.next_chunk().await { got.extend(c.unwrap()); }
        assert_eq!(got, data);
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(root.join(&key)).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(root.join("accounts/alice")).unwrap().permissions().mode() & 0o777, 0o700);
        st.delete(&key).await.unwrap(); st.delete(&key).await.unwrap();
        assert_eq!(st.stat(&key).await.unwrap(), None);
        assert_eq!(st.open(&key, None).await.err(), Some(StoreError::NotFound));
        let _ = std::fs::remove_dir_all(root);
    }
    #[tokio::test] async fn local_short_body_leaves_nothing_behind() {
        let root = tmpdir("s");
        let st = Storage::local(root.clone(), 1000);
        let key = object_key("alice", ID, "wav").unwrap();
        let mut rest: &[u8] = &[1, 2, 3];
        assert!(st.put(&key, "audio/wav", &[0; 4], &mut rest, 100).await.is_err());
        assert_eq!(st.stat(&key).await.unwrap(), None);
        let leftovers: Vec<_> = std::fs::read_dir(root.join("accounts/alice")).unwrap().collect();
        assert!(leftovers.is_empty(), "temp file must be removed");
        let _ = std::fs::remove_dir_all(root);
    }
    #[tokio::test] async fn local_refuses_traversal_keys() {
        let root = tmpdir("t");
        let st = Storage::local(root.clone(), 1000);
        assert_eq!(st.open("accounts/../../etc/passwd", None).await.err(), Some(StoreError::BadKey));
        assert_eq!(st.delete("../../x").await, Err(StoreError::BadKey));
        let mut rest: &[u8] = &[];
        assert_eq!(st.put("accounts/a/../b/0123456789abcdef.mp3", "x", &[1], &mut rest, 1).await, Err(StoreError::BadKey));
        let _ = std::fs::remove_dir_all(root);
    }
}
