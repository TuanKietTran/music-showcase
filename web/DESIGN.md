# Web Design
## Palette (one accent)
Dark (default): bg `#0f1115`, surface `#181c22`, surface-2 `#21262f`, text `#f2f4f8`, muted `#b9c1cd` (both >= 4.5:1 on bg/surface), accent amber `#ffbf47` (black text on it).
Light (via `prefers-color-scheme`, or manual toggle): bg `#ffffff`, surface `#f2f3f5`, surface-2 `#e6e8ec`, text `#16181d`, muted `#4a5260` (>= 4.5:1), accent `#8a4b00` (white text on it) — same amber hue, darkened for contrast.
All tokens live in `src/tokens.css` as CSS custom properties; manual toggle sets `data-theme` on `<html>` and is kept in memory only.
## Type scale
`--fs-xs 12 / sm 14 / base 16 / lg 20 / xl 28 / 2xl 40`, line-height 1.5 body, 1.15 headings; system font stack (no webfont download).
## Spacing / radii / motion
4/8 rhythm: `--sp-1 4, -2 8, -3 12, -4 16, -6 24, -8 32`. Radii: `--r-sm 6, -md 10, -lg 16, -full 999`.
Motion: `--dur 160ms ease` for hover/focus/playing indicator; skeleton pulse 1.2s. `@media (prefers-reduced-motion: reduce)` zeroes animations/transitions.
## Layout & UX
Header (brand + theme toggle) / main: now-playing panel + track grid (1 col phone, auto-fill >= 220px desktop) / sticky bottom player bar (cover thumb, transport, seek with buffered ranges, time, volume, shuffle/repeat, queue toggle). Queue is a list in the now-playing panel. States: skeleton cards while loading, empty notice, error notice with retry, token prompt on 401 (token in memory only). Buffering = spinner in player bar. Focus: 3px accent outline, `:focus-visible` only.
