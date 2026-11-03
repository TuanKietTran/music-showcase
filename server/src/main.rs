mod accounts; mod api; mod auth; mod http; mod index; mod library; mod media; mod policy; mod range; mod s3store; mod storage;
#[cfg(feature = "aws")]
mod cognito;
#[cfg(feature = "aws")]
mod ddb;
#[cfg(feature = "aws")]
mod s3_sdk;
use accounts::{parse_accounts, sha256_hex, Accounts};
use api::{cors_headers, json_reply, route, write_reply, Config, Ctx};
use http::parse_request_head;
use library::{ensure_library, load_dir};
use policy::Policy;
use std::{path::PathBuf, sync::Arc, time::Duration};
use storage::Storage;
use tokio::{io::{AsyncBufReadExt, BufReader}, net::{TcpListener, TcpStream}, sync::{Mutex, Semaphore}};

async fn read_head(reader: &mut BufReader<TcpStream>) -> Result<Option<Vec<u8>>, ()> {
    let fut = async {
        let mut head: Vec<u8> = Vec::new(); let mut lines = 0usize;
        loop {
            let mut line: Vec<u8> = Vec::new();
            let n = reader.read_until(b'\n', &mut line).await.map_err(|_| ())?;
            if n == 0 { return if head.is_empty() { Ok(None) } else { Err(()) }; }
            if lines == 0 && line.len() > http::MAX_REQUEST_LINE + 2 { return Err(()); }
            lines += 1; if lines > http::MAX_HEADERS + 1 { return Err(()); }
            let done = line == b"\r\n" || line == b"\n";
            head.extend_from_slice(&line);
            if head.len() > 65536 { return Err(()); }
            if done { break; }
        }
        Ok(Some(head))
    };
    match tokio::time::timeout(Duration::from_secs(10), fut).await { Ok(r) => r, Err(_) => Err(()) }
}

async fn handle(mut reader: BufReader<TcpStream>, ctx: Arc<Ctx>) {
    loop {
        let head = match read_head(&mut reader).await { Ok(Some(h)) => h, Ok(None) => return, Err(_) => { let _ = write_reply(reader.get_mut(), json_reply(400, "{\"error\":\"bad_request\"}", vec![]), false).await; return; } };
        let req = match parse_request_head(&head) { Ok(r) => r, Err(_) => { let _ = write_reply(reader.get_mut(), json_reply(400, "{\"error\":\"bad_request\"}", vec![]), false).await; return; } };
        let keep = if req.version == "HTTP/1.1" { !matches!(req.header("Connection"), Some(v) if v.eq_ignore_ascii_case("close")) } else { matches!(req.header("Connection"), Some(v) if v.eq_ignore_ascii_case("keep-alive")) };
        let cors = cors_headers(&req, &ctx.cfg);
        let head_only = req.method == "HEAD";
        let reply = if req.method == "OPTIONS" && req.path().starts_with("/api") {
            let mut h = cors.clone();
            h.push(("Access-Control-Allow-Methods".into(), "GET, HEAD, POST, PATCH, DELETE, OPTIONS".into())); h.push(("Access-Control-Allow-Headers".into(), "Authorization, Range, Content-Type".into()));
            h.push(("Access-Control-Max-Age".into(), "86400".into())); h.push(("Content-Length".into(), "0".into()));
            api::Reply { code: 204, headers: h, payload: api::Payload::None, close: false }
        } else { route(&ctx, &req, &mut reader, &cors).await };
        let close = reply.close;
        if write_reply(reader.get_mut(), reply, head_only).await.is_err() { return; }
        if close {
            // Lingering close: tell the client we are done, then discard (a bounded amount of) the unread request body,
            // so an early 403/413/415 reaches the client instead of being replaced by a TCP reset.
            let _ = tokio::io::AsyncWriteExt::shutdown(reader.get_mut()).await;
            let _ = tokio::time::timeout(Duration::from_secs(3), tokio::io::copy(&mut tokio::io::AsyncReadExt::take(&mut reader, 8 * 1024 * 1024), &mut tokio::io::sink())).await;
            return;
        }
        if !keep { return; }
    }
}

pub(crate) async fn serve(listener: TcpListener, ctx: Arc<Ctx>) {
    let sem = Arc::new(Semaphore::new(256));
    loop {
        let (stream, _peer) = match listener.accept().await { Ok(v) => v, Err(_) => continue };
        let permit = match sem.clone().try_acquire_owned() { Ok(p) => p, Err(_) => { drop(stream); continue; } };
        let c = ctx.clone();
        tokio::spawn(async move { let _permit = permit; handle(BufReader::new(stream), c).await; });
    }
}

