//! End-to-end tests over a real socket (docs/abac.md section 7), run against the local backend and a fake S3.
use crate::api::{test_ctx, Ctx};
use crate::s3store::{FakeS3, S3Store};
use crate::storage::Storage;
use std::{net::SocketAddr, sync::Arc};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::{TcpListener, TcpStream}};

struct Resp { code: u16, headers: String, body: Vec<u8> }
impl Resp {
    fn text(&self) -> String { String::from_utf8_lossy(&self.body).to_string() }
    fn header(&self, name: &str) -> Option<String> { self.headers.lines().find_map(|l| { let (n, v) = l.split_once(':')?; if n.eq_ignore_ascii_case(name) { Some(v.trim().to_string()) } else { None } }) }
}
fn mp3(n: usize) -> Vec<u8> { let mut v = b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec(); v.extend((0..n.saturating_sub(10)).map(|i| (i % 251) as u8)); v }
async fn start(ctx: Ctx) -> (SocketAddr, Arc<Ctx>) {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap(); let addr = l.local_addr().unwrap(); let ctx = Arc::new(ctx);
    tokio::spawn(crate::serve(l, ctx.clone())); (addr, ctx)
}
async fn raw(addr: SocketAddr, head: &str, body: &[u8], half_close: bool) -> Resp {
    let mut s = TcpStream::connect(addr).await.unwrap();
    s.write_all(head.as_bytes()).await.unwrap(); s.write_all(body).await.unwrap();
    if half_close { let _ = s.shutdown().await; }
    let mut buf = Vec::new(); let _ = tokio::time::timeout(std::time::Duration::from_secs(15), s.read_to_end(&mut buf)).await;
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4).unwrap_or(buf.len());
    let head = String::from_utf8_lossy(&buf[..split]).to_string();
    let code = head.split(' ').nth(1).and_then(|c| c.parse().ok()).unwrap_or(0);
    Resp { code, headers: head, body: buf[split..].to_vec() }
}
async fn call(addr: SocketAddr, method: &str, path: &str, token: Option<&str>, extra: &[(&str, &str)], body: Option<&[u8]>) -> Resp {
    let mut h = format!("{} {} HTTP/1.1\r\nHost: t\r\nConnection: close\r\n", method, path);
    if let Some(t) = token { h.push_str(&format!("Authorization: Bearer {}\r\n", t)); }
    for (n, v) in extra { h.push_str(&format!("{}: {}\r\n", n, v)); }
    if let Some(b) = body { h.push_str(&format!("Content-Length: {}\r\n", b.len())); }
    h.push_str("\r\n");
    raw(addr, &h, body.unwrap_or(&[]), false).await
}
fn id_of(r: &Resp) -> String { let t = r.text(); let i = t.find("\"id\":\"").unwrap() + 6; t[i..i + 16].to_string() }

fn local(tag: &str, max: u64) -> (Storage, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("music-e2e-{}-{}", tag, crate::storage::random_hex(4).unwrap()));
    (Storage::local(root.join("objects"), max), root)
}
fn fake(part: usize, max: u64) -> Storage { Storage::Fake(S3Store { client: FakeS3::default(), bucket: "bkt".into(), region: "r1".into(), encrypted: true, max_object_bytes: max, part_size: part }) }
const ACCTS: [(&str, &str, &str, u64); 4] = [("alice", "uploader", "tok-alice", 8000), ("bob", "listener", "tok-bob", 0), ("carol", "uploader", "tok-carol", 8000), ("susp1", "uploader", "tok-susp", 8000)];

