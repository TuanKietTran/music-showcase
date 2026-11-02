# Music Showcase

A small authenticated music-streaming demo with one Rust server and three clients (web, Android, macOS) that all speak the same HTTP API.

## Architecture

```
server/   Rust (Tokio only, no web framework) — serves the API and the audio/cover files
web/      React 19 + TypeScript + Vite
android/  Kotlin + Jetpack Compose + Media3/ExoPlayer + Coil
macos/    SwiftUI + AVPlayer (SwiftPM executable, macOS 14+)
docs/     docs/api.md — the API contract every client is written against
```

### Server (`server/`)

- `src/main.rs` — entry point, config, auth check, request dispatch. Binds `127.0.0.1`, max 256 concurrent connections.
- `src/http.rs` — minimal HTTP/1.1 request-head parsing helpers.
- `src/library.rs` — seeds/loads the track library from the data directory.
- `src/range.rs` — single-range `Range: bytes=…` parsing for stream seeking (206 / 416).
- `data/` — four seeded tracks, each a `.wav` plus a generated `.svg` cover: `aurora-lines`, `harbour-dawn`, `night-tram`, `paper-moon`.
- Configuration is env-only: `MUSIC_TOKEN` (required, no default — the server exits without it), `MUSIC_PORT` or `PORT` (default `8080`), `MUSIC_ORIGIN` (default `http://localhost:5173`, the only CORS origin allowed), `MUSIC_DATA_DIR` (default `./data`).

### API (see `docs/api.md` for the full contract)

- Every `/api` route except `GET /api/health` requires `Authorization: Bearer <MUSIC_TOKEN>`; missing/wrong token → `401 {"error":"unauthorized"}`.
- `GET /api/tracks` → JSON array of `{id,title,artist,durationSec,sampleRate,bitDepth,bytes,coverUrl,streamUrl}`. `coverUrl`/`streamUrl` are root-relative paths (`/api/tracks/{id}/cover|stream`) — clients must resolve them against their configured base URL.
- `GET /api/tracks/{id}/stream` → `audio/wav`, supports a single byte range (206 + `Content-Range`, 416 on a bad range), `ETag`, `Accept-Ranges`; `HEAD` returns headers only.
- `GET /api/tracks/{id}/cover` → `image/svg+xml`. Cover requests also need the Bearer token, so clients fetch cover bytes with the header rather than loading the URL directly.
- All clients keep the token in memory only (never disk / `localStorage` / `UserDefaults`) and prompt for it again on a 401.

### Clients

- **Web** (`web/src/api.ts`, `App.tsx`) — Vite dev server on port 5173 proxies `/api` to `http://localhost:8080` (see `web/vite.config.ts`), so no CORS config is needed in dev beyond the server default origin.
- **Android** (`android/app/src/main/java/com/example/musicshowcase/`) — `ApiClient.kt` (HTTP + JSON), `Track.kt` (model, relative-URL resolution, JSON parsing), `PlaybackConfig.kt` (in-memory base URL + token), `PlaybackService.kt` (Media3/ExoPlayer playback), `TrackListScreen.kt` / `NowPlayingScreen.kt` / `MainActivity.kt` (Compose UI). compileSdk/targetSdk 35, minSdk 26.
- **macOS** (`macos/Sources/MusicShowcase/`) — `APIClient.swift` (URLSession client; resolves relative `streamUrl`/`coverUrl` against the base URL and fetches cover data with the Bearer header), `PlayerModel.swift` (`AVPlayer` wrapper; stream requests carry the Authorization header via `AVURLAsset` options), `ContentView.swift` (server/token form + track list; token held in `@State` only), `NowPlayingView.swift` (seek/volume/prev/next controls), `MusicShowcaseApp.swift` (`@main` entry).

## Run instructions

Start the server first; every client needs the same token you give the server.

### Server

```sh
cd server
MUSIC_TOKEN=dev-token cargo run
# listening on http://127.0.0.1:8080
```

Smoke test (builds the server, starts it on a random free port, checks health, 401 without a token, the track list, a full stream, a 206 ranged stream, and a 416 bad range):

```sh
cd server
./scripts/smoke.sh   # prints SMOKE OK on success
```

### Web

```sh
cd web
npm install
npm run dev          # http://localhost:5173, /api proxied to :8080
npm run build        # tsc --noEmit && vite build
```

### macOS

```sh
cd macos
swift build
swift run MusicShowcase   # or ./.build/debug/MusicShowcase
```

Enter the server URL (default `http://localhost:8080`) and the `MUSIC_TOKEN` value in the form, then Load tracks.

### Android

Open `android/` in Android Studio (or a Gradle build with the Android SDK installed) and run the `app` configuration on a device/emulator. compileSdk 35 / minSdk 26, Java 17. There is no Gradle wrapper checked in.

## Verified vs not verified

Honest status as of 2026-11-02; "verified" means actually run and observed in this working session, not just written or read.

| Item | Status |
|---|---|
| macOS `swift build` | ✅ Verified — `Build complete!` (debug, SwiftPM) after the APIClient endpoint/relative-URL fix and the added SwiftUI app/views |
| Android internal-consistency fixes (Track model vs ApiClient/TrackList/MainActivity) | ⚠️ Written only — NOT compiled (no Android build run) |
| Server `cargo build` / `scripts/smoke.sh` | ⚠️ Not run in this session — instructions above are from the code and the smoke script itself |
| Web `npm run build` / dev server | ⚠️ Not run in this session |
| End-to-end playback (any client against the live server, incl. ranged seeking and cover loading) | ⚠️ Not verified |
| API contract in `docs/api.md` | Read and used as the source of truth for the client fixes; not independently re-tested against a running server in this session |
