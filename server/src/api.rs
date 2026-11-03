//! Request routing and handlers. Every decision about a track or account goes through `decide` (docs/abac.md).
use crate::accounts::Account;
use crate::auth::Auth;
use crate::http::Request;
use crate::index::{Index, UploadRow};
use crate::library::{content_type_for_format, generated_cover_svg, json_escape, Track};
use crate::media::{now_secs, parse_query, query_get, sanitize, sniff};
use crate::policy::{Attrs, Decision, Policy, Value};
use crate::range::parse_range;
use crate::storage::{object_key, random_hex, Body, Storage, StoreError};
use std::{io, path::PathBuf, time::Duration};
use tokio::{io::{AsyncReadExt, AsyncWriteExt, BufReader}, net::TcpStream, sync::{Mutex, Semaphore}};

pub struct Config { pub origin: String, pub max_upload_bytes: u64, pub trust_proxy: bool, pub upload_timeout: Duration }
pub struct Ctx { pub cfg: Config, pub tracks: Vec<Track>, pub policy: Policy, pub auth: Auth, pub storage: Storage, pub index: Mutex<Index>, pub slots: Semaphore }

pub enum Payload { None, Bytes(Vec<u8>), Body(Body), File { path: PathBuf, start: u64, len: u64 } }
pub struct Reply { pub code: u16, pub headers: Vec<(String, String)>, pub payload: Payload, pub close: bool }

pub fn reason(code: u16) -> &'static str {
    match code { 200=>"OK", 201=>"Created", 204=>"No Content", 206=>"Partial Content", 304=>"Not Modified", 400=>"Bad Request", 401=>"Unauthorized", 403=>"Forbidden", 404=>"Not Found",
        405=>"Method Not Allowed", 411=>"Length Required", 413=>"Content Too Large", 415=>"Unsupported Media Type", 416=>"Range Not Satisfiable", 500=>"Internal Server Error", 503=>"Service Unavailable", _=>"OK" }
}

pub fn cors_headers(req: &Request, cfg: &Config) -> Vec<(String, String)> {
    match req.header("Origin") {
        Some(o) if o == cfg.origin => vec![("Access-Control-Allow-Origin".into(), o.to_string()), ("Access-Control-Expose-Headers".into(), "Content-Range, Accept-Ranges".into()), ("Vary".into(), "Origin".into())],
        _ => vec![],
    }
}

fn reply(code: u16, headers: Vec<(String, String)>, payload: Payload) -> Reply { Reply { code, headers, payload, close: false } }
pub fn json_reply(code: u16, body: &str, extra: Vec<(String, String)>) -> Reply {
    let mut h = vec![("Content-Type".into(), "application/json".into()), ("Content-Length".into(), body.len().to_string())]; h.extend(extra);
    reply(code, h, Payload::Bytes(body.as_bytes().to_vec()))
}
fn err(code: u16, name: &str, cors: &[(String, String)]) -> Reply { json_reply(code, &format!("{{\"error\":\"{}\"}}", name), cors.to_vec()) }
fn closing(mut r: Reply) -> Reply { r.close = true; r }

pub async fn write_reply(w: &mut TcpStream, r: Reply, head_only: bool) -> io::Result<()> {
    let mut headers = r.headers;
    if !headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("X-Content-Type-Options")) { headers.push(("X-Content-Type-Options".into(), "nosniff".into())); }
    if r.close { headers.push(("Connection".into(), "close".into())); }
    let mut s = format!("HTTP/1.1 {} {}\r\n", r.code, reason(r.code));
    for (n, v) in &headers { s.push_str(&format!("{}: {}\r\n", n, v)); }
    s.push_str("\r\n");
    w.write_all(s.as_bytes()).await?;
    if head_only { return Ok(()); }
    match r.payload {
        Payload::None => {}
        Payload::Bytes(b) => w.write_all(&b).await?,
        Payload::Body(mut body) => { while let Some(chunk) = body.next_chunk().await { w.write_all(&chunk?).await?; } }
        Payload::File { path, start, len } => {
            let mut stdf = std::fs::File::open(&path)?;
            std::io::Seek::seek(&mut stdf, std::io::SeekFrom::Start(start))?;
            let mut file = tokio::fs::File::from_std(stdf);
            let (mut remaining, mut buf) = (len, vec![0u8; 16384]);
            while remaining > 0 { let want = remaining.min(buf.len() as u64) as usize; match file.read(&mut buf[..want]).await? { 0 => break, n => { w.write_all(&buf[..n]).await?; remaining -= n as u64; } } }
        }
    }
    Ok(())
}