/// The same scenario must hold for every backend.
async fn scenario(storage: Storage) {
    let (addr, ctx) = start(test_ctx(storage, ACCTS.to_vec(), 6000)).await;
    assert_eq!(call(addr, "GET", "/api/tracks", None, &[], None).await.code, 401);
    assert_eq!(call(addr, "GET", "/api/tracks", Some("wrong"), &[], None).await.code, 401);
    let data = mp3(3000);
    // upload (client lies about the content type: ignored) -> 201
    let up = call(addr, "POST", "/api/tracks?title=Hello%20There&artist=Me&visibility=private", Some("tok-alice"), &[("Content-Type", "text/html")], Some(&data)).await;
    assert_eq!(up.code, 201, "{}", up.text());
    assert!(up.text().contains("\"title\":\"Hello There\"") && up.text().contains("\"ownerId\":\"alice\"") && up.text().contains("\"format\":\"mp3\"") && up.text().contains("\"mine\":true"));
    let id = id_of(&up);
    // listed for the owner, invisible to others (private)
    assert!(call(addr, "GET", "/api/tracks", Some("tok-alice"), &[], None).await.text().contains(&id));
    assert!(!call(addr, "GET", "/api/tracks", Some("tok-bob"), &[], None).await.text().contains(&id));
    for p in ["", "/stream", "/cover"] { assert_eq!(call(addr, "GET", &format!("/api/tracks/{}{}", id, p), Some("tok-bob"), &[], None).await.code, 404, "private track must not leak ({})", p); }
    // stream with Range -> 206, exact bytes, nosniff, audio/mpeg
    let r = call(addr, "GET", &format!("/api/tracks/{}/stream", id), Some("tok-alice"), &[("Range", "bytes=10-19")], None).await;
    assert_eq!(r.code, 206); assert_eq!(r.body, data[10..20]); assert_eq!(r.header("Content-Range").unwrap(), "bytes 10-19/3000");
    assert_eq!(r.header("Content-Type").unwrap(), "audio/mpeg"); assert_eq!(r.header("Accept-Ranges").unwrap(), "bytes");
    let full = call(addr, "GET", &format!("/api/tracks/{}/stream", id), Some("tok-alice"), &[], None).await;
    assert_eq!((full.code, full.body.len()), (200, 3000)); assert_eq!(full.body, data);
    assert_eq!(call(addr, "GET", &format!("/api/tracks/{}/stream", id), Some("tok-alice"), &[("Range", "bytes=3000-")], None).await.code, 416);
    assert_eq!(call(addr, "HEAD", &format!("/api/tracks/{}/stream", id), Some("tok-alice"), &[], None).await.code, 200);
    // /api/me reflects usage
    let me = call(addr, "GET", "/api/me", Some("tok-alice"), &[], None).await.text();
    assert!(me.contains("\"usedBytes\":3000") && me.contains("\"quotaBytes\":8000") && me.contains("\"can\":{\"upload\":true}") && me.contains("\"maxUploadBytes\":6000"), "{}", me);
    assert!(call(addr, "GET", "/api/me", Some("tok-bob"), &[], None).await.text().contains("\"can\":{\"upload\":false}"));
    // making it public lets bob read but not edit/delete; carol cannot patch it either
    let pub_ = call(addr, "PATCH", &format!("/api/tracks/{}?visibility=public", id), Some("tok-alice"), &[], None).await;
    assert_eq!(pub_.code, 200); assert!(pub_.text().contains("\"visibility\":\"public\""));
    assert_eq!(call(addr, "GET", &format!("/api/tracks/{}/stream", id), Some("tok-bob"), &[("Range", "bytes=0-3")], None).await.code, 206);
    assert_eq!(call(addr, "DELETE", &format!("/api/tracks/{}", id), Some("tok-bob"), &[], None).await.code, 403);
    assert_eq!(call(addr, "PATCH", &format!("/api/tracks/{}?title=x", id), Some("tok-carol"), &[], None).await.code, 403);
    assert_eq!(call(addr, "PATCH", &format!("/api/tracks/{}?visibility=nope", id), Some("tok-alice"), &[], None).await.code, 400);
    // listener cannot upload; unsupported media; length required
    assert_eq!(call(addr, "POST", "/api/tracks", Some("tok-bob"), &[], Some(&data)).await.text(), "{\"error\":\"forbidden\"}");
    let bad = call(addr, "POST", "/api/tracks", Some("tok-alice"), &[], Some(b"this is definitely not an audio file")).await;
    assert_eq!((bad.code, bad.text().as_str()), (415, "{\"error\":\"unsupported_media\"}"));
    assert_eq!(raw(addr, "POST /api/tracks HTTP/1.1\r\nHost: t\r\nConnection: close\r\nAuthorization: Bearer tok-alice\r\n\r\n", b"", false).await.code, 411);
    assert_eq!(raw(addr, "POST /api/tracks HTTP/1.1\r\nHost: t\r\nConnection: close\r\nAuthorization: Bearer tok-alice\r\nTransfer-Encoding: chunked\r\n\r\n", b"0\r\n\r\n", false).await.code, 411);
    // oversize (max 6000) -> 413 before reading the body
    let big = call(addr, "POST", "/api/tracks", Some("tok-alice"), &[], Some(&mp3(6001))).await;
    assert_eq!((big.code, big.text().as_str()), (413, "{\"error\":\"upload_too_large\"}"));
    // quota: alice has 8000, used 3000 -> a second 3000 fits (6000), a third does not
    let second = call(addr, "POST", "/api/tracks", Some("tok-alice"), &[], Some(&mp3(3000))).await; assert_eq!(second.code, 201);
    let third = call(addr, "POST", "/api/tracks", Some("tok-alice"), &[], Some(&mp3(3000))).await;
    assert_eq!((third.code, third.text().as_str()), (403, "{\"error\":\"quota_exceeded\"}"));
    // nothing was committed for the failed attempts, and no reservation leaked
    assert_eq!(ctx.index.lock().await.committed_and_reserved("alice"), 6000);
    // delete frees the quota and removes the object; the track is gone for everyone
    let key = ctx.index.lock().await.get(&id).unwrap().key.clone();
    assert_eq!(ctx.storage.stat(&key).await.unwrap(), Some(3000));
    assert_eq!(call(addr, "DELETE", &format!("/api/tracks/{}", id), Some("tok-alice"), &[], None).await.code, 204);
    assert_eq!(ctx.storage.stat(&key).await.unwrap(), None, "current object removed");
    assert_eq!(call(addr, "GET", &format!("/api/tracks/{}/stream", id), Some("tok-alice"), &[], None).await.code, 404);
    assert_eq!(call(addr, "POST", "/api/tracks", Some("tok-alice"), &[], Some(&mp3(3000))).await.code, 201);
    // suspended: 403 forbidden everywhere except GET /api/me, which reports the status
    for (m, p) in [("GET", "/api/tracks"), ("GET", "/api/tracks/aurora-lines/stream"), ("DELETE", "/api/tracks/0123456789abcdef"), ("PATCH", "/api/tracks/0123456789abcdef?title=x"), ("GET", "/api/tracks/0123456789abcdef")] {
        assert_eq!(call(addr, m, p, Some("tok-susp"), &[], None).await.text(), "{\"error\":\"forbidden\"}", "{} {}", m, p);
    }
    assert_eq!(call(addr, "POST", "/api/tracks", Some("tok-susp"), &[], Some(&mp3(100))).await.code, 403);
    let sme = call(addr, "GET", "/api/me", Some("tok-susp"), &[], None).await; assert_eq!(sme.code, 200); assert!(sme.text().contains("\"status\":\"suspended\"") && sme.text().contains("\"can\":{\"upload\":false}"));
    // the bootstrap admin sees and may delete anything
    let adm_list = call(addr, "GET", "/api/tracks", Some("boot-token"), &[], None).await.text(); assert!(adm_list.matches("\"ownerId\":\"alice\"").count() == 2);
}

