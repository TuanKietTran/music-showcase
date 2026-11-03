export type ErrorCode =
  | "server-unreachable" | "request-timeout" | "unauthorized" | "token-rejected" | "unauthorized-mid-session"
  | "track-not-found" | "bad-range" | "server-error" | "malformed-response"
  | "audio-decode-failed" | "stream-stalled" | "connection-lost-during-playback"
  | "cover-load-failed" | "empty-library" | "unknown";
export interface CatalogEntry {
  code: ErrorCode;
  severity: 'info' | 'warning' | 'error' | 'blocking';
  surface: string;
  title: string;
  message: string;
  actions: string[];
  retry: { auto: boolean; maxAttempts: number; backoffMs: number[] };
  announce: 'assertive' | 'polite';
  silent?: boolean;
}
export const CATALOG: Record<ErrorCode, CatalogEntry> = {
  "server-unreachable": { code: "server-unreachable", severity: "error", surface: "inline-banner", title: "Can't reach the server", message: "We couldn't connect to the music server. Check that it is running and that you are online, then try again.", actions: ["retry", "change-server", "dismiss"], retry: { auto: true, maxAttempts: 3, backoffMs: [1000, 2000, 4000] }, announce: "assertive" },
  "request-timeout": { code: "request-timeout", severity: "warning", surface: "inline-banner", title: "The server is taking too long", message: "The server didn't answer in time. Your library is still here. Try again in a moment.", actions: ["retry", "dismiss"], retry: { auto: true, maxAttempts: 3, backoffMs: [1000, 2000, 4000] }, announce: "polite" },
  "unauthorized": { code: "unauthorized", severity: "blocking", surface: "blocking-dialog", title: "Access token needed", message: "This server needs an access token before it will share music. Enter the token to continue.", actions: ["enter-token", "change-server"], retry: { auto: false, maxAttempts: 0, backoffMs: [] }, announce: "assertive" },
  "token-rejected": { code: "token-rejected", severity: "warning", surface: "inline-banner", title: "That token wasn't accepted", message: "Check it and try again; it is the MUSIC_TOKEN the server was started with.", actions: ["retry"], retry: { auto: false, maxAttempts: 0, backoffMs: [] }, announce: "assertive" },
  "unauthorized-mid-session": { code: "unauthorized-mid-session", severity: "blocking", surface: "blocking-dialog", title: "Please sign in again", message: "The server no longer accepts your token. Enter it again to keep listening. Your queue and position are kept.", actions: ["enter-token", "dismiss"], retry: { auto: false, maxAttempts: 0, backoffMs: [] }, announce: "assertive" },
  "track-not-found": { code: "track-not-found", severity: "warning", surface: "toast", title: "Track unavailable", message: "This track is no longer on the server, so we can't play it. You can skip to the next one.", actions: ["skip-track", "dismiss"], retry: { auto: false, maxAttempts: 0, backoffMs: [] }, announce: "polite" },
  "bad-range": { code: "bad-range", severity: "warning", surface: "toast", title: "Couldn't jump to that spot", message: "We couldn't seek to that point in the track. Playback restarts from where you were.", actions: ["retry", "dismiss"], retry: { auto: true, maxAttempts: 1, backoffMs: [500] }, announce: "polite" },
  "server-error": { code: "server-error", severity: "error", surface: "inline-banner", title: "The server had a problem", message: "Something went wrong on the server's side. Nothing you did caused it. Try again shortly.", actions: ["retry", "dismiss"], retry: { auto: true, maxAttempts: 3, backoffMs: [1000, 3000, 8000] }, announce: "polite" },
  "malformed-response": { code: "malformed-response", severity: "error", surface: "inline-banner", title: "Unexpected reply from the server", message: "The server sent something we couldn't read. Trying again may help; if not, the server may need an update.", actions: ["retry", "change-server", "dismiss"], retry: { auto: false, maxAttempts: 0, backoffMs: [] }, announce: "polite" },
  "audio-decode-failed": { code: "audio-decode-failed", severity: "error", surface: "toast", title: "This track won't play", message: "We couldn't play this track's audio. Your queue is unchanged, so you can skip to the next track.", actions: ["skip-track", "dismiss"], retry: { auto: false, maxAttempts: 0, backoffMs: [] }, announce: "polite" },
  "stream-stalled": { code: "stream-stalled", severity: "info", surface: "inline-banner", title: "Still loading the music", message: "The music is taking a while to load. We'll keep trying and resume from the same spot.", actions: ["retry", "skip-track"], retry: { auto: true, maxAttempts: 3, backoffMs: [2000, 4000, 8000] }, announce: "polite" },
  "connection-lost-during-playback": { code: "connection-lost-during-playback", severity: "warning", surface: "inline-banner", title: "Connection lost", message: "Playback paused because the connection dropped. We'll resume from the same spot when you're back online.", actions: ["retry", "skip-track", "dismiss"], retry: { auto: true, maxAttempts: 3, backoffMs: [1000, 2000, 4000] }, announce: "polite" },
  "cover-load-failed": { code: "cover-load-failed", severity: "info", surface: "toast", title: "Cover art unavailable", message: "Cover art couldn't be loaded. A placeholder is shown instead.", actions: [], retry: { auto: false, maxAttempts: 0, backoffMs: [] }, announce: "polite", silent: true },
  "empty-library": { code: "empty-library", severity: "info", surface: "empty-state", title: "No music yet", message: "The library is empty. Add some tracks on the server and refresh.", actions: ["retry"], retry: { auto: false, maxAttempts: 0, backoffMs: [] }, announce: "polite" },
  "unknown": { code: "unknown", severity: "error", surface: "inline-banner", title: "Something went wrong", message: "Something unexpected happened. Try again, and your library and queue will stay as they are.", actions: ["retry", "dismiss"], retry: { auto: false, maxAttempts: 0, backoffMs: [] }, announce: "polite" },
};
export class HttpError extends Error { constructor(public status: number) { super('http') } }
export function classify(e: unknown, ctx?: { loaded?: boolean; playing?: boolean; tokenSubmitted?: boolean }): ErrorCode {
  const name = (e as { name?: unknown } | null | undefined)?.name;
  if (name === 'AuthError') return ctx?.tokenSubmitted ? 'token-rejected' : ctx?.loaded ? 'unauthorized-mid-session' : 'unauthorized';
  const status = (e as { status?: unknown } | null | undefined)?.status;
  if (e instanceof HttpError || typeof status === 'number') {
    if (status === 401) return ctx?.loaded ? 'unauthorized-mid-session' : 'unauthorized';
    if (status === 404) return 'track-not-found';
    if (status === 416) return 'bad-range';
    if (typeof status === 'number' && status >= 500) return 'server-error';
  }
  if (name === 'AbortError' || name === 'TimeoutError') return 'request-timeout';
  if (e instanceof TypeError) return ctx?.playing ? 'connection-lost-during-playback' : 'server-unreachable';
  if (e instanceof SyntaxError) return 'malformed-response';
  return 'unknown';
}
export function classifyMedia(err: MediaError | null): ErrorCode {
  if (err?.code === 2) return 'connection-lost-during-playback';
  if (err?.code === 3 || err?.code === 4) return 'audio-decode-failed';
  return 'unknown';
}