pub fn bearer_token(req: &Request) -> Option<&str> { req.header("Authorization").and_then(|v| v.strip_prefix("Bearer ")).filter(|t| !t.is_empty()) }

// ---- attributes -------------------------------------------------------------------------------------------------

struct View { id: String, title: String, artist: String, duration_sec: u64, sample_rate: u32, bit_depth: u32, bytes: u64, format: String, license: Option<String>, source: Option<String>, owner_id: String, visibility: String, key: Option<String>, file: Option<PathBuf>, cover: Option<PathBuf> }

fn builtin_view(t: &Track) -> View {
    View { id: t.id.clone(), title: t.title.clone(), artist: t.artist.clone(), duration_sec: t.duration_sec as u64, sample_rate: t.sample_rate, bit_depth: t.bit_depth, bytes: t.bytes, format: t.format.clone(), license: t.license.clone(), source: t.source.clone(), owner_id: "system".into(), visibility: "public".into(), key: None, file: Some(t.file.clone()), cover: t.cover.clone() }
}
fn row_view(r: &UploadRow) -> View {
    View { id: r.id.clone(), title: r.title.clone(), artist: r.artist.clone(), duration_sec: r.duration_sec, sample_rate: 0, bit_depth: 0, bytes: r.bytes, format: r.format.clone(), license: None, source: None, owner_id: r.owner_id.clone(), visibility: r.visibility.clone(), key: Some(r.key.clone()), file: None, cover: None }
}
fn all_views(ctx: &Ctx, ix: &Index) -> Vec<View> { ctx.tracks.iter().map(builtin_view).chain(ix.rows().iter().map(row_view)).collect() }
fn find_view(ctx: &Ctx, ix: &Index, id: &str) -> Option<View> { ctx.tracks.iter().find(|t| t.id == id).map(builtin_view).or_else(|| ix.get(id).map(row_view)) }

fn resource_attrs(v: &View) -> Attrs {
    let mut a = Attrs::new();
    a.insert("resource.type".into(), "track".into()); a.insert("resource.id".into(), v.id.clone().into()); a.insert("resource.ownerId".into(), v.owner_id.clone().into());
    a.insert("resource.visibility".into(), v.visibility.clone().into()); a.insert("resource.format".into(), v.format.clone().into()); a.insert("resource.bytes".into(), v.bytes.into());
    if let Some(l) = &v.license { a.insert("resource.license".into(), l.clone().into()); }
    if let Some(s) = &v.source { a.insert("resource.source".into(), s.clone().into()); }
    a
}
const UNLIMITED: u64 = u64::MAX / 4;
fn remaining(who: &Account, used: u64) -> u64 { if who.unlimited { UNLIMITED } else { who.quota_bytes.saturating_sub(used) } }

/// Full attribute bag for one decision. `usage` = (bytes already used or reserved by the resource owner) when relevant.
fn attrs(ctx: &Ctx, who: &Account, resource: Attrs, usage: Option<u64>, secure: bool) -> Attrs {
    let mut a = resource;
    a.insert("subject.id".into(), who.id.clone().into()); a.insert("subject.name".into(), who.name.clone().into());
    a.insert("subject.role".into(), who.role.clone().into()); a.insert("subject.status".into(), who.status.clone().into());
    a.insert("subject.quotaBytes".into(), who.quota_bytes.into());
    let s = ctx.storage.attributes();
    a.insert("storage.backend".into(), s.backend.into()); a.insert("storage.encrypted".into(), s.encrypted.into()); a.insert("storage.maxObjectBytes".into(), s.max_object_bytes.into());
    if let Some(r) = s.region { a.insert("storage.region".into(), r.into()); }
    if let Some(b) = s.bucket { a.insert("storage.bucket".into(), b.into()); }
    if let Some(u) = usage { a.insert("storage.usedBytes".into(), u.into()); a.insert("storage.quotaBytes".into(), if who.unlimited { UNLIMITED } else { who.quota_bytes }.into()); a.insert("storage.remainingBytes".into(), remaining(who, u).into()); }
    a.insert("env.now".into(), now_secs().into()); a.insert("env.secure".into(), Value::Bool(secure));
    a
}
fn log_deny(who: &Account, action: &str, d: &Decision) { if let Decision::Deny(rule) = d { eprintln!("deny account={} action={} rule={}", who.id, action, rule.as_deref().unwrap_or("(none: default deny)")); } }