#[tokio::test] async fn scenario_local_backend() { let (st, root) = local("s", 6000); scenario(st).await; let _ = std::fs::remove_dir_all(root); }
#[tokio::test] async fn scenario_s3_backend_single_put() { scenario(fake(1 << 20, 6000)).await; }
#[tokio::test] async fn scenario_s3_backend_multipart() { scenario(fake(1000, 6000)).await; }

fn fake_of(ctx: &Ctx) -> &FakeS3 { match &ctx.storage { Storage::Fake(s) => &s.client, _ => unreachable!() } }

#[tokio::test] async fn s3_key_layout_content_type_from_sniff_and_multipart() {
    let (addr, ctx) = start(test_ctx(fake(1000, 6000), ACCTS.to_vec(), 6000)).await;
    let data = mp3(2500);
    let r = call(addr, "POST", "/api/tracks", Some("tok-alice"), &[("Content-Type", "application/x-evil")], Some(&data)).await; assert_eq!(r.code, 201);
    let id = id_of(&r); let key = format!("accounts/alice/{}.mp3", id);
    let (bytes, ct) = fake_of(&ctx).object(&key).expect("object stored under the contract key");
    assert_eq!((bytes, ct.as_str()), (data, "audio/mpeg"));
    let st = fake_of(&ctx).state.lock().unwrap(); assert_eq!((st.completed, st.put_calls, st.uploads.len(), st.objects.len()), (1, 0, 0, 1));
}

