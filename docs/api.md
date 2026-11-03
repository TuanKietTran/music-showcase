# Music Showcase API
Base: `http://localhost:8080` (server); web dev proxies `/api` there.
## Auth
Every `/api` route except `GET /api/health` needs `Authorization: Bearer <token>`.
Token is read once at server start from env `MUSIC_TOKEN`; no default — server refuses to start without it.
Missing/wrong token: `401 {"error":"unauthorized"}` with `WWW-Authenticate: Bearer`.
## Routes
`GET /api/health` → `200 {"status":"ok"}` (no auth).
`GET /api/tracks` → `200` JSON array of `{id,title,artist,durationSec,sampleRate,bitDepth,bytes,coverUrl,streamUrl,format?,license?,source?}`; `Cache-Control: no-store`. `sampleRate` and `bitDepth` may be `0`, meaning unknown (clients must not display `0 kHz / 0-bit`). Optional `format` is one of `mp3|ogg|flac|m4a|wav` when known; optional `license` and `source` are strings when known (e.g. `CC BY 3.0` and the source URL from `media/SOURCES.md`). Synthesized default tracks are `wav` with `sampleRate: 44100` and `bitDepth: 16`.
`GET /api/tracks/{id}/stream` → Content-Type follows the underlying file: `audio/mpeg` for mp3, `audio/ogg` for ogg, `audio/flac` for flac, `audio/mp4` for m4a, `audio/wav` for wav; supports single `Range: bytes=a-b|a-|-n` → `206` + `Content-Range`, bad range → `416`; sends `Accept-Ranges`, `Content-Length`, `ETag`. `HEAD` = headers only.
`GET /api/tracks/{id}/cover` → `image/svg+xml` (generated SVG cover art). For tracks without an SVG cover file, a cover is generated on the fly: a simple gradient with the track title (and artist).
Unknown id (ids are the only addressing; no paths) → `404 {"error":"not_found"}`.
## Cross-cutting
CORS: `OPTIONS` allowed only for origin in env `MUSIC_ORIGIN` (default `http://localhost:5173`); no wildcard.
Every response: `X-Content-Type-Options: nosniff`. Malformed request → `400`. Errors are JSON `{error}` (except stream errors).
Clients keep the token in memory only (never localStorage) and show a token prompt on `401`.
