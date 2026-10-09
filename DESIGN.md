# Design

Visual system of Claude Code UI. The app (`wwwroot/app.css`) is the source of truth; the documentation site (`site/`) reuses it.

## World: console graphite

- Dark only. Five themes share one token set: Graphite (default), Encre, Ristretto, Mousse, Contraste élevé (`[data-theme]` in `wwwroot/app.css`, copied verbatim into `site/site.css`; keep them in sync).
- Working panes `--ground` #0E0F11, inspector `--sunk` #0B0C0E, raised blocks `--raise` #16181B (`--page` #08090A only behind the app shell), 1px seams (`--seam*`), accent `--accent` #E8875B (Graphite).
- Type: Geist (UI and reading) and Geist Mono (code, tool names, data), self-hosted variable woff2 (`wwwroot/fonts`, `site/fonts`).
- Tool colours: Read `--syn-fn`, Grep `--syn-kw`, Edit `--ok`, Bash `--syn-type`, Write `--syn-attr`, other tools `--fg-3`.
- Signature components: the tool ledger (`.ledger` / `.t` rows), the permission card (Autoriser / Toute la session / Refuser), the `.you` prompt block, the streamed caret.
- Mascot: BlazorKawaii `RubberDuck`; its mood follows state (happy, excited, sleepy, sad, shocked).

## Site (`site/`)

- `index.html` (Persuade): reads as a Claude Code session. Prompt, streamed answer, a three-row ledger, and an inspector whose permission card performs the download from the latest GitHub release. Feature ledger rows expand on mockup screens. The status line sits between the answer and the ledger and links to the card when it is not beside it (phones cannot install the desktop app, so the card may fall below the first screen there). With no release published, the card's third verb is « Compiler depuis les sources » instead of Refuser.
- `docs.html` (Read): same rail with a scroll-spy table of contents; calm reading column (16px Geist, prose capped at 47ch, about 70 characters since Geist runs narrower than `ch`) and, from 1240px, an inspector column showing the app screen for the section being read plus a link to the download card.
- Responsive: 3 columns ≥1240px, rail + one column ≥860px, top bar below. Reduced motion disables the stream, caret and transitions.

## Raster provenance

| File | Source |
|---|---|
| `site/assets/screens/*.png` | Copies of `docs/mockups/screens/` (approved mockups, synthetic data, labelled "maquette, données fictives") |
| `site/assets/favicon-32.png`, `icon-128.png` | Copies of `desktop/src-tauri/icons/32x32.png`, `128x128.png` |
| `site/assets/og.png` | Rendered by headless Edge from `.impeccable/og/og.html` at 1200x630 |
| `site/assets/favicon-32.png`, `icon-128.png` | Copies of `desktop/src-tauri/icons/` |