#[tokio::test] async fn dropped_upload_aborts_multipart_and_commits_nothing() {
    let (addr, ctx) = start(test_ctx(fake(1000, 6000), ACCTS.to_vec(), 6000)).await;
    let head = "POST /api/tracks HTTP/1.1\r\nHost: t\r\nConnection: close\r\nAuthorization: Bearer tok-alice\r\nContent-Length: 3000\r\n\r\n";
    let r = raw(addr, head, &mp3(1500), true).await; // body ends early, client half-closes
    assert_ne!(r.code, 201);
    assert_eq!(fake_of(&ctx).open_uploads(), 0, "no orphan multipart upload");
    let (aborted, objects) = { let s = fake_of(&ctx).state.lock().unwrap(); (s.aborted, s.objects.len()) };
    assert_eq!((aborted, objects), (1, 0));
    let ix = ctx.index.lock().await; assert!(ix.rows().is_empty()); assert_eq!(ix.committed_and_reserved("alice"), 0, "reservation released");
}

#[tokio::test] async fn rejected_uploads_never_touch_storage() {
    let (addr, ctx) = start(test_ctx(fake(1000, 6000), ACCTS.to_vec(), 6000)).await;
    assert_eq!(call(addr, "POST", "/api/tracks", Some("tok-alice"), &[], Some(b"not audio at all, just text")).await.code, 415);
    assert_eq!(call(addr, "POST", "/api/tracks", Some("tok-bob"), &[], Some(&mp3(100))).await.code, 403);
    assert_eq!(call(addr, "POST", "/api/tracks", Some("tok-alice"), &[], Some(&mp3(7000))).await.code, 413);
    let st = fake_of(&ctx).state.lock().unwrap(); assert_eq!((st.next, st.put_calls, st.objects.len()), (0, 0, 0));
}

#[tokio::test] async fn parallel_uploads_cannot_overshoot_the_quota() {
    // quota 8000, each upload 3000 -> at most two of five concurrent uploads may succeed
    let (addr, ctx) = start(test_ctx(fake(1000, 6000), ACCTS.to_vec(), 6000)).await;
    let mut tasks = vec![];
    for _ in 0..5 { tasks.push(tokio::spawn(async move { call(addr, "POST", "/api/tracks", Some("tok-alice"), &[], Some(&mp3(3000))).await.code })); }
    let mut ok = 0; for t in tasks { if t.await.unwrap() == 201 { ok += 1; } }
    assert_eq!(ok, 2); assert_eq!(ctx.index.lock().await.used_bytes("alice"), 6000);
}

#[tokio::test] async fn builtin_tracks_are_public_and_read_only() {
    let mut ctx = test_ctx(fake(1000, 6000), ACCTS.to_vec(), 6000);
    let dir = std::env::temp_dir().join(format!("music-e2e-lib-{}", crate::storage::random_hex(4).unwrap()));
    ctx.tracks = crate::library::ensure_library(&dir).unwrap();
    let (addr, _ctx) = start(ctx).await;
    let list = call(addr, "GET", "/api/tracks", Some("tok-bob"), &[], None).await.text();
    assert!(list.contains("aurora-lines") && list.contains("\"ownerId\":\"system\"") && list.contains("\"mine\":false"));
    assert_eq!(call(addr, "GET", "/api/tracks/aurora-lines/stream", Some("tok-bob"), &[("Range", "bytes=0-99")], None).await.code, 206);
    assert_eq!(call(addr, "DELETE", "/api/tracks/aurora-lines", Some("boot-token"), &[], None).await.code, 403);
    assert_eq!(call(addr, "PATCH", "/api/tracks/aurora-lines?title=x", Some("boot-token"), &[], None).await.code, 403);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test] async fn health_cors_and_method_rules() {
    let (addr, _ctx) = start(test_ctx(fake(1000, 6000), ACCTS.to_vec(), 6000)).await;
    assert_eq!(call(addr, "GET", "/api/health", None, &[], None).await.text(), "{\"status\":\"ok\"}");
    assert_eq!(call(addr, "PUT", "/api/tracks", Some("tok-alice"), &[], None).await.code, 405);
    let pre = call(addr, "OPTIONS", "/api/tracks", None, &[("Origin", "http://localhost:5173")], None).await;
    assert_eq!(pre.code, 204); assert!(pre.header("Access-Control-Allow-Methods").unwrap().contains("POST")); assert_eq!(pre.header("Access-Control-Allow-Origin").unwrap(), "http://localhost:5173");
    assert!(call(addr, "OPTIONS", "/api/tracks", None, &[("Origin", "http://evil.example")], None).await.header("Access-Control-Allow-Origin").is_none());
}
