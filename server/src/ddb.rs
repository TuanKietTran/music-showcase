//! DynamoDB persistence for the upload index: table `music-uploads`, partition key `ownerId` (S), sort key `id` (S).
//! The server keeps the full index in memory and writes through (see index.rs); the table is the durable copy.
use crate::index::{Change, UploadRow};
use aws_sdk_dynamodb::{error::DisplayErrorContext, types::AttributeValue as A, Client};
use std::{collections::HashMap, io};

pub struct DdbPersist { client: Client, table: String }

fn io_err<E: std::error::Error + Send + Sync + 'static>(e: E) -> io::Error { io::Error::other(DisplayErrorContext(&e).to_string()) }
fn s(item: &HashMap<String, A>, k: &str) -> io::Result<String> { item.get(k).and_then(|v| v.as_s().ok()).cloned().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("item missing '{}'", k))) }
fn n(item: &HashMap<String, A>, k: &str) -> io::Result<u64> { item.get(k).and_then(|v| v.as_n().ok()).and_then(|v| v.parse().ok()).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("item missing number '{}'", k))) }

impl DdbPersist {
    pub async fn connect(table: &str) -> DdbPersist {
        let conf = aws_config::defaults(aws_config::BehaviorVersion::latest()).load().await; // region + credentials from the default chain
        DdbPersist { client: Client::new(&conf), table: table.to_string() }
    }
    pub async fn load(&self) -> io::Result<Vec<UploadRow>> {
        let mut rows = Vec::new(); let mut start: Option<HashMap<String, A>> = None;
        loop {
            let out = self.client.scan().table_name(&self.table).consistent_read(true).set_exclusive_start_key(start.take()).send().await.map_err(io_err)?;
            for it in out.items() {
                rows.push(UploadRow { id: s(it, "id")?, owner_id: s(it, "ownerId")?, visibility: s(it, "visibility")?, title: s(it, "title")?, artist: s(it, "artist")?, key: s(it, "key")?, format: s(it, "format")?, bytes: n(it, "bytes")?, duration_sec: n(it, "durationSec")?, created_at: n(it, "createdAt")? });
            }
            match out.last_evaluated_key() { Some(k) if !k.is_empty() => start = Some(k.clone()), _ => break }
        }
        rows.sort_by_key(|r| r.created_at);
        Ok(rows)
    }
    pub async fn flush(&self, change: Change<'_>) -> io::Result<()> {
        match change {
            Change::Put(r) => {
                self.client.put_item().table_name(&self.table)
                    .item("ownerId", A::S(r.owner_id.clone())).item("id", A::S(r.id.clone())).item("visibility", A::S(r.visibility.clone())).item("title", A::S(r.title.clone())).item("artist", A::S(r.artist.clone()))
                    .item("key", A::S(r.key.clone())).item("format", A::S(r.format.clone())).item("bytes", A::N(r.bytes.to_string())).item("durationSec", A::N(r.duration_sec.to_string())).item("createdAt", A::N(r.created_at.to_string()))
                    .send().await.map_err(io_err)?;
            }
            Change::Delete(r) => { self.client.delete_item().table_name(&self.table).key("ownerId", A::S(r.owner_id.clone())).key("id", A::S(r.id.clone())).send().await.map_err(io_err)?; }
        }
        Ok(())
    }
}
