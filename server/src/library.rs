//! Seed library: 4 original synthesized 16-bit/44.1kHz stereo WAV tracks + SVG covers.
use std::{fs, io::Write, path::{Path, PathBuf}};
pub const SAMPLE_RATE: u32 = 44100;
#[derive(Clone)]
pub struct Track {
  pub id: String,
  pub title: String,
  pub artist: String,
  pub duration_sec: u32,
  pub sample_rate: u32,
  pub bit_depth: u32,
  pub bytes: u64,
  pub file: PathBuf,
  pub cover: Option<PathBuf>,
  pub format: String,
  pub license: Option<String>,
  pub source: Option<String>,
}
pub fn content_type_for_format(format: &str) -> &'static str {
    match format {
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}
#[cfg(test)]
impl Track {
    pub fn content_type(&self) -> &'static str { content_type_for_format(&self.format) }
}
pub fn format_from_filename(name: &str) -> Option<String> {
    let idx = name.rfind('.')?;
    if idx + 1 >= name.len() { return None; }
    let ext = name[idx + 1..].to_ascii_lowercase();
    match ext.as_str() {
        "mp3" | "ogg" | "flac" | "m4a" | "wav" => Some(ext),
        _ => None,
    }
}
pub fn parse_library_tsv(text: &str, dir: &Path) -> std::io::Result<Vec<Track>> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.is_empty() { continue; }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 7 { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "wrong field count")); }
        let id = fields[0];
        let title = fields[1];
        let artist = fields[2];
        let file_name = fields[3];
        let duration_raw = fields[4];
        let license_raw = fields[5];
        let source_raw = fields[6];
        if id.is_empty() || id.contains('/') || id.contains("..") { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid id")); }
        if file_name.is_empty() || file_name.contains('/') || file_name.contains("..") { return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid file")); }
        let duration_sec: u32 = duration_raw.parse().map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid durationSec"))?;
        let format = format_from_filename(file_name).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, "unknown format"))?;
        let file_path = dir.join(file_name);
        let bytes = fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
        out.push(Track { id: id.to_string(), title: title.to_string(), artist: artist.to_string(), duration_sec, sample_rate: 0, bit_depth: 0, bytes, file: file_path, cover: None, format, license: if license_raw.is_empty() { None } else { Some(license_raw.to_string()) }, source: if source_raw.is_empty() { None } else { Some(source_raw.to_string()) } });
    }
    Ok(out)
}
pub fn load_dir(dir: &Path) -> std::io::Result<Vec<Track>> {
    let text = fs::read_to_string(dir.join("library.tsv"))?;
    let mut tracks = parse_library_tsv(&text, dir)?;
    for track in &mut tracks {
        let metadata = fs::metadata(&track.file)?;
        track.bytes = metadata.len();
    }
    Ok(tracks)
}
pub fn generated_cover_svg(title: &str, artist: &str) -> String {
    const PALETTE: &[(&str, &str)] = &[("#7c5cff", "#00c2a8"), ("#ff7a59", "#ffc24b"), ("#4b8cff", "#b45cff"), ("#00a8c5", "#243b8f")];
    let sum: usize = title.bytes().map(|b| b as usize).sum();
    let (c1, c2) = PALETTE[sum % PALETTE.len()];
    format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 400 400'><defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'><stop offset='0' stop-color='{}'/><stop offset='1' stop-color='{}'/></linearGradient></defs><rect width='400' height='400' fill='url(#g)'/><circle cx='200' cy='175' r='85' fill='rgba(255,255,255,0.25)'/><circle cx='200' cy='175' r='55' fill='rgba(0,0,0,0.25)'/><text x='200' y='310' font-family='sans-serif' font-size='30' fill='white' text-anchor='middle'>{}</text><text x='200' y='345' font-family='sans-serif' font-size='18' fill='white' text-anchor='middle'>{}</text></svg>", c1, c2, xml_escape(title), xml_escape(artist))
}
struct Def { id: &'static str, title: &'static str, artist: &'static str, duration_sec: u32, notes: &'static [f64], c1: &'static str, c2: &'static str }
const DEFS: &[Def] = &[
    Def { id: "aurora-lines", title: "Aurora Lines", artist: "Showcase Ensemble", duration_sec: 24, notes: &[261.63, 329.63, 392.0, 523.25, 392.0, 329.63], c1: "#7c5cff", c2: "#00c2a8" },
    Def { id: "harbour-dawn", title: "Harbour Dawn", artist: "Showcase Ensemble", duration_sec: 28, notes: &[220.0, 261.63, 293.66, 349.23, 293.66, 261.63], c1: "#ff7a59", c2: "#ffc24b" },
    Def { id: "paper-moon", title: "Paper Moon", artist: "Showcase Ensemble", duration_sec: 32, notes: &[293.66, 349.23, 440.0, 523.25, 440.0, 349.23], c1: "#4b8cff", c2: "#b45cff" },
    Def { id: "night-tram", title: "Night Tram", artist: "Showcase Ensemble", duration_sec: 36, notes: &[196.0, 246.94, 293.66, 392.0, 293.66, 246.94], c1: "#00a8c5", c2: "#243b8f" },
];
pub fn xml_escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;") }
pub fn json_escape(s: &str) -> String { s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n") }
fn synth_wav(def: &Def, path: &Path) -> std::io::Result<()> {
    let frames = SAMPLE_RATE * def.duration_sec;
    let data_len = frames * 4; // stereo i16
    let mut f = fs::File::create(path)?;
    f.write_all(b"RIFF")?; f.write_all(&(36 + data_len).to_le_bytes())?; f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?; f.write_all(&1u16.to_le_bytes())?; f.write_all(&2u16.to_le_bytes())?;
    f.write_all(&SAMPLE_RATE.to_le_bytes())?; f.write_all(&(SAMPLE_RATE * 4).to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?; f.write_all(&16u16.to_le_bytes())?;
    f.write_all(b"data")?; f.write_all(&data_len.to_le_bytes())?;
    let note_frames = SAMPLE_RATE / 2; // 0.5 s per note, looping
    let mut buf: Vec<u8> = Vec::with_capacity(16384 * 4);
    for i in 0..frames {
        let freq = def.notes[((i / note_frames) as usize) % def.notes.len()];
        let pos = (i % note_frames) as f64 / note_frames as f64;
        let env = (pos * std::f64::consts::PI).sin().min(1.0) * (1.0 - 0.3 * pos);
        let t = i as f64 / SAMPLE_RATE as f64;
        let v = (2.0 * std::f64::consts::PI * freq * t).sin() + 0.3 * (4.0 * std::f64::consts::PI * freq * t).sin();
        let sample = (v * 0.35 * env).clamp(-1.0, 1.0);
        let l = (sample * i16::MAX as f64) as i16;
        let r = (sample * 0.8 * i16::MAX as f64) as i16;
        buf.extend_from_slice(&l.to_le_bytes()); buf.extend_from_slice(&r.to_le_bytes());
        if buf.len() >= 16384 { f.write_all(&buf)?; buf.clear(); }
    }
    if !buf.is_empty() { f.write_all(&buf)?; }
    Ok(())
}
fn cover_svg(def: &Def, path: &Path) -> std::io::Result<()> {
    let svg = format!("<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 400 400'><defs><linearGradient id='g' x1='0' y1='0' x2='1' y2='1'><stop offset='0' stop-color='{}'/><stop offset='1' stop-color='{}'/></linearGradient></defs><rect width='400' height='400' fill='url(#g)'/><circle cx='200' cy='175' r='85' fill='rgba(255,255,255,0.25)'/><circle cx='200' cy='175' r='55' fill='rgba(0,0,0,0.25)'/><text x='200' y='310' font-family='sans-serif' font-size='30' fill='white' text-anchor='middle'>{}</text><text x='200' y='345' font-family='sans-serif' font-size='18' fill='white' text-anchor='middle'>{}</text></svg>", def.c1, def.c2, xml_escape(def.title), xml_escape(def.artist));
    fs::write(path, svg)
}
pub fn ensure_library(data: &Path) -> std::io::Result<Vec<Track>> {
    fs::create_dir_all(data)?;
    let mut out = Vec::new();
    for def in DEFS {
        let wav = data.join(format!("{}.wav", def.id));
        let cover = data.join(format!("{}.svg", def.id));
        if !wav.exists() || fs::metadata(&wav).map(|m| m.len() == 0).unwrap_or(true) { synth_wav(def, &wav)?; }
        if !cover.exists() { cover_svg(def, &cover)?; }
        let bytes = fs::metadata(&wav)?.len();
        out.push(Track { id: def.id.to_string(), title: def.title.to_string(), artist: def.artist.to_string(), duration_sec: def.duration_sec, sample_rate: SAMPLE_RATE, bit_depth: 16, bytes, file: wav, cover: Some(cover), format: "wav".to_string(), license: None, source: None });
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn traversal_id_never_matches() {
        // Ids come only from the fixed DEFS index; a traversal string is not an id.
        assert!(DEFS.iter().all(|d| !d.id.contains('/') && !d.id.contains("..")));
        assert!(DEFS.iter().find(|d| d.id == "../aurora-lines").is_none());
        assert!(DEFS.iter().find(|d| d.id == "..").is_none());
    }
    #[test] fn content_type_mapping() {
        assert_eq!(content_type_for_format("mp3"), "audio/mpeg");
        assert_eq!(content_type_for_format("ogg"), "audio/ogg");
        assert_eq!(content_type_for_format("flac"), "audio/flac");
        assert_eq!(content_type_for_format("m4a"), "audio/mp4");
        assert_eq!(content_type_for_format("wav"), "audio/wav");
    }
    #[test] fn content_type_unknown_fallback() {
        assert_eq!(content_type_for_format("aac"), "application/octet-stream");
        assert_eq!(content_type_for_format(""), "application/octet-stream");
    }
    #[test] fn format_from_filename_cases() {
        assert_eq!(format_from_filename("song.mp3"), Some("mp3".to_string()));
        assert_eq!(format_from_filename("Song.FLAC"), Some("flac".to_string()));
        assert_eq!(format_from_filename("archive.tar.ogg"), Some("ogg".to_string()));
        assert_eq!(format_from_filename("noext"), None);
        assert_eq!(format_from_filename("trailing."), None);
        assert_eq!(format_from_filename("song.txt"), None);
    }
    #[test] fn tsv_parses_valid_line() {
        let dir = Path::new("/tmp/rm-srv-3-media");
        let text = "test1\tTest Song\tTest Artist\tsong.mp3\t123\tCC0\thttps://example.org/src";
        let tracks = parse_library_tsv(text, dir).expect("valid line should parse");
        assert_eq!(tracks.len(), 1);
        let t = &tracks[0];
        assert_eq!(t.id, "test1");
        assert_eq!(t.title, "Test Song");
        assert_eq!(t.artist, "Test Artist");
        assert_eq!(t.duration_sec, 123);
        assert_eq!(t.format, "mp3");
        assert_eq!(t.file, dir.join("song.mp3"));
        assert_eq!(t.sample_rate, 0);
        assert_eq!(t.bit_depth, 0);
        assert_eq!(t.cover, None);
        assert_eq!(t.license, Some("CC0".to_string()));
        assert_eq!(t.source, Some("https://example.org/src".to_string()));
        assert_eq!(t.content_type(), "audio/mpeg");
    }
    #[test] fn tsv_empty_license_source_are_none() {
        let dir = Path::new("/tmp/rm-srv-3-media");
        let line = "test1\tTest Song\tTest Artist\tsong.mp3\t123\t\t";
        let text = format!("\n\n{}\n\n", line);
        let tracks = parse_library_tsv(&text, dir).expect("valid line with blank lines should parse");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].license, None);
        assert_eq!(tracks[0].source, None);
        let empty = parse_library_tsv("", dir).expect("empty text should parse");
        assert!(empty.is_empty());
    }
    #[test] fn tsv_rejects_wrong_field_count() {
        let dir = Path::new("/tmp/rm-srv-3-media");
        let six = "test1\tTest Song\tTest Artist\tsong.mp3\t123\tCC0";
        assert!(parse_library_tsv(six, dir).is_err());
        let eight = "test1\tTest Song\tTest Artist\tsong.mp3\t123\tCC0\thttps://example.org/src\textra";
        assert!(parse_library_tsv(eight, dir).is_err());
    }
    #[test] fn tsv_rejects_traversal_and_bad_values() {
        let dir = Path::new("/tmp/rm-srv-3-media");
        let cases: [(&str, &str, &str); 7] = [("", "song.mp3", "123"), ("../x", "song.mp3", "123"), ("a/b", "song.mp3", "123"), ("test1", "../song.mp3", "123"), ("test1", "sub/song.mp3", "123"), ("test1", "song.mp3", "abc"), ("test1", "song.txt", "123")];
        for (id, file, duration) in cases {
            let line = format!("{}\tTest Song\tTest Artist\t{}\t{}\tCC0\thttps://example.org/src", id, file, duration);
            assert!(parse_library_tsv(&line, dir).is_err(), "should reject id={:?} file={:?} duration={:?}", id, file, duration);
        }
    }
}
