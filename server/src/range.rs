//! Single-range parsing: bytes=a-b, bytes=a-, bytes=-n.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteRange { pub start: u64, pub end: u64 } // inclusive
pub fn parse_range(header: &str, len: u64) -> Result<ByteRange, ()> {
    if len == 0 { return Err(()); }
    let spec = header.trim().strip_prefix("bytes=").ok_or(())?;
    if spec.contains(',') || spec.is_empty() { return Err(()); }
    let (a, b) = spec.split_once('-').ok_or(())?;
    if a.is_empty() { // suffix
        let n: u64 = b.parse().map_err(|_| ())?;
        if n == 0 { return Err(()); }
        let count = n.min(len);
        return Ok(ByteRange { start: len - count, end: len - 1 });
    }
    let start: u64 = a.parse().map_err(|_| ())?;
    if start >= len { return Err(()); }
    if b.is_empty() { return Ok(ByteRange { start, end: len - 1 }); }
    let end_req: u64 = b.parse().map_err(|_| ())?;
    if end_req < start { return Err(()); }
    Ok(ByteRange { start, end: end_req.min(len - 1) })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn suffix() { assert_eq!(parse_range("bytes=-100", 1000), Ok(ByteRange{start:900,end:999})); }
    #[test] fn open_ended() { assert_eq!(parse_range("bytes=10-", 1000), Ok(ByteRange{start:10,end:999})); }
    #[test] fn bounded_clamped() { assert_eq!(parse_range("bytes=0-9999", 1000), Ok(ByteRange{start:0,end:999})); }
    #[test] fn invalid() {
        assert!(parse_range("bytes=1000-", 1000).is_err());
        assert!(parse_range("bytes=50-10", 1000).is_err());
        assert!(parse_range("bytes=-0", 1000).is_err());
        assert!(parse_range("bytes=0-1,5-6", 1000).is_err());
        assert!(parse_range("items=0-1", 1000).is_err());
    }
}
