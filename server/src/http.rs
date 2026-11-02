//! Hand-written HTTP/1.1 request-head parser (no external crates).
pub const MAX_REQUEST_LINE: usize = 8192;
pub const MAX_HEADERS: usize = 64;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request { pub method: String, pub target: String, pub version: String, pub headers: Vec<(String, String)> }
impl Request {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
    pub fn path(&self) -> &str { self.target.split('?').next().unwrap_or("/") }
}
#[derive(Debug, PartialEq, Eq)]
pub struct ParseError(pub &'static str);
fn valid_token(s: &str) -> bool { !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b)) }
pub fn parse_request_line(line: &str) -> Result<(String, String, String), ParseError> {
    if line.len() > MAX_REQUEST_LINE { return Err(ParseError("request line too long")); }
    let mut parts = line.split(' ');
    let method = parts.next().ok_or(ParseError("bad request line"))?;
    let target = parts.next().ok_or(ParseError("bad request line"))?;
    let version = parts.next().ok_or(ParseError("bad request line"))?;
    if parts.next().is_some() || !valid_token(method) || method.chars().any(|c| c.is_lowercase()) { return Err(ParseError("bad method")); }
    if !target.starts_with('/') || target.contains(' ') { return Err(ParseError("bad target")); }
    if version != "HTTP/1.1" && version != "HTTP/1.0" { return Err(ParseError("bad version")); }
    Ok((method.to_string(), target.to_string(), version.to_string()))
}
pub fn parse_request_head(head: &[u8]) -> Result<Request, ParseError> {
    let text = std::str::from_utf8(head).map_err(|_| ParseError("not utf8"))?;
    let mut lines = text.lines();
    let first = lines.next().ok_or(ParseError("empty"))?;
    let (method, target, version) = parse_request_line(first)?;
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() { continue; }
        if headers.len() >= MAX_HEADERS { return Err(ParseError("too many headers")); }
        let (name, value) = line.split_once(':').ok_or(ParseError("bad header"))?;
        if !valid_token(name.trim()) { return Err(ParseError("bad header name")); }
        headers.push((name.trim().to_string(), value.trim().to_string()));
    }
    Ok(Request { method, target, version, headers })
}
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = (a.len() ^ b.len()) as u32;
    let n = a.len().max(b.len());
    for i in 0..n {
        let x = if i < a.len() { a[i] } else { 0 };
        let y = if i < b.len() { b[i] } else { 0 };
        diff |= (x ^ y) as u32;
    }
    diff == 0
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn parses_request() {
        let r = parse_request_head(b"GET /api/tracks HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer x\r\n\r\n").unwrap();
        assert_eq!(r.method, "GET"); assert_eq!(r.path(), "/api/tracks"); assert_eq!(r.header("authorization"), Some("Bearer x"));
    }
    #[test] fn rejects_garbage() {
        assert!(parse_request_head(b"WAT\r\n\r\n").is_err());
        assert!(parse_request_head(b"GET /x HTTP/9.9\r\n\r\n").is_err());
        assert!(parse_request_line(&"x".repeat(MAX_REQUEST_LINE + 1)).is_err());
    }
    #[test] fn auth_compare() { assert!(constant_time_eq(b"secret", b"secret")); assert!(!constant_time_eq(b"secret", b"secrex")); assert!(!constant_time_eq(b"secret", b"sec")); }
}
