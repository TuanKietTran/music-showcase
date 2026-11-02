# Error UX

`docs/error-catalog.json` is the single source of truth for codes, copy, surfaces, actions and retry policy. Clients mirror it; they never invent their own strings.

## Principles
1. Say what happened, what it means for the user, what to do next. No blame, no jargon, no status codes in visible text.
2. A playback error never hides the library or the queue. A library error never stops audio that is already playing.
3. Never lose position: on stall, retry or token re-entry, resume from the last known time.
4. The token never appears in any message, log, URL, or error detail. Messages come only from the catalog.
5. Entries with `"silent": true` (cover-load-failed) render nothing; the client uses a placeholder. `empty-library` is an empty state, not an error.

## Severity to surface
| severity | surface | notes |
|---|---|---|
| info | toast or empty-state | auto-dismiss ~6 s, never steals focus |
| warning | toast or inline-banner | stays until dismissed or resolved |
| error | inline-banner | at the top of the affected area (library or player) |
| blocking | blocking-dialog | only for 401; focus moves to the primary action |

The catalog's `surface` field is authoritative; this table explains the intent.

## Retry and backoff
- `retry.auto` true: wait `backoffMs[n]` before attempt n+1, up to `maxAttempts`, then show the manual Retry action.
- Never auto-retry 401, 404 or decode/malformed errors (their `auto` is false).
- Manual Retry always works and resets the attempt counter.
- Backoff is cancelled when the user changes track, changes server, or dismisses.
- Going back online (browser `online` event, reachability change) triggers one immediate retry.

## Playback state machine
```
playing --buffer empty >5s--> stalled --auto retry--> retrying --ok--> playing
stalled/retrying --network lost--> retrying (connection-lost-during-playback)
retrying --attempts exhausted--> failed (manual Retry / Skip shown)
any --404 or decode error--> failed --skip-track--> playing (next) or skipped
```
`skipped` means the track was passed over; the queue keeps it listed and marks it unavailable. Entering `failed` pauses audio, keeps position, keeps queue and library on screen.

## Accessibility
- assertive entries render with `role="alert"`; polite entries with `role="status"` (macOS: post VoiceOver announcements with matching priority; Android: `liveRegion`).
- Focus moves to the primary action only for blocking dialogs. Banners and toasts never take focus.
- Never color-only: severity also shown by icon/text label.
- Respect reduced motion: no slide/fade animations, no spinner pulsing beyond a static indicator.
- Actions are real buttons with accessible names ("Try again", "Skip track").

## Copy tone
| bad | good |
|---|---|
| "Error 401: Unauthorized" | "This server needs an access token before it will share music." |
| "Failed to fetch" | "We couldn't connect to the music server. Check that it is running and that you are online, then try again." |
| "You entered an invalid token" | "The server no longer accepts your token. Enter it again to keep listening." |

Rules: plain words, active voice, one next step, no exclamation marks, no blame.

## Mapping native errors to codes
| source | condition | code |
|---|---|---|
| fetch | TypeError / network failure | server-unreachable |
| fetch | AbortError from our timeout | request-timeout |
| HTTP | 401 (before library loaded / after) | unauthorized / unauthorized-mid-session |
| HTTP | 404 | track-not-found |
| HTTP | 416 | bad-range |
| HTTP | 5xx | server-error |
| JSON | `.json()` throws or wrong shape | malformed-response |
| HTMLMediaElement | MediaError 2 (NETWORK) | connection-lost-during-playback |
| HTMLMediaElement | MediaError 3 (DECODE), 4 (SRC_NOT_SUPPORTED) | audio-decode-failed |
| HTMLMediaElement | `stalled`/`waiting` > 5 s | stream-stalled |
| URLError | notConnectedToInternet, cannotConnectToHost, cannotFindHost, networkConnectionLost | server-unreachable (connection-lost-during-playback if playing) |
| URLError | timedOut | request-timeout |
| DecodingError | any | malformed-response |
| AVPlayerItem | failed with decode/format error | audio-decode-failed |
| AVPlayerItem | playbackStalled notification | stream-stalled |
| ExoPlayer | ERROR_CODE_IO_NETWORK_CONNECTION_*, TIMEOUT | connection-lost-during-playback / request-timeout |
| ExoPlayer | ERROR_CODE_DECODING_*, PARSING_* | audio-decode-failed |
| ExoPlayer | ERROR_CODE_IO_BAD_HTTP_STATUS | by status, as HTTP rows |
| anything else | | unknown |

## Adding a new error
1. Add an entry to `docs/error-catalog.json` with every field; pick the lowest severity that is honest.
2. Write the message with the tone guide; no status codes, no token.
3. Decide retry: auto only if transient; never for 401/404/decode.
4. Add the code to each client's catalog mirror (web `errors.ts`, macOS `AppError.swift`, Android `ErrorCatalog.kt`) and its classifier.
5. Verify the banner/toast role, focus behaviour, and reduced-motion handling.
6. Check a library error does not clear tracks and a playback error does not clear the queue.

## Status of implementations
Web and macOS are built and compiled. Android (`ErrorCatalog.kt`, `ErrorBanner`) is written as code only and has not been compiled or run.