fn track_json(v: &View, me: &Account) -> String {
    let fmt = format!(",\"format\":\"{}\"", json_escape(&v.format));
    let lic = v.license.as_ref().map(|l| format!(",\"license\":\"{}\"", json_escape(l))).unwrap_or_default();
    let src = v.source.as_ref().map(|l| format!(",\"source\":\"{}\"", json_escape(l))).unwrap_or_default();
    format!("{{\"id\":\"{id}\",\"title\":\"{}\",\"artist\":\"{}\",\"durationSec\":{},\"sampleRate\":{},\"bitDepth\":{},\"bytes\":{},\"coverUrl\":\"/api/tracks/{id}/cover\",\"streamUrl\":\"/api/tracks/{id}/stream\"{}{}{},\"ownerId\":\"{}\",\"visibility\":\"{}\",\"mine\":{}}}",
        json_escape(&v.title), json_escape(&v.artist), v.duration_sec, v.sample_rate, v.bit_depth, v.bytes, fmt, lic, src, json_escape(&v.owner_id), v.visibility, v.owner_id == me.id, id = v.id)
}

// ---- routing ----------------------------------------------------------------------------------------------------

pub async fn route(ctx: &Ctx, req: &Request, reader: &mut BufReader<TcpStream>, cors: &[(String, String)]) -> Reply {
    let (method, path) = (req.method.as_str(), req.path());
    let head = method == "HEAD";
    let secure = ctx.cfg.trust_proxy && req.header("X-Forwarded-Proto") == Some("https");
    if path == "/api/health" { return if method == "GET" || head { json_reply(200, "{\"status\":\"ok\"}", cors.to_vec()) } else { err(405, "method_not_allowed", cors) }; }
    if !path.starts_with("/api/") { return if matches!(method, "GET" | "HEAD") { err(404, "not_found", cors) } else { err(405, "method_not_allowed", cors) }; }
    if !matches!(method, "GET" | "HEAD" | "POST" | "PATCH" | "DELETE") { return err(405, "method_not_allowed", cors); }
    let who = match bearer_token(req) { Some(t) => ctx.auth.authenticate(t).await, None => None };
    let who = match who { Some(w) => w, None => { let mut h = cors.to_vec(); h.push(("WWW-Authenticate".into(), "Bearer".into())); return json_reply(401, "{\"error\":\"unauthorized\"}", h); } };
    // A request body we are not going to read must close the connection (no request smuggling via leftover bytes).
    let has_body = req.header("Content-Length").map(|v| v != "0").unwrap_or(false) || req.header("Transfer-Encoding").is_some();
    let r = match (method, path) {
        ("GET" | "HEAD", "/api/me") => me(ctx, &who, secure, cors).await,
        ("GET" | "HEAD", "/api/tracks") => list(ctx, &who, secure, cors).await,
        ("POST", "/api/tracks") => return upload(ctx, req, reader, &who, secure, cors).await,
        (_, p) => match p.strip_prefix("/api/tracks/") {
            Some(rest) => {
                let (id, action) = rest.split_once('/').unwrap_or((rest, ""));
                let gate = match (method, action) { (_, "stream") => "stream", ("PATCH", _) => "update", ("DELETE", _) => "delete", _ => "read" };
                if gated(ctx, &who, gate, secure) { return err(403, "forbidden", cors); }
                match (method, action) {
                    ("GET" | "HEAD", "") => one(ctx, &who, id, secure, cors).await,
                    ("GET" | "HEAD", "stream") => stream(ctx, req, &who, id, secure, cors, head).await,
                    ("GET" | "HEAD", "cover") => cover(ctx, &who, id, secure, cors).await,
                    ("PATCH", "") => patch(ctx, req, &who, id, secure, cors).await,
                    ("DELETE", "") => delete(ctx, &who, id, secure, cors).await,
                    _ => err(404, "not_found", cors),
                }
            }
            None => err(404, "not_found", cors),
        },
    };
    if has_body { closing(r) } else { r }
}

