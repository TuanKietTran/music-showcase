//! Seed library: 4 original synthesized 16-bit/44.1kHz stereo WAV tracks + SVG covers.
use std::{fs, io::Write, path::{Path, PathBuf}};
pub const SAMPLE_RATE: u32 = 44100;
#[derive(Clone)]
pub struct Track { pub id: &'static str, pub title: &'static str, pub artist: &'static str, pub duration_sec: u32, pub bytes: u64, pub wav: PathBuf, pub cover: PathBuf }
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
        out.push(Track { id: def.id, title: def.title, artist: def.artist, duration_sec: def.duration_sec, bytes, wav, cover });
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
}
