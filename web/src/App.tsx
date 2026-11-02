import { useCallback, useEffect, useRef, useState } from 'react';
import { AuthError, blobUrl, fetchTracks, type Track } from './api';
type Repeat = 'off' | 'all' | 'one';
const fmt = (s: number) => (!isFinite(s) || s < 0 ? '0:00' : `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`);
export default function App() {
  const [token, setToken] = useState(''); const [tokenInput, setTokenInput] = useState('');
  const [tracks, setTracks] = useState<Track[]>([]); const [covers, setCovers] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false); const [error, setError] = useState(''); const [needsToken, setNeedsToken] = useState(true);
  const [idx, setIdx] = useState<number | null>(null); const [playing, setPlaying] = useState(false); const [buffering, setBuffering] = useState(false);
  const [time, setTime] = useState(0); const [dur, setDur] = useState(0); const [volume, setVolume] = useState(0.8);
  const [shuffle, setShuffle] = useState(false); const [repeat, setRepeat] = useState<Repeat>('off');
  const [theme, setTheme] = useState<'auto' | 'dark' | 'light'>('auto'); const [buffered, setBuffered] = useState<{ start: number; end: number }[]>([]);
  const audioRef = useRef<HTMLAudioElement>(null); const preloadRef = useRef<HTMLAudioElement>(null); const wantPlay = useRef(false);
  const current = idx !== null ? tracks[idx] : undefined;
  useEffect(() => { if (theme === 'auto') document.documentElement.removeAttribute('data-theme'); else document.documentElement.dataset.theme = theme; }, [theme]);
  const load = useCallback(async (tok: string) => {
    setLoading(true); setError('');
    try { const list = await fetchTracks(tok); setTracks(list); setNeedsToken(false);
      const entries = await Promise.all(list.map(async (t) => [t.id, await blobUrl(t.coverUrl, tok)] as const)); setCovers(Object.fromEntries(entries));
    } catch (e) { if (e instanceof AuthError) { setNeedsToken(true); setError('That token was rejected (401). Enter the server token.'); } else setError(e instanceof Error ? e.message : 'Failed to load tracks.'); }
    setLoading(false);
  }, []);
  const playAt = useCallback((i: number, autoplay = true) => { wantPlay.current = autoplay; setIdx(i); }, []);
  const step = useCallback((dir: 1 | -1) => {
    if (!tracks.length) return;
    if (shuffle && dir === 1 && tracks.length > 1) { let n = idx ?? 0; while (n === (idx ?? 0)) n = Math.floor(Math.random() * tracks.length); playAt(n); return; }
    const base = idx ?? 0; const n = (base + dir + tracks.length) % tracks.length; playAt(n);
  }, [tracks, idx, shuffle, playAt]);
  useEffect(() => { // load stream for current track, then honour autoplay
    const a = audioRef.current; if (!a || idx === null || !current) return; let live = true;
    setBuffering(true);
    blobUrl(current.streamUrl, token).then((url) => { if (!live) return; if (a.dataset.trackId !== current.id) { a.dataset.trackId = current.id; a.src = url; a.load(); } if (wantPlay.current) void a.play().catch(() => setPlaying(false)); }).catch(() => setError('Could not load that track.'));
    return () => { live = false; };
  }, [idx, current, token]);
  useEffect(() => { // preload the next track (blob prefetch + hidden preload element)
    if (idx === null || tracks.length < 2) return; const n = tracks[(idx + 1) % tracks.length];
    blobUrl(n.streamUrl, token).then((url) => { if (preloadRef.current) preloadRef.current.src = url; }).catch(() => undefined);
  }, [idx, tracks, token]);
  useEffect(() => { if (audioRef.current) audioRef.current.volume = volume; }, [volume]);
  useEffect(() => { // keyboard: space, arrows, n/p (ignored while typing)
    const onKey = (e: KeyboardEvent) => { const el = e.target as HTMLElement; if (el.tagName === 'INPUT' && (el as HTMLInputElement).type !== 'range' || el.tagName === 'TEXTAREA') return;
      const a = audioRef.current; if (!a) return;
      if (e.code === 'Space') { e.preventDefault(); if (a.paused) void a.play(); else a.pause(); }
      else if (e.key === 'ArrowLeft') a.currentTime = Math.max(0, a.currentTime - 5);
      else if (e.key === 'ArrowRight') a.currentTime = Math.min(a.duration || 0, a.currentTime + 5);
      else if (e.key === 'ArrowUp') { e.preventDefault(); setVolume((v) => Math.min(1, v + 0.1)); }
      else if (e.key === 'ArrowDown') { e.preventDefault(); setVolume((v) => Math.max(0, v - 0.1)); }
      else if (e.key === 'n') step(1); else if (e.key === 'p') step(-1); };
    window.addEventListener('keydown', onKey); return () => window.removeEventListener('keydown', onKey);
  });
  const onEnded = () => { if (repeat === 'one') { const a = audioRef.current; if (a) { a.currentTime = 0; void a.play(); } return; } if (idx !== null && idx === tracks.length - 1 && repeat === 'off' && !shuffle) { setPlaying(false); return; } step(1); };
  const readBuffered = () => { const a = audioRef.current; if (!a || !a.duration) return; const out: { start: number; end: number }[] = []; for (let i = 0; i < a.buffered.length; i++) out.push({ start: a.buffered.start(i), end: a.buffered.end(i) }); setBuffered(out); };
  return (<>
    <header className="site"><h1>Music Showcase</h1><button type="button" onClick={() => setTheme((t) => (t === 'auto' ? 'dark' : t === 'dark' ? 'light' : 'auto'))} aria-label="Toggle colour theme">Theme: {theme}</button></header>
    <main>
      <p className="sr" aria-live="polite">{current ? `Now playing ${current.title} by ${current.artist}` : ''}</p>
      {needsToken && (<section className="notice" aria-labelledby="tokh"><h2 id="tokh">Server token required</h2><p className="muted">The server requires a bearer token (401 without one). It is kept in memory only, never in localStorage.</p><form className="token" onSubmit={(e) => { e.preventDefault(); setToken(tokenInput); void load(tokenInput); }}><label htmlFor="token">Token</label><input id="token" type="password" value={tokenInput} onChange={(e) => setTokenInput(e.target.value)} autoComplete="off" /><button type="submit">Connect</button></form></section>)}
      {error && !needsToken && (<section className="notice" role="alert"><p>{error}</p><button type="button" onClick={() => void load(token)}>Retry</button></section>)}
      {loading && (<section aria-label="Loading tracks"><div className="grid">{[1, 2, 3, 4].map((n) => <div key={n} className="skel" />)}</div></section>)}
      {!loading && !needsToken && tracks.length === 0 && !error && (<section className="notice"><h2>No tracks</h2><p className="muted">The library is empty.</p></section>)}
      {current && (<section className="np" aria-labelledby="nph"><img src={covers[current.id]} alt={`Cover art for ${current.title}`} /><div><h2 id="nph">{current.title}</h2><p className="muted">{current.artist} · {fmt(current.durationSec)} · {current.sampleRate / 1000} kHz / {current.bitDepth}-bit</p><h3>Queue</h3><ol className="queue">{tracks.map((t, i) => (<li key={t.id}><button type="button" onClick={() => playAt(i)} aria-current={i === idx}> {i + 1}. {t.title} — {t.artist}</button></li>))}</ol></div></section>)}
      {!loading && tracks.length > 0 && (<section aria-labelledby="listh"><h2 id="listh">Tracks</h2><div className="grid">{tracks.map((t, i) => (<button key={t.id} type="button" className="card" onClick={() => playAt(i)} aria-current={i === idx} aria-label={`Play ${t.title} by ${t.artist}`}><img src={covers[t.id]} alt="" /><h3>{t.title}</h3><span className="muted">{t.artist} · {fmt(t.durationSec)}</span></button>))}</div></section>)}
    </main>
    <footer className="player" aria-label="Player">
      <audio ref={audioRef} preload="auto" onPlay={() => setPlaying(true)} onPause={() => setPlaying(false)} onEnded={onEnded} onTimeUpdate={(e) => { setTime(e.currentTarget.currentTime); readBuffered(); }} onLoadedMetadata={(e) => setDur(e.currentTarget.duration)} onWaiting={() => setBuffering(true)} onStalled={() => setBuffering(true)} onCanPlay={() => setBuffering(false)} onProgress={readBuffered} />
      <audio ref={preloadRef} preload="auto" hidden aria-hidden="true" />
      <div className="row"><strong>{current ? current.title : 'Nothing selected'}</strong><span className="muted">{current?.artist ?? ''}</span>{buffering && <span className="spinner" role="status" aria-label="Buffering" />}
        <button type="button" onClick={() => step(-1)} aria-label="Previous track">⏮</button>
        <button type="button" onClick={() => { const a = audioRef.current; if (!a) return; if (idx === null && tracks.length) { playAt(0); return; } if (a.paused) void a.play(); else a.pause(); }} aria-label={playing ? 'Pause' : 'Play'}>{playing ? '⏸' : '▶'}</button>
        <button type="button" onClick={() => step(1)} aria-label="Next track">⏭</button>
        <button type="button" aria-pressed={shuffle} onClick={() => setShuffle((s) => !s)} aria-label="Shuffle">Shuffle</button>
        <button type="button" onClick={() => setRepeat((r) => (r === 'off' ? 'all' : r === 'all' ? 'one' : 'off'))} aria-label={`Repeat: ${repeat}`}>Repeat: {repeat}</button></div>
      <div className="row"><span aria-label="Elapsed time">{fmt(time)}</span><div className="seekwrap"><div className="buffered" aria-hidden="true">{buffered.map((b, i) => (<i key={i} style={{ left: `${dur ? (b.start / dur) * 100 : 0}%`, width: `${dur ? ((b.end - b.start) / dur) * 100 : 0}%` }} />))}</div><input type="range" min={0} max={dur || current?.durationSec || 0} step={0.1} value={time} onChange={(e) => { if (audioRef.current) audioRef.current.currentTime = Number(e.target.value); }} aria-label="Seek" /></div><span aria-label="Remaining time">-{fmt((dur || 0) - time)}</span><label>Volume <input type="range" min={0} max={1} step={0.05} value={volume} onChange={(e) => setVolume(Number(e.target.value))} aria-label="Volume" /></label></div>
    </footer>
  </>);
}