async fn me(ctx: &Ctx, who: &Account, secure: bool, cors: &[(String, String)]) -> Reply {
    let used = ctx.index.lock().await.used_bytes(&who.id);
    let mut probe = Attrs::new(); probe.insert("resource.type".into(), "track".into()); probe.insert("resource.ownerId".into(), who.id.clone().into()); probe.insert("resource.bytes".into(), 1u64.into());
    probe.insert("resource.visibility".into(), "private".into());
    let can = ctx.policy.decide(&attrs(ctx, who, probe, Some(used), secure), "upload", "track") == Decision::Permit;
    let body = format!("{{\"id\":\"{}\",\"name\":\"{}\",\"role\":\"{}\",\"status\":\"{}\",\"quotaBytes\":{},\"usedBytes\":{},\"maxUploadBytes\":{},\"can\":{{\"upload\":{}}}}}", json_escape(&who.id), json_escape(&who.name), who.role, who.status, who.quota_bytes, used, ctx.cfg.max_upload_bytes, can);
    let mut h = cors.to_vec(); h.push(("Cache-Control".into(), "no-store".into()));
    json_reply(200, &body, h)
}

/// Subject-level gate: an explicit deny rule that holds for a harmless public system track (e.g. a suspended account)
/// is a 403 on every route, whereas an ordinary "not allowed to see this track" stays a 404.
fn gated(ctx: &Ctx, who: &Account, action: &str, secure: bool) -> bool {
    let mut probe = Attrs::new(); probe.insert("resource.type".into(), "track".into()); probe.insert("resource.ownerId".into(), "system".into()); probe.insert("resource.visibility".into(), "public".into());
    let d = ctx.policy.decide(&attrs(ctx, who, probe, None, secure), action, "track");
    if let Decision::Deny(Some(_)) = &d { log_deny(who, action, &d); true } else { false }
}

async fn list(ctx: &Ctx, who: &Account, secure: bool, cors: &[(String, String)]) -> Reply {
    if gated(ctx, who, "list", secure) { return err(403, "forbidden", cors); }
    let ix = ctx.index.lock().await;
    let items: Vec<String> = all_views(ctx, &ix).iter().filter(|v| ctx.policy.decide(&attrs(ctx, who, resource_attrs(v), None, secure), "list", "track") == Decision::Permit).map(|v| track_json(v, who)).collect();
    let mut h = cors.to_vec(); h.push(("Cache-Control".into(), "no-store".into()));
    json_reply(200, &format!("[{}]", items.join(",")), h)
}

/// Read-type access. A track the subject may not read is indistinguishable from a missing one (404).
async fn visible(ctx: &Ctx, who: &Account, id: &str, action: &str, secure: bool) -> Option<View> {
    let ix = ctx.index.lock().await;
    let v = find_view(ctx, &ix, id)?;
    let d = ctx.policy.decide(&attrs(ctx, who, resource_attrs(&v), None, secure), action, "track");
    if d == Decision::Permit { Some(v) } else { log_deny(who, action, &d); None }
}

async fn one(ctx: &Ctx, who: &Account, id: &str, secure: bool, cors: &[(String, String)]) -> Reply {
    match visible(ctx, who, id, "read", secure).await {
        Some(v) => { let mut h = cors.to_vec(); h.push(("Cache-Control".into(), "no-store".into())); json_reply(200, &track_json(&v, who), h) }
        None => err(404, "not_found", cors),
    }
}

async fn cover(ctx: &Ctx, who: &Account, id: &str, secure: bool, cors: &[(String, String)]) -> Reply {
    let v = match visible(ctx, who, id, "read", secure).await { Some(v) => v, None => return err(404, "not_found", cors) };
    let data = match &v.cover { Some(p) => match tokio::fs::read(p).await { Ok(d) => d, Err(_) => return err(500, "cover_missing", cors) }, None => generated_cover_svg(&v.title, &v.artist).into_bytes() };
    let mut h = vec![("Content-Type".into(), "image/svg+xml".into()), ("Content-Length".into(), data.len().to_string())]; h.extend(cors.iter().cloned());
    reply(200, h, Payload::Bytes(data))
}

