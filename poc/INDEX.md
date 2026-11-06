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
