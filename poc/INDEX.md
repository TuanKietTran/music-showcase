# Vinyl — PoC Asset Index

Goal (briefs/dod-push.md): >=100 PoC assets in `poc/`, including >=15 GIF/MP4
flow clips, every asset labelled by provenance. Other managers add sections
below; counts are per contribution.

## Provenance labels

- **REAL CAPTURE** — direct export or screenshot of the real source
  (Penpot board export / Penpot workspace screenshot). Not a redraw.
- **ASSEMBLED CLIP** — ffmpeg slideshow assembled from REAL CAPTURE stills
  (fade between frames, padded to 1440x900 on canonical `--bg #0C0B09`).
  Follows real page/prototype order; it is NOT an interactive recording.
- **PROTOTYPE RENDER** — HTML/mock render of a design. None in this section.
- **REAL RECORDING** — screen recording of the real built app running
  (vite preview page load). Initial shell only; backend unreachable.
- **DETAIL CROP** — a cropped region of a REAL CAPTURE still, pixels
  unmodified, no redraw.

## Contribution: Penpot Design System (mgr-penpot2) — 36 assets

Source: Penpot 2.18.6, file `Vinyl Design System`
(`f1a04e90-1a2d-80ed-8008-e6f85ed8a8a7`), branch `ui/vinyl-redesign`.
Totals: **30 stills + 3 clips (each in GIF + MP4) = 36 assets.**
Rebuild clips + restage stills: `python3 poc/penpot/gen_clips.py`
(requires ffmpeg; stills are byte copies of `design/penpot/png/*` plus the
library screenshot from `notes/shots/penpot/`).

### Stills — REAL CAPTURE (30, in `poc/penpot/stills/`)

- Library (1): `00-penpot-library-open.png` — workspace screenshot,
  Assets panel showing 17 components / 18 colors / 7 typographies.
- Tokens (4): `01-tokens--colors-dark.png`, `01-tokens--colors-light.png`,
  `01-tokens--spacing-radii-motion.png`, `01-tokens--type.png`
- Components (17): `02-components--button-ghost.png`,
  `02-components--button-primary.png`, `02-components--errorbanner.png`,
  `02-components--iconbutton.png`, `02-components--masthead.png`,
  `02-components--player-bar.png`, `02-components--player-buffering.png`,
  `02-components--player-playing.png`, `02-components--recordcard.png`,
  `02-components--recordcard-disc.png`,
  `02-components--recordcard-playing.png`,
  `02-components--skeletoncard.png`, `02-components--tokengate.png`,
  `02-components--tracklist-sidegroup.png`,
  `02-components--tracklist-sidegroupb.png`,
  `02-components--trackrow-default.png`,
  `02-components--trackrow-playing.png`
- Screens (8): `03-screens-empty--empty-desktop-dark.png`,
  `03-screens-flow--flow-prototype-map.png`,
  `03-screens-library--library-desktop-dark-paused.png`,
  `03-screens-library-light--library-light-desktop-paused.png`,
  `03-screens-loading--loading-desktop-dark.png`,
  `03-screens-mobile--mobile-390-dark.png`,
  `03-screens-nowplaying--library-nowplaying-desktop-dark.png`,
  `03-screens-tokengate--tokengate-error-desktop-dark.png`

### Clips — ASSEMBLED CLIP (3 flows, in `poc/penpot/clips/`)

| Flow | Frames (in order) | MP4 | GIF |
|---|---|---|---|
| `flow-01-unlock-to-nowplaying` | TokenGate/Error -> Library -> NowPlaying (the 2 prototype navigate-to links built in Penpot) | 1440x900 H.264, 5.4s, 129407B | 720x450, 5.38s, 354871B |
| `flow-02-screen-states` | Loading -> Library Light -> Empty -> Mobile | 1440x900 H.264, 7.2s, 117567B | 720x450, 7.26s, 322824B |
| `flow-03-design-system` | Colors Dark -> Type -> Flow prototype map -> Library screenshot | 1440x900 H.264, 7.2s, 191838B | 720x450, 7.26s, 671634B |

Verified 2026-11-06: ffprobe dimensions/durations/codecs as tabled;
stills byte-identical to their sources; gen_clips.py rerun reproduces
the same file set and sizes.

## Contribution: PoC expansion (poc-expand) — 78 assets

Running totals for `poc/`: **114 assets = 78 PNG + 18 GIF + 18 MP4**
(plus `INDEX.md` and 4 `gen_*.py` rebuild scripts), meeting
briefs/poc-expand.md / dod-push.md (>=100 assets, >=15 clips, >=18 GIFs).
This contribution: 36 app stills + 4 app clip files + 14 penpot tour
clip files + 12 penpot zoom clip files + 12 penpot detail crops = 78.
Rebuild penpot additions: `python3 poc/penpot/gen_tours.py`,
`python3 poc/penpot/gen_zooms.py`, `python3 poc/penpot/gen_crops.py`
(require ffmpeg; gen_crops.py also requires Pillow).

### App stills — REAL CAPTURE (36, in `poc/app/`)