async fn stream(ctx: &Ctx, req: &Request, who: &Account, id: &str, secure: bool, cors: &[(String, String)], head: bool) -> Reply {
    let v = match visible(ctx, who, id, "stream", secure).await { Some(v) => v, None => return err(404, "not_found", cors) };
    if v.bytes == 0 { return err(404, "not_found", cors); }
    let etag = format!("\"{}-{}-{}\"", v.id, v.bytes, v.duration_sec);
    if req.header("If-None-Match") == Some(etag.as_str()) { let mut h = vec![("ETag".into(), etag), ("Content-Length".into(), "0".into())]; h.extend(cors.iter().cloned()); return reply(304, h, Payload::None); }
    let (start, end, code) = match req.header("Range") {
        Some(rh) => match parse_range(rh, v.bytes) { Ok(r) => (r.start, r.end, 206), Err(()) => { let mut extra = cors.to_vec(); extra.push(("Content-Range".into(), format!("bytes */{}", v.bytes))); extra.push(("Accept-Ranges".into(), "bytes".into())); return json_reply(416, "{\"error\":\"range_not_satisfiable\"}", extra); } },
        None => (0, v.bytes - 1, 200),
    };
    let len = end - start + 1;
    let mut h = vec![("Content-Type".into(), content_type_for_format(&v.format).to_string()), ("Content-Length".into(), len.to_string()), ("Accept-Ranges".into(), "bytes".into()), ("ETag".into(), etag)];
    if code == 206 { h.push(("Content-Range".into(), format!("bytes {}-{}/{}", start, end, v.bytes))); }
    h.extend(cors.iter().cloned());
    if head { return reply(code, h, Payload::None); }
    let payload = match (&v.key, &v.file) {
        (Some(key), _) => match ctx.storage.open(key, Some((start, end))).await { Ok(b) => Payload::Body(b), Err(StoreError::NotFound) => return err(404, "not_found", cors), Err(e) => { eprintln!("storage open failed: {:?}", e); return err(500, "storage_error", cors); } },
        (None, Some(path)) => Payload::File { path: path.clone(), start, len },
        _ => return err(404, "not_found", cors),
    };
    reply(code, h, payload)
}

fn valid_visibility(v: &str) -> bool { v == "public" || v == "private" }

async fn patch(ctx: &Ctx, req: &Request, who: &Account, id: &str, secure: bool, cors: &[(String, String)]) -> Reply {
    let q = parse_query(&req.target);
    let vis = query_get(&q, "visibility");
    if matches!(vis, Some(v) if !valid_visibility(v)) { return err(400, "bad_request", cors); }
    let mut ix = ctx.index.lock().await;
    let v = match find_view(ctx, &ix, id) { Some(v) => v, None => return err(404, "not_found", cors) };
    let read = ctx.policy.decide(&attrs(ctx, who, resource_attrs(&v), None, secure), "read", "track");
    if read != Decision::Permit { log_deny(who, "read", &read); return err(404, "not_found", cors); }
    let d = ctx.policy.decide(&attrs(ctx, who, resource_attrs(&v), None, secure), "update", "track");
    if v.key.is_none() || d != Decision::Permit { log_deny(who, "update", &d); return err(403, "forbidden", cors); } // built-in tracks are read-only
    let (title, artist) = (query_get(&q, "title").map(|t| sanitize(t, "Untitled")), query_get(&q, "artist").map(|a| sanitize(a, "Unknown")));
    let vis = vis.map(str::to_string);
    match ix.update(id, |r| { if let Some(t) = title { r.title = t; } if let Some(a) = artist { r.artist = a; } if let Some(v) = vis { r.visibility = v; } }).await {
        Ok(Some(row)) => { let mut h = cors.to_vec(); h.push(("Cache-Control".into(), "no-store".into())); json_reply(200, &track_json(&row_view(&row), who), h) }
        Ok(None) => err(404, "not_found", cors),
        Err(e) => { eprintln!("index update failed: {}", e); err(500, "internal_error", cors) }
    }
}

