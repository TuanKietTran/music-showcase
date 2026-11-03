import { useCallback, useEffect, useRef, useState } from 'react';
import { blobUrl, classifyProbed, fetchTracks, type Track } from './api';
import { CATALOG, classify, classifyMedia, type ErrorCode } from './errors';
import { policyFor, sleep, withRetry } from './retry';
import ErrorBanner from './ErrorBanner';
type Repeat = 'off' | 'all' | 'one';
const fmt = (s: number) => (!isFinite(s) || s < 0 ? '0:00' : `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, '0')}`);
export default function App() {
  const [token, setToken] = useState(''); const [tokenInput, setTokenInput] = useState('');
  const [tracks, setTracks] = useState<Track[]>([]); const [covers, setCovers] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false); const [libError, setLibError] = useState<ErrorCode | null>(null); const [playError, setPlayError] = useState<ErrorCode | null>(null); const loadedRef = useRef(false); const resumeAt = useRef(0); const retryRun = useRef(0); const [needsToken, setNeedsToken] = useState(false);
  const [idx, setIdx] = useState<number | null>(null); const [playing, setPlaying] = useState(false); const [buffering, setBuffering] = useState(false);
  const [time, setTime] = useState(0); const [dur, setDur] = useState(0); const [volume, setVolume] = useState(0.8);
  const [shuffle, setShuffle] = useState(false); const [repeat, setRepeat] = useState<Repeat>('off');
  const [theme, setTheme] = useState<'auto' | 'dark' | 'light'>('auto'); const [buffered, setBuffered] = useState<{ start: number; end: number }[]>([]);
  const audioRef = useRef<HTMLAudioElement>(null); const preloadRef = useRef<HTMLAudioElement>(null); const wantPlay = useRef(false);
  const current = idx !== null ? tracks[idx] : undefined;
  useEffect(() => { if (theme === 'auto') document.documentElement.removeAttribute('data-theme'); else document.documentElement.dataset.theme = theme; }, [theme]);
  const load = useCallback(async (tok: string, submitted = false) => {
    setLoading(true); setLibError(null);
    try { const list = await withRetry(() => fetchTracks(tok), policyFor('server-unreachable'), { shouldRetry: (e) => { const c = classify(e); return c !== 'unauthorized' && c !== 'malformed-response'; } });
      setTracks(list); setNeedsToken(false); loadedRef.current = true;
      // cover failures are silent: fall back to an empty src (cover-load-failed has no banner)
      const entries = await Promise.all(list.map(async (t) => [t.id, await blobUrl(t.coverUrl, tok).catch(() => '')] as const)); setCovers(Object.fromEntries(entries));
    } catch (e) { const code = await classifyProbed(e, { loaded: loadedRef.current, tokenSubmitted: submitted && tok !== '' }); setNeedsToken(code === 'unauthorized' || code === 'unauthorized-mid-session' || code === 'token-rejected'); setLibError(code); } // tracks are never cleared on error
    setLoading(false);
  }, []);
  const playAt = useCallback((i: number, autoplay = true) => { wantPlay.current = autoplay; setIdx(i); }, []);
  useEffect(() => { void load(''); }, [load]); // first load shows the library, the unreachable banner, or (only after a 401) the token gate
  const step = useCallback((dir: 1 | -1) => {
    if (!tracks.length) return;
    if (shuffle && dir === 1 && tracks.length > 1) { let n = idx ?? 0; while (n === (idx ?? 0)) n = Math.floor(Math.random() * tracks.length); playAt(n); return; }
    const base = idx ?? 0; const n = (base + dir + tracks.length) % tracks.length; playAt(n);
  }, [tracks, idx, shuffle, playAt]);
  useEffect(() => { // load stream for current track, then honour autoplay
    const a = audioRef.current; if (!a || idx === null || !current) return; let live = true;
    setBuffering(true);
    blobUrl(current.streamUrl, token).then((url) => { if (!live) return; if (a.dataset.trackId !== current.id) { a.dataset.trackId = current.id; a.src = url; a.load(); } if (wantPlay.current) void a.play().catch(() => setPlaying(false)); }).catch(async (e) => { const c = await classifyProbed(e, { playing: true }); if (!live) return; setBuffering(false); setPlayError(c); });
    return () => { live = false; };
  }, [idx, current, token]);
  useEffect(() => { setPlayError(null); retryRun.current++; }, [idx]);
  const reloadAudio = useCallback(async () => { // re-fetch the stream and resume from the last known position
    const a = audioRef.current; if (!a || !current) return;
    const url = await blobUrl(current.streamUrl, token); a.dataset.trackId = current.id; a.src = url; a.load();
    a.currentTime = resumeAt.current; await a.play();
  }, [current, token]);
  const retryPlayback = useCallback(() => { retryRun.current++; setBuffering(true); reloadAudio().then(() => setPlayError(null)).catch(async (e) => { const c = await classifyProbed(e, { playing: true }); setBuffering(false); setPlayError(c); }); }, [reloadAudio]);
  useEffect(() => { // auto-retry per catalog policy (never for 401/404/decode: their policy has auto=false)
    if (!playError) return; const pol = CATALOG[playError].retry; if (!pol.auto) return;
    const run = ++retryRun.current; let n = 0;
    (async () => { while (n < pol.maxAttempts && run === retryRun.current) {
      await sleep(pol.backoffMs[Math.min(n, pol.backoffMs.length - 1)] ?? 1000); n++; if (run !== retryRun.current) return;
      try { await reloadAudio(); if (run === retryRun.current) setPlayError(null); return; } catch { /* try again */ } } })();
  }, [playError, reloadAudio]);
  useEffect(() => { // network events
    const off = () => { if (audioRef.current && !audioRef.current.paused) { resumeAt.current = audioRef.current.currentTime; setPlayError('connection-lost-during-playback'); } };
    const on = () => { if (libError === 'server-unreachable' && token) void load(token); if (playError) retryPlayback(); };
    window.addEventListener('offline', off); window.addEventListener('online', on); return () => { window.removeEventListener('offline', off); window.removeEventListener('online', on); };
  }, [libError, playError, token, load, retryPlayback]);
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
  const stallTimer = useRef<number | null>(null); // stream-stalled appears only after 5 s of waiting
  const startStall = () => { setBuffering(true); if (stallTimer.current === null) stallTimer.current = window.setTimeout(() => { stallTimer.current = null; if (audioRef.current) resumeAt.current = audioRef.current.currentTime; setPlayError((e) => e ?? 'stream-stalled'); }, 5000); };
  const stopStall = () => { if (stallTimer.current !== null) { window.clearTimeout(stallTimer.current); stallTimer.current = null; } setPlayError((e) => (e === 'stream-stalled' ? null : e)); };
  useEffect(() => () => { if (stallTimer.current !== null) window.clearTimeout(stallTimer.current); }, []);
  const barRef = useRef<HTMLElement>(null); // keep --bar-h equal to the real player bar height so nothing hides behind it
  useEffect(() => { const el = barRef.current; if (!el) return; const set = () => document.documentElement.style.setProperty('--bar-h', `${el.offsetHeight}px`); set(); const ro = new ResizeObserver(set); ro.observe(el); return () => ro.disconnect(); }, []);
  const onEnded = () => { if (repeat === 'one') { const a = audioRef.current; if (a) { a.currentTime = 0; void a.play(); } return; } if (idx !== null && idx === tracks.length - 1 && repeat === 'off' && !shuffle) { setPlaying(false); return; } step(1); };
  const readBuffered = () => { const a = audioRef.current; if (!a || !a.duration) return; const out: { start: number; end: number }[] = []; for (let i = 0; i < a.buffered.length; i++) out.push({ start: a.buffered.start(i), end: a.buffered.end(i) }); setBuffered(out); };
  return (<>
    <header className="site"><h1>Music Showcase</h1><button type="button" onClick={() => setTheme((t) => (t === 'auto' ? 'dark' : t === 'dark' ? 'light' : 'auto'))} aria-label="Toggle colour theme">Theme: {theme}</button></header>
    <main>
      <p className="sr" aria-live="polite">{current ? `Now playing ${current.title} by ${current.artist}` : ''}</p>
      {needsToken && (<section className="notice" aria-labelledby="tokh"><h2 id="tokh">{CATALOG[libError === 'unauthorized-mid-session' ? libError : 'unauthorized'].title}</h2><p className="muted">{CATALOG[libError === 'unauthorized-mid-session' ? libError : 'unauthorized'].message}</p><form className="token" onSubmit={(e) => { e.preventDefault(); setToken(tokenInput); void load(tokenInput, true); }}><label htmlFor="token">Token</label><input id="token" type="password" value={tokenInput} onChange={(e) => setTokenInput(e.target.value)} autoComplete="off" /><button type="submit">Connect</button></form>{libError === 'token-rejected' && <ErrorBanner code="token-rejected" onAction={() => document.getElementById('token')?.focus()} />}</section>)}
      {libError && !needsToken && <ErrorBanner code={libError} onAction={(a) => (a === 'retry' ? void load(token) : setLibError(null))} />}
      {loading && (<section aria-label="Loading tracks"><div className="grid">{[1, 2, 3, 4].map((n) => <div key={n} className="skel" />)}</div></section>)}
      {!loading && !needsToken && tracks.length === 0 && !libError && (<section className="notice"><h2>{CATALOG['empty-library'].title}</h2><p className="muted" role="status">{CATALOG['empty-library'].message}</p></section>)}
      {current && (<section className="np" aria-labelledby="nph"><img src={covers[current.id]} alt={`Cover art for ${current.title}`} /><div><h2 id="nph">{current.title}</h2><p className="muted">{current.artist} · {fmt(current.durationSec)}{current.sampleRate > 0 ? ` · ${current.sampleRate / 1000} kHz / ${current.bitDepth}-bit` : ''}{current.format ? ` · ${current.format.toUpperCase()}` : ''}{current.license ? ` · ${current.license}` : ''}</p><h3>Queue</h3><ol className="queue">{tracks.map((t, i) => (<li key={t.id}><button type="button" onClick={() => playAt(i)} aria-current={i === idx}> {i + 1}. {t.title} — {t.artist}</button></li>))}</ol></div></section>)}
      {!loading && tracks.length > 0 && (<section aria-labelledby="listh"><h2 id="listh">Tracks</h2><div className="grid">{tracks.map((t, i) => (<button key={t.id} type="button" className="card" onClick={() => playAt(i)} aria-current={i === idx} aria-label={`Play ${t.title} by ${t.artist}`}><img src={covers[t.id]} alt="" /><h3>{t.title}</h3><span className="muted">{t.artist} · {fmt(t.durationSec)}</span></button>))}</div></section>)}
    </main>

    <footer className="player" aria-label="Player" ref={barRef}>
      <audio ref={audioRef} preload="auto" onError={(e) => { resumeAt.current = e.currentTarget.currentTime; setPlaying(false); setBuffering(false); setPlayError(classifyMedia(e.currentTarget.error)); }} onPlaying={() => { stopStall(); setPlayError(null); }} onPlay={() => setPlaying(true)} onPause={() => { setPlaying(false); stopStall(); }} onEnded={onEnded} onTimeUpdate={(e) => { setTime(e.currentTarget.currentTime); readBuffered(); }} onLoadedMetadata={(e) => setDur(e.currentTarget.duration)} onWaiting={startStall} onStalled={startStall} onCanPlay={() => { setBuffering(false); stopStall(); }} onProgress={readBuffered} />
      <audio ref={preloadRef} preload="auto" hidden aria-hidden="true" />
      {playError && <ErrorBanner code={playError} onAction={(a) => (a === 'skip-track' ? step(1) : a === 'retry' ? retryPlayback() : setPlayError(null))} />}
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