Screenshots of the real built app (vite preview, initial shell —
backend unreachable, nothing faked). AFTER = `ui/vinyl-redesign` build
(PASS, 22 modules); BEFORE = `main` 6758166 build (PASS, 21 modules).
One file per build per viewport, named by build and viewport size
(e.g. `after-1440x900.png`, `before-390x844.png`). Viewports (18 each):
320x700, 360x740, 375x812, 390x844, 412x915, 414x896, 480x800, 600x900,
768x1024, 834x1112, 900x1200, 1024x768, 1280x800, 1366x768, 1440x900,
1536x864, 1600x1000, 1920x1080.

### App clips — REAL RECORDING (2 flows, in `poc/app/clips/`)

| Flow | Subject | MP4 | GIF |
|---|---|---|---|
| `page-load-reveal-desktop` | AFTER build page-load reveal, desktop | 1440x900 H.264, 7.33s, 1234049B | 720x450, 7.40s, 2150358B |
| `page-load-reveal-mobile` | AFTER build page-load reveal, mobile | 390x844 H.264, 5.39s, 508286B | 390x844, 5.43s, 2983838B |

### Penpot tour clips — ASSEMBLED CLIP (7 flows, in `poc/penpot/clips/`)

Same ASSEMBLED method as flow-01..03 (1.8s per frame, fades, 1440x900).

| Flow | Frames (in order) | MP4 | GIF |
|---|---|---|---|
| `flow-04-tokens-tour` | Colors Dark -> Colors Light -> Type -> Spacing/Radii/Motion | 1440x900 H.264, 7.2s, 133836B | 720x450, 7.26s, 426198B |
| `flow-05-components-player` | Player/Bar -> Player/Buffering -> Player/Playing -> TrackRow/Default -> TrackRow/Playing | 1440x900 H.264, 9.0s, 103532B | 720x450, 9.01s, 219232B |
| `flow-06-components-cards-buttons` | Button/Primary -> Button/Ghost -> IconButton -> RecordCard -> RecordCard/Disc -> RecordCard/Playing -> SkeletonCard -> ErrorBanner | 1440x900 H.264, 14.4s, 205708B | 720x450, 14.38s, 667701B |
| `flow-07-screens-dark-tour` | Library -> NowPlaying -> Flow prototype map | 1440x900 H.264, 5.4s, 146229B | 720x450, 5.38s, 409584B |
| `flow-08-states-tour` | Loading -> Empty -> TokenGate/Error screen -> TokenGate -> ErrorBanner | 1440x900 H.264, 9.0s, 124601B | 720x450, 9.01s, 349476B |
| `flow-09-light-mobile-tour` | Library Light -> Colors Light -> Mobile | 1440x900 H.264, 5.4s, 103521B | 720x450, 5.38s, 307422B |
| `flow-10-full-system-tour` | Library screenshot -> Colors Dark -> Type -> Masthead -> Player/Playing -> RecordCard -> Library -> NowPlaying -> Flow prototype map | 1440x900 H.264, 16.2s, 357922B | 720x450, 16.26s, 1083430B |

### Penpot zoom/pan clips — ASSEMBLED CLIP (6 flows, in `poc/penpot/clips/`)

ffmpeg zoompan over a single REAL CAPTURE still normalized onto a
2880x1800 padded canvas (canonical bg), output 1440x900, 4.0s each.
NOT interactive recordings. All GIFs 720x450, ~4.01s.

| Flow | Source still / motion | MP4 (1440x900 H.264, 4.0s) | GIF |
|---|---|---|---|
| `zoom-01-tokens-colors-zoom-in` | Colors Dark, zoom-in 1.0->1.5 | 215834B | 956144B |
| `zoom-02-type-pan-right` | Type, pan left->right at 1.4x | 130016B | 870376B |
| `zoom-03-library-zoom-in` | Library screen, zoom-in 1.0->1.5 | 243851B | 653083B |
| `zoom-04-nowplaying-zoom-out` | NowPlaying screen, zoom-out 1.5->1.0 | 337341B | 1228774B |
| `zoom-05-mobile-pan-down` | Mobile screen, pan top->bottom at 1.4x | 150428B | 746678B |
| `zoom-06-library-open-pan-right` | Library screenshot, pan left->right at 1.4x | 243137B | 1595112B |

### Penpot detail crops — DETAIL CROP (12, in `poc/penpot/details/`)

| Crop | Source still | Size |
|---|---|---|
| `detail-01-library-top-left.png` | Library screen | 720x450 |
| `detail-02-library-center.png` | Library screen | 720x450 |
| `detail-03-nowplaying-center.png` | NowPlaying screen | 720x540 |
| `detail-04-nowplaying-player-strip.png` | NowPlaying screen | 1440x225 |
| `detail-05-colors-dark-top.png` | Colors Dark board | 620x320 |
| `detail-06-type-left.png` | Type board | 640x460 |
| `detail-07-player-playing-center.png` | Player/Playing component | 552x96 |
| `detail-08-recordcard-playing-center.png` | RecordCard/Playing component | 180x240 |
| `detail-09-mobile-top.png` | Mobile screen | 390x422 |
| `detail-10-library-open-left-panel.png` | Library screenshot | 640x1000 |
| `detail-11-flow-map-center.png` | Flow prototype map | 864x540 |
| `detail-12-masthead-center.png` | Masthead component | 736x112 |

Verified 2026-11-06: ffprobe dimensions/durations/codecs for all 36
clip files as tabled; totals 78 PNG + 18 GIF + 18 MP4 = 114 assets;
INDEX filename check 0 missing; gen_tours/gen_zooms/gen_crops reruns
reproduce the same file set and sizes.
