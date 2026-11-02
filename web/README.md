## Error handling

Errors come from `docs/error-catalog.json`, the single source of truth for codes, copy, severity, surface, actions and retry policy; `src/errors.ts` mirrors it as `CATALOG` — clients never invent their own strings. See [Error UX](../docs/error-ux.md).

- `classify()` in `src/errors.ts` maps failures to codes: fetch `TypeError` to `server-unreachable` (or `connection-lost-during-playback` while playing), HTTP status to `unauthorized`/`unauthorized-mid-session` (401), `track-not-found` (404), `bad-range` (416) and `server-error` (5xx), `AbortError`/`TimeoutError` to `request-timeout`, and JSON `SyntaxError` to `malformed-response`; `classifyMedia()` maps `MediaError` codes 2 to connection-lost and 3/4 to `audio-decode-failed`, otherwise `unknown`.
- `src/retry.ts`: `policyFor(code)` reads the catalog policy; `withRetry()` auto-retries only when `retry.auto` is true, waiting `backoffMs` (e.g. 1000/2000/4000) before the next attempt up to `maxAttempts`, then surfaces manual Retry. Never auto-retry 401, 404 or decode/malformed errors.
- `src/ErrorBanner.tsx`: `ErrorBanner` and `Toast` render catalog title/message/actions only, with `role="alert"` when `announce` is `assertive` and `role="status"` when `polite`. Banners stay at the top of the affected area until dismissed/resolved; toasts are transient (auto-dismiss ~6s). Silent entries render nothing.
- Library and playback errors are separate: a library error never stops audio already playing, and a playback error never hides the library — neither clears tracks or the queue, and playback position is preserved.
- The token never appears in any message, log, URL or error detail; visible text comes only from the catalog, with no status codes or jargon.
