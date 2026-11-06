# Web Design — Vinyl

All tokens live in `src/tokens.css` as CSS custom properties. No gradients anywhere. No purple. Light background is a cool paper grey, not cream.

## Palette

### Lacquer dark (default, `:root`)

| Token | Value | Use |
|---|---|---|
| `--bg` | `#0C0B09` | Page background |
| `--surface` | `#161310` | Primary surface / cards |
| `--surface2` | `#211C16` | Raised / secondary surface |
| `--groove` | `#2A251D` | Groove lines, dividers, subtle borders inspired by record grooves |
| `--text` | `#F0E9DB` | Primary text |
| `--muted` | `#A99F8D` | Secondary text |
| `--accent` | `#FF4D0D` | Primary accent / primary actions |
| `--on-accent` | `#160B04` | Text / icons on `--accent` |
| `--accent2` | `#E8B33C` | Secondary accent — see usage rule below |

### Light

| Token | Value | Use |
|---|---|---|
| `--bg` | `#E8E6E1` | Page background (cool paper grey) |
| `--surface` | `#F4F2ED` | Primary surface / cards |
| `--surface2` | `#DDD9D0` | Raised / secondary surface |
| `--text` | `#171310` | Primary text |
| `--muted` | `#4E463C` | Secondary text |
| `--accent` | `#C23A00` | Primary accent / primary actions |
| `--on-accent` | `#FFFFFF` | Text / icons on `--accent` |

Light blocks do NOT override `--groove` or `--accent2`; both are inherited from `:root` (`--groove: #2A251D`, `--accent2: #E8B33C`).

### Groove / accent2 usage rule

- `--groove` (`#2A251D`): use for groove lines, dividers, hairlines and subtle structural borders in both themes.
- `--accent2` (`#E8B33C`): use ONLY for secondary / playing states — for example the currently playing indicator, secondary playing highlights, and secondary status cues. Never use `--accent2` as the primary accent, for primary actions, or as a general decorative accent. Primary actions and primary emphasis always use `--accent`.

### Contrast

Text contrast stays >= 4.5:1 in both themes: `--text` and `--muted` each meet >= 4.5:1 against `--bg`, `--surface` and `--surface2` in both the dark and light palettes above.

### Theme behaviour

- `:root` is the dark (lacquer) default.
- `@media (prefers-color-scheme: light)` applies the light palette to `:root:not([data-theme='dark'])` — i.e. the system light preference applies unless the user has explicitly chosen dark.
- `:root[data-theme='light']` applies the light palette (explicit light via the theme toggle).
- `:root[data-theme='dark']` applies the dark palette (explicit dark via the theme toggle).
- The theme toggle sets `data-theme` (`light` or `dark`) on `<html>`; without a manual choice, `prefers-color-scheme` decides.

## Typography

Fonts are loaded in `index.html` from Google Fonts with `font-display: swap` (`display=swap`): preconnect to `fonts.googleapis.com` and `fonts.gstatic.com` (with `crossorigin`), then Anton, IBM Plex Mono 400;500, and Space Grotesk 400;500;600;700.

| Token | Stack | Use |
|---|---|---|
| `--font-display` | `'Anton', Impact, Haettenschweiler, 'Franklin Gothic Heavy', sans-serif` | Display / headings |
| `--font-body` | `'Space Grotesk', Futura, 'Trebuchet MS', sans-serif` | Body / UI text |
| `--font-mono` | `'IBM Plex Mono', 'SF Mono', Consolas, 'Courier New', monospace` | Monospace / numeric / code-like text |

Fallbacks are part of each token stack above; if the webfonts fail to load, the listed system fallbacks apply in order.

### Type scale (unchanged)

| Token | Value |
|---|---|
| `--fs-xs` | `12px` |
| `--fs-sm` | `14px` |
| `--fs-base` | `16px` |
| `--fs-lg` | `20px` |
| `--fs-xl` | `28px` |
| `--fs-2xl` | `40px` |

## Spacing / radii / motion

### Spacing (unchanged)

| Token | Value |
|---|---|
| `--sp-1` | `4px` |
| `--sp-2` | `8px` |
| `--sp-3` | `12px` |
| `--sp-4` | `16px` |
| `--sp-6` | `24px` |
| `--sp-8` | `32px` |

### Radii (unchanged)

| Token | Value |
|---|---|
| `--r-sm` | `6px` |
| `--r-md` | `10px` |
| `--r-lg` | `16px` |
| `--r-full` | `999px` |

### Motion

| Token | Value | Use |
|---|---|---|
| `--dur` | `160ms` | Standard transition duration (hover / focus / UI state) |
| `--dur-spin` | `1818ms` | Record spin duration — 33rpm (one revolution at 33 1/3 rpm = 1818ms) |