async fn delete(ctx: &Ctx, who: &Account, id: &str, secure: bool, cors: &[(String, String)]) -> Reply {
    let mut ix = ctx.index.lock().await;
    let v = match find_view(ctx, &ix, id) { Some(v) => v, None => return err(404, "not_found", cors) };
    let read = ctx.policy.decide(&attrs(ctx, who, resource_attrs(&v), None, secure), "read", "track");
    if read != Decision::Permit { log_deny(who, "read", &read); return err(404, "not_found", cors); }
    let d = ctx.policy.decide(&attrs(ctx, who, resource_attrs(&v), None, secure), "delete", "track");
    let key = match (&v.key, d == Decision::Permit) { (Some(k), true) => k.clone(), _ => { log_deny(who, "delete", &d); return err(403, "forbidden", cors); } };
    // Object first (idempotent), then the index row: a failure in between leaves a row that can simply be deleted again.
    if let Err(e) = ctx.storage.delete(&key).await { eprintln!("storage delete failed: {:?}", e); return err(500, "storage_error", cors); }
    match ix.remove(id).await { Ok(_) => reply(204, cors.to_vec().into_iter().chain([("Content-Length".to_string(), "0".to_string())]).collect(), Payload::None), Err(e) => { eprintln!("index remove failed: {}", e); err(500, "internal_error", cors) } }
}

// ---- upload -----------------------------------------------------------------------------------------------------

async fn upload(ctx: &Ctx, req: &Request, reader: &mut BufReader<TcpStream>, who: &Account, secure: bool, cors: &[(String, String)]) -> Reply {
    // 1. Headers only: Content-Length is required, chunked is rejected.
    if req.header("Transfer-Encoding").is_some() { return closing(err(411, "length_required", cors)); }
    let len: u64 = match req.header("Content-Length") { None => return closing(err(411, "length_required", cors)), Some(v) => match v.parse() { Ok(n) => n, Err(_) => return closing(err(400, "bad_request", cors)) } };
    let q = parse_query(&req.target);
    let vis = query_get(&q, "visibility").unwrap_or("private");
    if !valid_visibility(vis) { return closing(err(400, "bad_request", cors)); }
    let duration: u64 = match query_get(&q, "durationSec") { None => 0, Some(d) => match d.parse() { Ok(n) => n, Err(_) => return closing(err(400, "bad_request", cors)) } };
    // 2. Decide before reading the body; reserve the bytes so parallel uploads cannot overshoot the quota.
    let mut proposed = Attrs::new();
    proposed.insert("resource.type".into(), "track".into()); proposed.insert("resource.ownerId".into(), who.id.clone().into()); proposed.insert("resource.visibility".into(), vis.into()); proposed.insert("resource.bytes".into(), len.into());
    {
        let mut ix = ctx.index.lock().await;
        let usage = ix.committed_and_reserved(&who.id);
        let d = ctx.policy.decide(&attrs(ctx, who, proposed.clone(), Some(usage), secure), "upload", "track");
        match d {
            Decision::Permit => ix.reserve(&who.id, len),
            Decision::Deny(rule) => {
                log_deny(who, "upload", &Decision::Deny(rule.clone()));
                return closing(match rule.as_deref() {
                    Some("deny-oversize") => err(413, "upload_too_large", cors),
                    Some(_) => err(403, "forbidden", cors),
                    None => {
                        // Default deny: was it only the quota? Re-evaluate with unlimited room.
                        let mut roomy = attrs(ctx, who, proposed.clone(), Some(usage), secure);
                        roomy.insert("storage.remainingBytes".into(), UNLIMITED.into());
                        if ctx.policy.decide(&roomy, "upload", "track") == Decision::Permit { err(403, "quota_exceeded", cors) } else { err(403, "forbidden", cors) }
                    }
                });
            }
        }
    }
    let result = ingest(ctx, req, reader, who, vis, duration, len, &q, cors).await;
    ctx.index.lock().await.release(&who.id, len);
    result
}

