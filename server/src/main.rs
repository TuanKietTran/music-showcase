mod http; mod library; mod range;
use http::{constant_time_eq, parse_request_head, Request};
use library::{ensure_library, json_escape, Track};
use std::{io::Seek, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};
use tokio::{io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader}, net::{TcpListener, TcpStream}, sync::Semaphore};
struct Config { token: String, origin: String }
fn reason(code: u16) -> &'static str { match code { 200=>"OK", 204=>"No Content", 206=>"Partial Content", 304=>"Not Modified", 400=>"Bad Request", 401=>"Unauthorized", 404=>"Not Found", 405=>"Method Not Allowed", 416=>"Range Not Satisfiable", 500=>"Internal Server Error", 503=>"Service Unavailable", _=>"OK" } }
struct Ctx { cfg: Arc<Config>, tracks: Arc<Vec<Track>> }
fn cors_headers(req: &Request, cfg: &Config) -> Vec<(String, String)> {
    match req.header("Origin") { Some(o) if o == cfg.origin => vec![("Access-Control-Allow-Origin".into(), o.to_string()), ("Vary".into(), "Origin".into())], _ => vec![] }
}
async fn write_head<W: AsyncWriteExt + Unpin>(w: &mut W, code: u16, mut headers: Vec<(String, String)>) -> std::io::Result<()> {
    if !headers.iter().any(|(n,_)| n.eq_ignore_ascii_case("X-Content-Type-Options")) { headers.push(("X-Content-Type-Options".into(), "nosniff".into())); }
    let mut s = format!("HTTP/1.1 {} {}\r\n", code, reason(code));
    for (n, v) in &headers { s.push_str(&format!("{}: {}\r\n", n, v)); }
    s.push_str("\r\n"); w.write_all(s.as_bytes()).await
}
async fn send_json<W: AsyncWriteExt + Unpin>(w: &mut W, code: u16, body: &str, extra: Vec<(String,String)>, head_only: bool) -> std::io::Result<()> {
    let mut h = vec![("Content-Type".into(), "application/json".into()), ("Content-Length".into(), body.len().to_string())]; h.extend(extra);
    write_head(w, code, h).await?; if !head_only { w.write_all(body.as_bytes()).await?; } Ok(())
}
fn authorized(req: &Request, cfg: &Config) -> bool {
    match req.header("Authorization") { Some(v) => match v.strip_prefix("Bearer ") { Some(t) => constant_time_eq(t.as_bytes(), cfg.token.as_bytes()), None => false }, None => false }
}
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
async fn handle(mut reader: BufReader<TcpStream>, ctx: Arc<Ctx>, _peer: SocketAddr) {
    loop {
        let head = match read_head(&mut reader).await { Ok(Some(h)) => h, Ok(None) => return, Err(_) => { let w = reader.get_mut(); let _ = send_json(w, 400, "{\"error\":\"bad_request\"}", vec![], false).await; return; } };
        let req = match parse_request_head(&head) { Ok(r) => r, Err(_) => { let w = reader.get_mut(); let _ = send_json(w, 400, "{\"error\":\"bad_request\"}", vec![], false).await; return; } };
        let keep = if req.version == "HTTP/1.1" { !matches!(req.header("Connection"), Some(v) if v.eq_ignore_ascii_case("close")) } else { matches!(req.header("Connection"), Some(v) if v.eq_ignore_ascii_case("keep-alive")) };
        let cors = cors_headers(&req, &ctx.cfg);
        let w = reader.get_mut();
        let path = req.path().to_string(); let method = req.method.clone(); let head_only = method == "HEAD";
        if method == "OPTIONS" && path.starts_with("/api") {
            let mut h = cors.clone(); h.push(("Access-Control-Allow-Methods".into(), "GET, HEAD, OPTIONS".into())); h.push(("Access-Control-Allow-Headers".into(), "Authorization, Range, Content-Type".into())); h.push(("Access-Control-Max-Age".into(), "86400".into())); h.push(("Content-Length".into(), "0".into()));
            if write_head(w, 204, h).await.is_err() { return; }
        } else if !(method == "GET" || method == "HEAD") {
            if send_json(w, 405, "{\"error\":\"method_not_allowed\"}", cors.clone(), false).await.is_err() { return; }
        } else if path == "/api/health" {
            if send_json(w, 200, "{\"status\":\"ok\"}", cors.clone(), head_only).await.is_err() { return; }
        } else if !path.starts_with("/api/") {
            if send_json(w, 404, "{\"error\":\"not_found\"}", cors.clone(), head_only).await.is_err() { return; }
        } else if !authorized(&req, &ctx.cfg) {
            let mut extra = cors.clone(); extra.push(("WWW-Authenticate".into(), "Bearer".into()));
            if send_json(w, 401, "{\"error\":\"unauthorized\"}", extra, head_only).await.is_err() { return; }
        } else if path == "/api/tracks" {
            let mut body = String::from("[");
            for (i, t) in ctx.tracks.iter().enumerate() { if i > 0 { body.push(','); } body.push_str(&format!("{{\"id\":\"{}\",\"title\":\"{}\",\"artist\":\"{}\",\"durationSec\":{},\"sampleRate\":{},\"bitDepth\":16,\"bytes\":{},\"coverUrl\":\"/api/tracks/{}/cover\",\"streamUrl\":\"/api/tracks/{}/stream\"}}", t.id, json_escape(t.title), json_escape(t.artist), t.duration_sec, library::SAMPLE_RATE, t.bytes, t.id, t.id)); }
            body.push(']');
            let mut extra = cors.clone(); extra.push(("Cache-Control".into(), "no-store".into()));
            if send_json(w, 200, &body, extra, head_only).await.is_err() { return; }
        } else if let Some(rest) = path.strip_prefix("/api/tracks/") {
            // rest is "{id}/stream" or "{id}/cover"; id lookup only, never a filesystem path.
            let mut parts = rest.splitn(2, '/'); let id = parts.next().unwrap_or(""); let action = parts.next().unwrap_or("");
            let track = ctx.tracks.iter().find(|t| t.id == id);
            match (track, action) {
                (Some(t), "cover") => {
                    let data = match tokio::fs::read(&t.cover).await { Ok(d) => d, Err(_) => { let _ = send_json(w, 500, "{\"error\":\"cover_missing\"}", cors.clone(), head_only).await; return; } };
                    let mut h = vec![("Content-Type".into(), "image/svg+xml".into()), ("Content-Length".into(), data.len().to_string())]; h.extend(cors.clone());
                    if write_head(w, 200, h).await.is_err() { return; } if !head_only && w.write_all(&data).await.is_err() { return; }
                }
                (Some(t), "stream") => {
                    let etag = format!("\"{}-{}-{}\"", t.id, t.bytes, t.duration_sec);
                    if req.header("If-None-Match") == Some(etag.as_str()) { let mut h = vec![("ETag".into(), etag.clone()), ("Content-Length".into(), "0".into())]; h.extend(cors.clone()); if write_head(w, 304, h).await.is_err() { return; } }
                    else {
                        let (start, end, code) = if let Some(rh) = req.header("Range") { match range::parse_range(rh, t.bytes) { Ok(r) => (r.start, r.end, 206), Err(()) => { let mut extra = cors.clone(); extra.push(("Content-Range".into(), format!("bytes */{}", t.bytes))); extra.push(("Accept-Ranges".into(), "bytes".into())); if send_json(w, 416, "{\"error\":\"range_not_satisfiable\"}", extra, head_only).await.is_err() { return; } if !keep { return; } continue; } } } else { (0, t.bytes - 1, 200) };
                        let len = end - start + 1;
                        let mut h = vec![("Content-Type".into(), "audio/wav".into()), ("Content-Length".into(), len.to_string()), ("Accept-Ranges".into(), "bytes".into()), ("ETag".into(), etag)]; if code == 206 { h.push(("Content-Range".into(), format!("bytes {}-{}/{}", start, end, t.bytes))); } h.extend(cors.clone());
                        if write_head(w, code, h).await.is_err() { return; }
                        if !head_only {
                            // Stream from disk in bounded chunks; stop on disconnect (write error).
                            let mut stdf = match std::fs::File::open(&t.wav) { Ok(f) => f, Err(_) => return };
                            if stdf.seek(std::io::SeekFrom::Start(start)).is_err() { return; }
                            let mut file = tokio::fs::File::from_std(stdf);
                            let mut remaining = len; let mut buf = vec![0u8; 16384];
                            while remaining > 0 { let want = remaining.min(buf.len() as u64) as usize; match file.read(&mut buf[..want]).await { Ok(0) => break, Ok(n) => { if w.write_all(&buf[..n]).await.is_err() { return; } remaining -= n as u64; } Err(_) => return } }
                        }
                    }
                }
                _ => { if send_json(w, 404, "{\"error\":\"not_found\"}", cors.clone(), head_only).await.is_err() { return; } }
            }
        } else {
            if send_json(w, 404, "{\"error\":\"not_found\"}", cors.clone(), head_only).await.is_err() { return; }
        }
        if !keep { return; }
    }
}
#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let token = match std::env::var("MUSIC_TOKEN") { Ok(v) if !v.is_empty() => v, _ => { eprintln!("error: MUSIC_TOKEN env var is required (no default)"); std::process::exit(2); } };
    let origin = std::env::var("MUSIC_ORIGIN").unwrap_or_else(|_| "http://localhost:5173".to_string());
    let port: u16 = std::env::var("MUSIC_PORT").ok().and_then(|v| v.parse().ok()).or_else(|| std::env::var("PORT").ok().and_then(|v| v.parse().ok())).unwrap_or(8080);
    let data = std::env::var("MUSIC_DATA_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("./data"));
    let tracks = match ensure_library(&data) { Ok(t) => t, Err(e) => { eprintln!("error: cannot seed library: {}", e); std::process::exit(1); } };
    let _ = &data; let ctx = Arc::new(Ctx { cfg: Arc::new(Config { token, origin }), tracks: Arc::new(tracks) });
    let listener = match TcpListener::bind(("127.0.0.1", port)).await { Ok(l) => l, Err(e) => { eprintln!("error: bind {}: {}", port, e); std::process::exit(1); } };
    println!("music-server listening on http://127.0.0.1:{} ({} tracks)", port, ctx.tracks.len());
    let sem = Arc::new(Semaphore::new(256));
    loop {
        let (stream, peer) = match listener.accept().await { Ok(v) => v, Err(_) => continue };
        let permit = match sem.clone().try_acquire_owned() { Ok(p) => p, Err(_) => { drop(stream); continue; } };
        let c = ctx.clone();
        tokio::spawn(async move { let _permit = permit; handle(BufReader::new(stream), c, peer).await; });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn req(headers: &[(&str,&str)]) -> Request { Request { method: "GET".into(), target: "/api/tracks".into(), version: "HTTP/1.1".into(), headers: headers.iter().map(|(a,b)| (a.to_string(), b.to_string())).collect() } }
    #[test] fn auth_rejection() { let cfg = Config { token: "s3cret".into(), origin: "http://localhost:5173".into() }; assert!(!authorized(&req(&[]), &cfg)); assert!(!authorized(&req(&[("Authorization","Bearer wrong")]), &cfg)); assert!(authorized(&req(&[("Authorization","Bearer s3cret")]), &cfg)); }
}