fn fatal(msg: &str) -> ! { eprintln!("error: {}", msg); std::process::exit(1) }
fn env(name: &str) -> Option<String> { std::env::var(name).ok().filter(|v| !v.is_empty()) }
fn env_u64(name: &str, default: u64) -> u64 { match env(name) { None => default, Some(v) => v.parse().unwrap_or_else(|_| fatal(&format!("{} must be a number", name))) } }

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    if std::env::args().nth(1).as_deref() == Some("hash-token") { // token comes from stdin so it never appears in argv or shell history
        let mut s = String::new(); let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut s);
        println!("{}", sha256_hex(s.trim_end_matches(['\n', '\r']).as_bytes())); return;
    }
    if std::env::args().nth(1).as_deref() == Some("healthcheck") { // for container health checks in images without a shell or curl
        let port: u16 = env("MUSIC_PORT").and_then(|v| v.parse().ok()).or_else(|| env("PORT").and_then(|v| v.parse().ok())).unwrap_or(8080);
        let ok = async {
            let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.ok()?;
            tokio::io::AsyncWriteExt::write_all(&mut s, b"GET /api/health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n").await.ok()?;
            let mut buf = Vec::new(); tokio::io::AsyncReadExt::read_to_end(&mut s, &mut buf).await.ok()?;
            Some(buf.starts_with(b"HTTP/1.1 200"))
        };
        let healthy = matches!(tokio::time::timeout(Duration::from_secs(3), ok).await, Ok(Some(true)));
        std::process::exit(if healthy { 0 } else { 1 });
    }
    let token = env("MUSIC_TOKEN").unwrap_or_else(|| { eprintln!("error: MUSIC_TOKEN env var is required (no default)"); std::process::exit(2) });
    let origin = env("MUSIC_ORIGIN").unwrap_or_else(|| "http://localhost:5173".to_string());
    let port: u16 = env("MUSIC_PORT").and_then(|v| v.parse().ok()).or_else(|| env("PORT").and_then(|v| v.parse().ok())).unwrap_or(8080);
    let bind = env("MUSIC_BIND").unwrap_or_else(|| "127.0.0.1".to_string());
    let data = PathBuf::from(env("MUSIC_DATA_DIR").unwrap_or_else(|| "./data".to_string()));
    let max_upload = env_u64("MUSIC_MAX_UPLOAD_BYTES", 104_857_600);
    if let Err(e) = std::fs::create_dir_all(&data) { fatal(&format!("cannot create data dir: {}", e)); }
    let tracks = match env("MUSIC_MEDIA_DIR") {
        Some(v) => load_dir(&PathBuf::from(v)).unwrap_or_else(|e| fatal(&format!("cannot load media library: {}", e))),
        None => ensure_library(&data).unwrap_or_else(|e| fatal(&format!("cannot seed library: {}", e))),
    };
    let policy = match env("MUSIC_POLICY_FILE") {
        Some(p) => Policy::parse(&std::fs::read_to_string(&p).unwrap_or_else(|e| fatal(&format!("cannot read policy file: {}", e)))).unwrap_or_else(|e| fatal(&format!("policy file: {}", e))),
        None => Policy::default_policy(),
    };
    let entries = match std::fs::read_to_string(data.join("accounts.tsv")) {
        Ok(t) => parse_accounts(&t).unwrap_or_else(|e| fatal(&format!("accounts.tsv: {}", e))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec![],
        Err(e) => fatal(&format!("cannot read accounts.tsv: {}", e)),
    };
    let accounts = Accounts::new(&token, entries);
    let storage = match env("MUSIC_STORAGE").as_deref().unwrap_or("local") {
        "local" => Storage::local(data.join("objects"), max_upload),
        #[cfg(feature = "aws")]
        "s3" => Storage::S3(s3_sdk::connect(max_upload).await.unwrap_or_else(|e| fatal(&e))),
        #[cfg(not(feature = "aws"))]
        "s3" => fatal("s3 backend not available in this build (rebuild with --features aws)"),
        other => fatal(&format!("unknown MUSIC_STORAGE '{}' (local|s3)", other)),
    };
    let persist = match env("MUSIC_DDB_TABLE") {
        #[cfg(feature = "aws")]
        Some(table) => index::Persist::Ddb(ddb::DdbPersist::connect(&table).await),
        #[cfg(not(feature = "aws"))]
        Some(_) => fatal("MUSIC_DDB_TABLE needs a build with --features aws"),
        None => index::Persist::File(data.join("uploads.tsv")),
    };
    let index = index::Index::open(persist).await.unwrap_or_else(|e| fatal(&format!("cannot load upload index: {}", e)));
    let auth = auth::Auth { accounts, #[cfg(feature = "aws")] oidc: cognito::Oidc::from_env() };
    let cfg = Config { origin, max_upload_bytes: max_upload, trust_proxy: env("MUSIC_TRUST_PROXY").as_deref() == Some("1"), upload_timeout: Duration::from_secs(env_u64("MUSIC_UPLOAD_TIMEOUT_SECS", 900)) };
    let ctx = Arc::new(Ctx { cfg, tracks, policy, auth, storage, index: Mutex::new(index), slots: Semaphore::new(env_u64("MUSIC_MAX_CONCURRENT_UPLOADS", 4) as usize) });
    let listener = TcpListener::bind((bind.as_str(), port)).await.unwrap_or_else(|e| fatal(&format!("bind {}:{}: {}", bind, port, e)));
    let a = ctx.storage.attributes();
    println!("music-server listening on http://{}:{} ({} built-in tracks, {} uploads, storage={}, {} policy rules)", bind, port, ctx.tracks.len(), ctx.index.lock().await.rows().len(), a.backend, ctx.policy.rule_count());
    serve(listener, ctx).await;
}

#[cfg(test)]
mod tests;
