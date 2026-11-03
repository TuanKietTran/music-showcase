//! Upload index (docs/abac.md section 4). The whole index is held in memory (the server is the only writer; run one task)
//! and written through to a persistence backend: `uploads.tsv` (atomic rewrite) or DynamoDB table `music-uploads`
//! (partition key `ownerId`, sort key `id`). "Used bytes of an account" is therefore a cheap in-memory sum.
use std::{collections::HashMap, io, path::PathBuf};

#[derive(Clone, Debug, PartialEq)]
pub struct UploadRow {
    pub id: String, pub owner_id: String, pub visibility: String, pub title: String, pub artist: String,
    pub key: String, pub format: String, pub bytes: u64, pub duration_sec: u64, pub created_at: u64,
}

pub enum Change<'a> { Put(&'a UploadRow), Delete(&'a UploadRow) }

pub enum Persist {
    File(PathBuf),
    #[cfg(feature = "aws")]
    Ddb(crate::ddb::DdbPersist),
    #[cfg(test)]
    Mem,
    #[cfg(test)]
    Failing,
}

fn bad(msg: String) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, msg) }

pub fn parse_tsv(text: &str) -> io::Result<Vec<UploadRow>> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') { continue; }
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 10 { return Err(bad(format!("uploads.tsv line {}: expected 10 fields", i + 1))); }
        let num = |s: &str, what: &str| s.parse::<u64>().map_err(|_| bad(format!("uploads.tsv line {}: bad {}", i + 1, what)));
        out.push(UploadRow { id: f[0].into(), owner_id: f[1].into(), visibility: f[2].into(), title: f[3].into(), artist: f[4].into(), key: f[5].into(), format: f[6].into(), bytes: num(f[7], "bytes")?, duration_sec: num(f[8], "durationSec")?, created_at: num(f[9], "createdAt")? });
    }
    Ok(out)
}
pub fn to_tsv(rows: &[UploadRow]) -> String {
    rows.iter().map(|r| format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n", r.id, r.owner_id, r.visibility, r.title, r.artist, r.key, r.format, r.bytes, r.duration_sec, r.created_at)).collect()
}

impl Persist {
    async fn load(&self) -> io::Result<Vec<UploadRow>> {
        match self {
            Persist::File(p) => match tokio::fs::read_to_string(p).await { Ok(t) => parse_tsv(&t), Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(vec![]), Err(e) => Err(e) },
            #[cfg(feature = "aws")]
            Persist::Ddb(d) => d.load().await,
            #[cfg(test)]
            Persist::Mem | Persist::Failing => Ok(vec![]),
        }
    }
    async fn flush(&self, rows: &[UploadRow], change: Change<'_>) -> io::Result<()> {
        let _ = &change;
        match self {
            Persist::File(p) => {
                let tmp = p.with_extension("tsv.tmp");
                let _ = tokio::fs::remove_file(&tmp).await;
                let mut f = tokio::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp).await?;
                tokio::io::AsyncWriteExt::write_all(&mut f, to_tsv(rows).as_bytes()).await?;
                drop(f);
                tokio::fs::rename(&tmp, p).await
            }
            #[cfg(feature = "aws")]
            Persist::Ddb(d) => d.flush(change).await,
            #[cfg(test)]
            Persist::Mem => Ok(()),
            #[cfg(test)]
            Persist::Failing => Err(io::Error::other("injected index failure")),
        }
    }
}

pub struct Index { rows: Vec<UploadRow>, reserved: HashMap<String, u64>, persist: Persist }

impl Index {
    pub async fn open(persist: Persist) -> io::Result<Index> { let rows = persist.load().await?; Ok(Index { rows, reserved: HashMap::new(), persist }) }
    pub fn rows(&self) -> &[UploadRow] { &self.rows }
    pub fn get(&self, id: &str) -> Option<&UploadRow> { self.rows.iter().find(|r| r.id == id) }
    /// Committed bytes of one account.
    pub fn used_bytes(&self, owner: &str) -> u64 { self.rows.iter().filter(|r| r.owner_id == owner).map(|r| r.bytes).sum() }
    /// Committed plus in-flight (reserved) bytes: what quota decisions must use so parallel uploads cannot overshoot.
    pub fn committed_and_reserved(&self, owner: &str) -> u64 { self.used_bytes(owner) + self.reserved.get(owner).copied().unwrap_or(0) }
    pub fn reserve(&mut self, owner: &str, bytes: u64) { *self.reserved.entry(owner.to_string()).or_insert(0) += bytes; }
    pub fn release(&mut self, owner: &str, bytes: u64) {
        if let Some(v) = self.reserved.get_mut(owner) { *v = v.saturating_sub(bytes); if *v == 0 { self.reserved.remove(owner); } }
    }
    pub async fn insert(&mut self, row: UploadRow) -> io::Result<()> {
        self.rows.push(row);
        let r = self.rows.last().cloned().unwrap();
        if let Err(e) = self.persist.flush(&self.rows, Change::Put(&r)).await { self.rows.pop(); return Err(e); }
        Ok(())
    }
    pub async fn update(&mut self, id: &str, f: impl FnOnce(&mut UploadRow)) -> io::Result<Option<UploadRow>> {
        let i = match self.rows.iter().position(|r| r.id == id) { Some(i) => i, None => return Ok(None) };
        let old = self.rows[i].clone(); f(&mut self.rows[i]);
        let new = self.rows[i].clone();
        if let Err(e) = self.persist.flush(&self.rows, Change::Put(&new)).await { self.rows[i] = old; return Err(e); }
        Ok(Some(new))
    }
    pub async fn remove(&mut self, id: &str) -> io::Result<Option<UploadRow>> {
        let i = match self.rows.iter().position(|r| r.id == id) { Some(i) => i, None => return Ok(None) };
        let old = self.rows.remove(i);
        if let Err(e) = self.persist.flush(&self.rows, Change::Delete(&old)).await { self.rows.insert(i, old); return Err(e); }
        Ok(Some(old))
    }
}

