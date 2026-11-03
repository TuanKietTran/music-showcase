//! Upload helpers: magic-byte sniffing, text sanitizing, query parsing (docs/abac.md section 5).

/// mp3 (`ID3` or an MPEG frame sync), ogg (`OggS`), flac (`fLaC`), m4a (`ftyp` at offset 4), wav (`RIFF....WAVE`).
pub fn sniff(b: &[u8]) -> Option<&'static str> {
    if b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WAVE" { return Some("wav"); }
    if b.len() >= 4 && &b[0..4] == b"fLaC" { return Some("flac"); }
    if b.len() >= 4 && &b[0..4] == b"OggS" { return Some("ogg"); }
    if b.len() >= 8 && &b[4..8] == b"ftyp" { return Some("m4a"); }
    if b.len() >= 3 && &b[0..3] == b"ID3" { return Some("mp3"); }
    // MPEG audio frame sync: 11 set bits, with valid version (not 01) and layer (not 00) fields.
    if b.len() >= 2 && b[0] == 0xFF && (b[1] & 0xE0) == 0xE0 && (b[1] & 0x18) != 0x08 && (b[1] & 0x06) != 0x00 { return Some("mp3"); }
    None
}

/// Trim, drop control characters (tab/newline included), cap at 120 chars; `default` if nothing is left.
pub fn sanitize(s: &str, default: &str) -> String {
    let cleaned: String = s.chars().filter(|c| !c.is_control()).collect();
    let t: String = cleaned.trim().chars().take(120).collect();
    let t = t.trim().to_string();
    if t.is_empty() { default.to_string() } else { t }
}

pub fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes(); let mut out = Vec::with_capacity(b.len()); let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => { let h = s.get(i + 1..i + 3)?; out.push(u8::from_str_radix(h, 16).ok()?); i += 3; }
            b'+' => { out.push(b' '); i += 1; }
            c => { out.push(c); i += 1; }
        }
    }
    String::from_utf8(out).ok()
}

/// Parse `a=b&c=d` (no duplicates handling needed: first wins).
pub fn parse_query(target: &str) -> Vec<(String, String)> {
    let q = match target.split_once('?') { Some((_, q)) => q, None => return vec![] };
    q.split('&').filter(|p| !p.is_empty()).filter_map(|p| { let (k, v) = p.split_once('=').unwrap_or((p, "")); Some((percent_decode(k)?, percent_decode(v)?)) }).collect()
}
pub fn query_get<'a>(q: &'a [(String, String)], k: &str) -> Option<&'a str> { q.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str()) }

pub fn now_secs() -> u64 { std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn sniffs_all_five_formats() {
        assert_eq!(sniff(b"ID3\x04\x00\x00\x00\x00\x00\x00"), Some("mp3"));
        assert_eq!(sniff(&[0xFF, 0xFB, 0x90, 0x00]), Some("mp3"));
        assert_eq!(sniff(b"OggS\x00\x02"), Some("ogg"));
        assert_eq!(sniff(b"fLaC\x00\x00\x00\x22"), Some("flac"));
        assert_eq!(sniff(b"\x00\x00\x00\x20ftypM4A \x00"), Some("m4a"));
        assert_eq!(sniff(b"RIFF\x24\x00\x00\x00WAVEfmt "), Some("wav"));
    }
    #[test] fn garbage_is_not_audio() {
        for g in [&b""[..], b"hello world, not audio", b"<html>", b"\x7fELF\x02\x01\x01", b"MZ\x90\x00", b"RIFF\x24\x00\x00\x00AVI LIST", &[0xFF, 0xF1][..] /* AAC-ADTS: layer 00 */, &[0xFF, 0xE8, 0, 0][..] /* reserved version */, &[0xFF][..], b"PK\x03\x04"] {
            assert_eq!(sniff(g), None, "{:?}", g);
        }
    }
    #[test] fn sanitizes_text() {
        assert_eq!(sanitize("  Hello\tWorld\n ", "Untitled"), "HelloWorld"); // control chars are dropped, not replaced
        assert_eq!(sanitize("\t\n ", "Untitled"), "Untitled");
        assert_eq!(sanitize(&"x".repeat(300), "d").chars().count(), 120);
        assert_eq!(sanitize("a\u{0}b\u{7f}c", "d"), "abc");
        assert_eq!(sanitize("Tëst ♪", "d"), "Tëst ♪");
    }
    #[test] fn query_parsing() {
        let q = parse_query("/api/tracks?title=Hello%20World&artist=A+B&visibility=public&empty=");
        assert_eq!(query_get(&q, "title"), Some("Hello World")); assert_eq!(query_get(&q, "artist"), Some("A B"));
        assert_eq!(query_get(&q, "visibility"), Some("public")); assert_eq!(query_get(&q, "empty"), Some("")); assert_eq!(query_get(&q, "x"), None);
        assert_eq!(percent_decode("%zz"), None); assert_eq!(percent_decode("%C3%A9"), Some("é".to_string())); assert_eq!(percent_decode("%ff"), None);
    }
}
