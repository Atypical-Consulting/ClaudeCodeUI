# Design

Visual system of Claude Code UI. The app (`wwwroot/app.css`) is the source of truth; the documentation site (`site/`) reuses it.

## World: console graphite

- Dark only. Five themes share one token set: Graphite (default), Encre, Ristretto, Mousse, Contraste élevé (`[data-theme]` in `wwwroot/app.css`, copied verbatim into `site/site.css`; keep them in sync).
- Ground `--page` #08090A, surfaces `--raise` #16181B, 1px seams (`--seam*`), accent `--accent` #E8875B (Graphite).
- Type: Geist (UI and reading) and Geist Mono (code, tool names, data), self-hosted variable woff2 (`wwwroot/fonts`, `site/fonts`).
- Tool colours: Read `--syn-fn`, Grep `--syn-kw`, Edit `--ok`, Bash `--syn-type`, Write `--syn-attr`, other tools `--fg-3`.
- Signature components: the tool ledger (`.ledger` / `.t` rows), the permission card (Autoriser / Toute la session / Refuser), the `.you` prompt block, the streamed caret.
- Mascot: BlazorKawaii `RubberDuck`; its mood follows state (happy, excited, sleepy, sad, shocked).

## Site (`site/`)

- `index.html` (Persuade): reads as a Claude Code session. Prompt, streamed answer, a three-row ledger, and an inspector whose permission card performs the download from the latest GitHub release. Feature ledger rows expand on mockup screens.
- `docs.html` (Read): same rail with a scroll-spy table of contents; calm reading column (16px Geist, ~70ch).
- Responsive: 3 columns ≥1240px, rail + one column ≥860px, top bar below. Reduced motion disables the stream, caret and transitions.

## Raster provenance

| File | Source |
|---|---|
| `site/assets/screens/*.png` | Copies of `docs/mockups/screens/` (approved mockups, synthetic data, labelled "maquette, données fictives") |
| `site/assets/og.png` | Rendered by headless Edge from `.impeccable/og/og.html` at 1200x630 |
| `site/assets/favicon-32.png`, `icon-128.png` | Copies of `desktop/src-tauri/icons/` |