#[cfg(test)]
pub fn mem() -> Index { Index { rows: vec![], reserved: HashMap::new(), persist: Persist::Mem } }
#[cfg(test)]
pub fn failing() -> Index { Index { rows: vec![], reserved: HashMap::new(), persist: Persist::Failing } }

#[cfg(test)]
mod tests {
    use super::*;
    pub fn row(id: &str, owner: &str, bytes: u64) -> UploadRow { UploadRow { id: id.into(), owner_id: owner.into(), visibility: "private".into(), title: "T".into(), artist: "A".into(), key: format!("accounts/{}/{}.mp3", owner, id), format: "mp3".into(), bytes, duration_sec: 0, created_at: 1 } }
    #[test] fn tsv_roundtrip_and_errors() {
        let rows = vec![row("0123456789abcdef", "a", 5), row("fedcba9876543210", "b", 7)];
        assert_eq!(parse_tsv(&to_tsv(&rows)).unwrap(), rows);
        assert!(parse_tsv("a\tb\n").unwrap_err().to_string().contains("line 1"));
        assert!(parse_tsv(&to_tsv(&rows).replace("\t5\t", "\tx\t")).unwrap_err().to_string().contains("bad bytes"));
    }
    #[tokio::test] async fn used_bytes_sum_and_reservations() {
        let mut ix = mem();
        ix.insert(row("0123456789abcdef", "a", 10)).await.unwrap(); ix.insert(row("1123456789abcdef", "a", 5)).await.unwrap(); ix.insert(row("2123456789abcdef", "b", 100)).await.unwrap();
        assert_eq!((ix.used_bytes("a"), ix.used_bytes("b"), ix.used_bytes("c")), (15, 100, 0));
        ix.reserve("a", 20); assert_eq!((ix.used_bytes("a"), ix.committed_and_reserved("a")), (15, 35));
        ix.release("a", 20); assert_eq!(ix.committed_and_reserved("a"), 15);
        ix.release("a", 99); assert_eq!(ix.committed_and_reserved("a"), 15); // never underflows
        ix.remove("0123456789abcdef").await.unwrap(); assert_eq!(ix.used_bytes("a"), 5);
    }
    #[tokio::test] async fn failed_persist_rolls_back_memory() {
        let mut ix = failing();
        assert!(ix.insert(row("0123456789abcdef", "a", 10)).await.is_err());
        assert!(ix.rows().is_empty());
        let mut ix2 = mem(); ix2.insert(row("0123456789abcdef", "a", 10)).await.unwrap(); ix2.persist = Persist::Failing;
        assert!(ix2.remove("0123456789abcdef").await.is_err()); assert_eq!(ix2.rows().len(), 1);
        assert!(ix2.update("0123456789abcdef", |r| r.title = "X".into()).await.is_err()); assert_eq!(ix2.get("0123456789abcdef").unwrap().title, "T");
    }
    #[tokio::test] async fn file_persist_is_atomic_and_reloads() {
        let dir = std::env::temp_dir().join(format!("music-index-{}", crate::storage::random_hex(4).unwrap())); std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("uploads.tsv");
        let mut ix = Index::open(Persist::File(p.clone())).await.unwrap();
        ix.insert(row("0123456789abcdef", "a", 10)).await.unwrap();
        ix.update("0123456789abcdef", |r| r.visibility = "public".into()).await.unwrap();
        let again = Index::open(Persist::File(p.clone())).await.unwrap();
        assert_eq!(again.rows().len(), 1); assert_eq!(again.rows()[0].visibility, "public");
        use std::os::unix::fs::PermissionsExt; assert_eq!(std::fs::metadata(&p).unwrap().permissions().mode() & 0o777, 0o600);
        assert!(!dir.join("uploads.tsv.tmp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