#[allow(clippy::too_many_arguments)]
async fn ingest(ctx: &Ctx, req: &Request, reader: &mut BufReader<TcpStream>, who: &Account, vis: &str, duration: u64, len: u64, q: &[(String, String)], cors: &[(String, String)]) -> Reply {
    let _slot = match ctx.slots.try_acquire() { Ok(s) => s, Err(_) => return closing(err(503, "server_busy", cors)) };
    let first_len = len.min(512) as usize;
    let mut first = vec![0u8; first_len];
    if req.header("Expect").map(|e| e.eq_ignore_ascii_case("100-continue")).unwrap_or(false) { if reader.get_mut().write_all(b"HTTP/1.1 100 Continue\r\n\r\n").await.is_err() { return closing(err(400, "bad_request", cors)); } }
    match tokio::time::timeout(Duration::from_secs(30), reader.read_exact(&mut first)).await { Ok(Ok(_)) => {}, _ => return closing(err(400, "bad_request", cors)) }
    let format = match sniff(&first) { Some(f) => f, None => return closing(err(415, "unsupported_media", cors)) }; // nothing has been written yet
    let id = match random_hex(8) { Ok(i) => i, Err(_) => return closing(err(500, "internal_error", cors)) };
    let key = match object_key(&who.id, &id, format) { Ok(k) => k, Err(_) => return closing(err(500, "internal_error", cors)) };
    // Content-Type comes from the sniffed format only; the client's header and any filename are ignored.
    let mut rest = (&mut *reader).take(len - first_len as u64);
    let put = tokio::time::timeout(ctx.cfg.upload_timeout, ctx.storage.put(&key, content_type_for_format(format), &first, &mut rest, len)).await;
    match put { Ok(Ok(())) => {}, Ok(Err(e)) => { eprintln!("upload failed for account {}: {:?}", who.id, e); return closing(err(500, "upload_failed", cors)); }, Err(_) => { eprintln!("upload timed out for account {}", who.id); let _ = ctx.storage.delete(&key).await; return closing(err(500, "upload_failed", cors)); } }
    let row = UploadRow { id, owner_id: who.id.clone(), visibility: vis.to_string(), title: sanitize(query_get(q, "title").unwrap_or(""), "Untitled"), artist: sanitize(query_get(q, "artist").unwrap_or(""), "Unknown"), key: key.clone(), format: format.to_string(), bytes: len, duration_sec: duration, created_at: now_secs() };
    let mut ix = ctx.index.lock().await;
    if let Err(e) = ix.insert(row.clone()).await { eprintln!("index insert failed: {}", e); let _ = ctx.storage.delete(&key).await; return err(500, "upload_failed", cors); } // never a partial track
    let mut h = cors.to_vec(); h.push(("Cache-Control".into(), "no-store".into())); h.push(("Location".into(), format!("/api/tracks/{}", row.id)));
    json_reply(201, &track_json(&row_view(&row), who), h)
}

#[cfg(test)]
pub(crate) fn test_ctx(storage: Storage, accounts: Vec<(&str, &str, &str, u64)>, max_upload: u64) -> Ctx {
    use crate::accounts::{sha256_hex, Accounts};
    let entries = accounts.iter().map(|(id, role, token, quota)| (Account { id: id.to_string(), name: id.to_string(), role: role.to_string(), status: if id.starts_with("susp") { "suspended".into() } else { "active".into() }, quota_bytes: *quota, unlimited: false }, sha256_hex(token.as_bytes()))).collect();
    Ctx { cfg: Config { origin: "http://localhost:5173".into(), max_upload_bytes: max_upload, trust_proxy: false, upload_timeout: Duration::from_secs(20) }, tracks: vec![], policy: Policy::default_policy(),
          auth: Auth { accounts: Accounts::new("boot-token", entries), #[cfg(feature = "aws")] oidc: None }, storage, index: Mutex::new(crate::index::mem()), slots: Semaphore::new(4) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn req(headers: &[(&str, &str)]) -> Request { Request { method: "GET".into(), target: "/api/tracks".into(), version: "HTTP/1.1".into(), headers: headers.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() } }
    #[test] fn bearer_parsing() {
        assert_eq!(bearer_token(&req(&[])), None); assert_eq!(bearer_token(&req(&[("Authorization", "Basic abc")])), None);
        assert_eq!(bearer_token(&req(&[("Authorization", "Bearer ")])), None); assert_eq!(bearer_token(&req(&[("authorization", "Bearer s3cret")])), Some("s3cret"));
    }
}
