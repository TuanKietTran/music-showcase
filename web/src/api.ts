import { HttpError } from './errors';
export interface Track { id: string; title: string; artist: string; durationSec: number; sampleRate: number; bitDepth: number; bytes: number; coverUrl: string; streamUrl: string; }
export class AuthError extends Error { constructor(message?: string) { super(message); this.name = 'AuthError'; } }
const auth = (token: string) => ({ Authorization: `Bearer ${token}` });
export async function fetchTracks(token: string): Promise<Track[]> {
  const r = await fetch('/api/tracks', { headers: auth(token), signal: AbortSignal.timeout(15000) });
  if (r.status === 401) throw new AuthError('unauthorized');
  if (!r.ok) throw new HttpError(r.status);
  let data: unknown;
  try { data = await r.json(); } catch { throw new SyntaxError('bad shape'); }
  if (!Array.isArray(data)) throw new SyntaxError('bad shape');
  return data as Track[];
}
// Covers and streams need the bearer header, which <img>/<audio> cannot send,
// so they are fetched (with Range-capable server semantics for native clients)
// and exposed to the elements as object URLs.
const blobCache = new Map<string, Promise<string>>();
export function blobUrl(url: string, token: string): Promise<string> {
  const key = url + '|' + token.length;
  let p = blobCache.get(key);
  if (!p) { p = fetch(url, { headers: auth(token) }).then(async (r) => { if (r.status === 401) throw new AuthError('unauthorized'); if (!r.ok) throw new HttpError(r.status); return URL.createObjectURL(await r.blob()); }).catch((e) => { blobCache.delete(key); throw e; }); blobCache.set(key, p); }
  return p;
}
