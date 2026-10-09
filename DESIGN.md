---
name: Claude Code UI
description: Dark developer console for the Claude Code CLI; the session is a workbench, not a chat.
colors:
  page: "#08090A"
  ground: "#0E0F11"
  sunk: "#0B0C0E"
  raise: "#16181B"
  raise-2: "#1A1C20"
  field: "#121316"
  hair: "#1A1C1F"
  seam: "#1F2125"
  seam-2: "#2A2D33"
  seam-3: "#3A3E46"
  fg: "#EDEEF0"
  fg-2: "#C7CAD0"
  fg-3: "#9095A0"
  fg-4: "#83888F"
  fg-5: "#5C6068"
  prose: "#BCC0C7"
  accent: "#E8875B"
  accent-ink: "#1A0D06"
  ok: "#C3E88D"
  err: "#F07178"
  syn-kw: "#C792EA"
  syn-str: "#C3E88D"
  syn-fn: "#82AAFF"
  syn-type: "#FFCB6B"
  syn-num: "#F78C6C"
  syn-com: "#7F848E"
  syn-attr: "#89DDFF"
  syn-var: "#F07178"
  code-inline: "#E6C07B"
typography:
  display:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "clamp(30px, 3.6vw, 46px)"
    fontWeight: 600
    lineHeight: 1.08
    letterSpacing: "-0.032em"
  headline:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "22px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "-0.02em"
  title:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 600
    lineHeight: 1.3
    letterSpacing: "-0.01em"
  body:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.5
  reading:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "16px"
    fontWeight: 400
    lineHeight: 1.7
  code:
    fontFamily: "Geist Mono, ui-monospace, Consolas, monospace"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.65
  label:
    fontFamily: "Geist, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 600
    letterSpacing: "0.08em"
rounded:
  xs: "4px"
  r: "6px"
  r-lg: "8px"
  pop: "10px"
  overlay: "12px"
  full: "999px"
spacing:
  xs: "4px"
  sm: "8px"
  md: "12px"
  lg: "16px"
  xl: "24px"
  2xl: "32px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-ink}"
    typography: "{typography.body}"
    rounded: "{rounded.r}"
    padding: "7px 12px"
  button-secondary:
    backgroundColor: "transparent"
    textColor: "{colors.fg-2}"
    rounded: "{rounded.r}"
    padding: "7px 12px"
  button-secondary-hover:
    backgroundColor: "{colors.raise}"
    textColor: "{colors.fg}"
  button-danger:
    backgroundColor: "transparent"
    textColor: "{colors.err}"
    rounded: "{rounded.r}"
    padding: "7px 12px"
  prompt-block:
    backgroundColor: "{colors.raise}"
    textColor: "{colors.fg}"
    rounded: "{rounded.r-lg}"
    padding: "12px 14px"
  ledger-row:
    backgroundColor: "transparent"
    textColor: "{colors.fg}"
    typography: "{typography.code}"
    padding: "8px 12px"
  ledger-row-selected:
    backgroundColor: "{colors.raise}"
  composer:
    backgroundColor: "{colors.field}"
    textColor: "{colors.fg}"
    rounded: "{rounded.r-lg}"
    padding: "12px 12px 10px 14px"
  input:
    backgroundColor: "{colors.field}"
    textColor: "{colors.fg}"
    rounded: "{rounded.r}"
    padding: "10px 12px"
  inspector-section:
    backgroundColor: "{colors.sunk}"
    textColor: "{colors.fg-3}"
    padding: "18px 20px"
  chip:
    backgroundColor: "transparent"
    textColor: "{colors.fg-3}"
    rounded: "{rounded.full}"
    padding: "2px 9px"
  pill-wait:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-ink}"
    rounded: "{rounded.full}"
    padding: "1px 7px"
  nav-item-active:
    backgroundColor: "{colors.raise-2}"
    textColor: "{colors.fg}"
    rounded: "{rounded.r}"
    padding: "8px 10px"
  kbd:
    backgroundColor: "{colors.field}"
    textColor: "{colors.fg-3}"
    rounded: "{rounded.xs}"
    padding: "0 5px"
---

# Design System: Claude Code UI

The app stylesheet (`wwwroot/app.css`) is the source of truth. The documentation site (`site/site.css`) copies its token block verbatim; keep the two in sync. Token values above are the default Graphite theme.

## Overview

**Creative North Star: "Console Graphite"**

The session is a workbench, not a chat. Fixed panes (sessions rail, thread, inspector) persist; navigation changes what a pane shows, never the layout. Everything the agent did is read as a compact tool ledger in monospace, and every decision is taken with the exact diff or command in front of the developer, keyboard first.

The world is dark only, low-chroma graphite with depth carried by tonal steps and 1px seams rather than shadows. Colour is spent on meaning: syntax hues name tool kinds, green and red mean added/removed or ok/error, and a single warm accent marks what needs the user. Density is high but calm: 13-14px UI text, generous line-height in prose, tabular numbers wherever figures align.

Five themes (Graphite, Encre, Ristretto, Mousse, Contraste élevé) override the same variable set through `[data-theme]`; components reference variables only, so every rule here holds in every theme. A BlazorKawaii `RubberDuck` mascot sits at the foot of the rail and its mood mirrors state (happy, excited, sleepy, sad, shocked).

**Key Characteristics:**
- Dark only; five themes, one token set.
- Panes separated by 1px seams; one raised step for blocks.
- Geist for people, Geist Mono for machines.
- Tool kinds coloured from syntax tokens.
- One accent, spent on attention, action, selection and focus.
- Flat at rest; drop shadows only on floating overlays.

## Colors

Near-black graphite neutrals, a syntax-highlighter hue set doing semantic work, and one warm accent.

### Primary
- **Ember** (`accent`): the "needs you" colour. Waiting status dots and pills, the pending ledger row, the primary button (Autoriser, Envoyer), the selected tab underline, the 2px selection bar, focus outlines, the text caret and the brand square. Text on it is **Ember Ink** (`accent-ink`). Derived: `--accent-soft` (12% mix, halos and selected fills) and `--accent-hover` (85% mix with white).

### Secondary
- **Syntax set** (`syn-kw`, `syn-fn`, `syn-type`, `syn-attr`, `syn-num`, `syn-str`, `syn-com`, `syn-var`, `code-inline`): highlight.js colouring and, through aliases, tool kinds: Read `--read`=`syn-fn`, Grep `--grep`=`syn-kw`, Edit `--edit`=`ok`, Bash `--bash`=`syn-type`, Write `--write`=`syn-attr`, web tools `--web`=`syn-num`, any other tool `fg-3`. Links in prose use `syn-fn` with a 40% underline.
- **Signal** (`ok`, `err`): added lines, success, running pulse; removed lines, failures, destructive buttons. Their text-weight mixes `--ok-fg` / `--err-fg` (65% toward `fg`) keep long runs readable.

### Neutral
- **Page** (`page`): behind the app shell only.
- **Ground** (`ground`): working panes (thread, rail) and the site background.
- **Sunk** (`sunk`): the inspector, ledger and code-block headers, table heads.
- **Field** (`field`): inputs, composer, command and diff wells.
- **Raise / Raise 2** (`raise`, `raise-2`): the one elevation step (prompt block, hover, selected row) and the active-item fill.
- **Seams** (`hair`, `seam`, `seam-2`, `seam-3`): row dividers, pane borders, control strokes, hover strokes, in that order of weight.
- **Ink ramp** (`fg` to `fg-5`, `prose`): headings and values, body UI, secondary, meta, disabled/markers; `prose` for assistant text.

### Named Rules
**The Needs-You Rule.** The accent marks only attention (waiting, pending decision), the primary action, the current selection and keyboard focus. It is never a decorative fill, a heading colour or a highlight on marketing words.

**The Syntax-Is-Tools Rule.** Tool kinds take their colour from the syntax tokens through `--read`/`--grep`/`--edit`/`--bash`/`--write`/`--web`. No new hue is introduced for a tool; an unmapped tool is `fg-3`.

**The One Token Set Rule.** Components reference variables, never hex. A new theme is a new `[data-theme]` block overriding the same names, and must stay dark.

## Typography

**Display Font:** Geist (with system-ui, sans-serif)
**Body Font:** Geist (with system-ui, sans-serif)
**Label/Mono Font:** Geist Mono (with ui-monospace, Consolas, monospace)

**Character:** One neutral grotesk for everything a person says or reads, its mono sibling for everything the CLI emits. Both are self-hosted variable woff2 (`wwwroot/fonts`, `site/fonts`).

### Hierarchy
- **Display** (600, clamp 30-46px, 1.08, -0.032em): the site's opening line only; docs `h1` runs clamp 32-42px.
- **Headline** (600, 22px, -0.02em): app screen titles (overview, worktrees, extensions, appearance); the new-session greeting is 24px; docs `h2` 24px.
- **Title** (600, 17px, 1.3, -0.01em): inspector section headings (the permission question). Pane header titles are 600 13px; markdown `h3` 15px.
- **Body** (400, 14px, 1.5): app UI. Assistant prose 14.5px/1.7 in `prose`; the site body is 15px/1.55.
- **Reading** (400, 16px, 1.7): docs column, prose capped at 47ch (about 70 characters, Geist runs narrower than `ch`).
- **Code** (Geist Mono, 13px, 1.65): code blocks, ledger rows (13-13.5px), diffs and file views (12-12.5px), meta lines and `kbd` (11-12px).
- **Label** (600, 11px, 0.08em, uppercase): rail group captions and table headers. Status badges use 0.06em.

### Named Rules
**The Mono-for-Machines Rule.** Tool names, paths, commands, branches, model names, counts, costs, durations and shortcuts are set in Geist Mono with tabular figures. Human sentences never are.

## Layout

Fixed three-pane workbench. App: sessions rail 256px, thread `minmax(0,1fr)`, inspector 420px (the direction contract said 400px; the build is 420px); without an inspector the grid drops to two columns. Pane headers are 48px (52px on the site) with a 1px bottom seam. The thread column caps content at 760px with 28-32px side padding; the composer is pinned at the bottom of the thread.

Site: rail 248px, column, inspector 432px; docs rail 272px with a scroll-spy table of contents and a 400px inspector showing the app screen for the section being read. Below 1240px the inspector folds under the intro (index) or disappears (docs); below 860px the rail becomes a sticky top bar and everything is one column.

Rhythm runs on 4px with 8/12/16/24/32 doing most of the work; list rows are 8-11px tall-padded, sections 18-22px. Reduced motion disables the stream, caret, pulses and all transitions.

## Elevation & Depth

Flat and tonal. Depth is read from the neutral steps (`page` < `sunk` < `ground` < `field` < `raise` < `raise-2`) and from 1px seams between panes, never from shadows on resting surfaces. Inset box-shadows are used as strokes and state markers, not as elevation: the 2px accent left bar on a selected row, the 1px `seam-2` ring on the active rail item, 3px `accent-soft` halos on waiting dots, focused composer and selected theme card.

### Shadow Vocabulary
- **Overlay** (`box-shadow: 0 24px 60px -10px #000c, 0 0 0 1px #00000080`): the command palette and the reconnect dialog, over a blurred `sunk` scrim.
- **Popover** (`box-shadow: 0 18px 40px -12px #000c`): slash-command and model menus, and the unhandled-error toast.

### Named Rules
**The Seam Rule.** Resting surfaces separate by tone and a 1px seam. A drop shadow means the element floats above the workbench and will go away.

## Shapes

Gently squared. Controls, inputs, code wells and list items take 6px (`--r`); containers (prompt block, ledger, composer, code blocks, theme cards) take 8px (`--r-lg`); `kbd`, badges, tags and inline code 4px; popovers 10px, the palette 12px. Pills, chips, status dots and toggles are fully round. Strokes are always 1px except the 2px selection bar, the 2px tab underline and the `kbd` 2px bottom edge.

## Components

### Buttons
Quiet outlines; only the primary action is filled.
- **Shape:** 6px radius, 1px `seam-2` stroke, 500 13px Geist, 7px 12px (site 8px 13px), optional trailing `kbd`.
- **Primary:** filled Ember with Ember Ink, weight 600. Hover lifts to `--accent-hover`.
- **Secondary:** transparent; hover gains `raise` fill, `seam-3` stroke, `fg` text.
- **Ghost:** no stroke, `fg-3`.
- **Danger:** `err` text with a 25-30% `err` stroke, filled only on hover; in action rows it is pushed to the far end (`margin-left:auto`) so it is isolated by space.
- **Focus:** 2px accent outline, 2px offset. Disabled: 40% opacity (site 55%).

### Chips, pills, badges
- **Chip:** round, 1px `seam` stroke, 12px mono `fg-3`; selected filter gains `raise` and `seam-3`.
- **Pill:** round 600 11px; waiting = Ember fill; error = `err` inset ring.
- **Badge / tag:** 4px radius, uppercase badges for allowed/denied with a tinted inset ring; mono tags with `seam-2` stroke, accent variant on `accent-soft`.

### Cards / Containers
- **Corner Style:** 8px.
- **Background:** `raise` for the prompt block, `ground` with a `sunk` header strip for ledgers and workflow panels, `sunk` for code blocks, `field` for command and diff wells.
- **Shadow Strategy:** none (see Elevation & Depth).
- **Border:** 1px `seam`; rows inside divide with `hair`.
- **Internal Padding:** 12-14px.

### Inputs / Fields
- **Style:** `field` background, 1px `seam-2` stroke, 6px radius (composer 8px), mono 13.5px for path inputs; caret is the accent.
- **Focus:** stroke shifts to 50% accent with a 3px 10% accent halo (composer `:focus-within`).

### Navigation
- **Rail items:** 8px 10px, 6px radius, `fg-3`; hover `raise` + `fg`; current item `raise-2` with a 1px `seam-2` inset ring. Session rows lead with a status dot: idle ring, running green pulse, waiting Ember with halo, error red.
- **Tabs:** 500 13px, `fg-4`; active `fg` with a 2px accent underline.
- **Mobile (site):** rail becomes a sticky top bar, nav scrolls horizontally, the table of contents opens from a button.

### Tool Ledger (signature)
Bordered 8px container with a `sunk` header line ("3 outils · ..."). Each row is a mono grid: chevron, tool kind in its syntax colour (500), target in `fg` with ellipsis, meta in `fg-4`. Hover `raise`; selected or expanded rows get `raise` plus the 2px accent inset bar; a row waiting for permission is tinted 6% accent with accent meta; voided rows strike through. On the site rows expand (Enter or click) on a screenshot and explanation, animating `grid-template-rows` over 320ms.

### Permission Card (signature)
Inspector section on `sunk`: pane header "Permission" with a waiting pill ("1 sur N"), a 17px question, a one-line description, then the exact diff, command well or JSON, then the action row Autoriser (primary, `⏎`) · Toute la session (secondary, `Maj ⏎`) · Refuser (danger, `Suppr`, isolated right). On the site the same card performs the download; with no release published its third verb reads « Compiler depuis les sources ».

### Crash Card and Status Screens
The `.crash` card (8px, 25% `err` stroke over a 5% `err` tint, duck left, text right) says "something broke" everywhere: a crashed session, a failed API turn, and the HTTP status pages. `StatusScreen` centres it in the thread column under a normal pane header: a mono `code · path` line, a 17px heading, one sentence naming the recovery, then the action row (primary New session or Reload, secondary Overview, ghost Copy request ID). A 404 is not a failure, so it takes `.crash.calm` (`seam` stroke on `sunk`, no tint). Duck moods: Sad for 404, Shocked for other 4xx, Ko for 5xx and crashes. Status pages render as static SSR (`[ExcludeFromInteractiveRouting]`) so a re-executed status keeps its copy. Requests refused before the pipeline (desktop token 403, host filter 400) stay bare by design.

### Reconnect Dialog and Error Toast
The reconnect dialog is the one modal that interrupts: the UI is dead until the circuit returns. Overlay vocabulary (raise, `seam-2`, 12px, Overlay shadow), duck plus a 17px title per state (Connection lost, Can't reach the server, Page paused, Couldn't resume), one sentence, and either a 2px `fg-3` sweep on a `seam-2` track while rejoining or the state's actions (Retry or Resume primary, Reload secondary). Focus moves to the action, else the title; Escape does not dismiss it. The unhandled-error toast floats over the thread column with the Popover shadow, a 6% `err` tint and a KO duck, Reload primary and an icon dismiss. `.blazor-error-boundary` uses the crash tint with an `err` dot.

### Prompt Block and Stream
The user's turn is a `raise` block with a mono meta line ("toi · maintenant"). Assistant text streams in `prose` behind an accent block caret blinking at 1s steps; the stream plays once and is removed under reduced motion.

## Do's and Don'ts

### Do:
- **Do** take every colour from a variable so all five themes hold; add a theme only as a dark `[data-theme]` block.
- **Do** colour a tool by its kind alias (`--read`, `--grep`, `--edit`, `--bash`, `--write`, `--web`) and fall back to `fg-3`.
- **Do** mark selection with `raise` plus the 2px accent inset bar, and waiting with the Ember dot and its 3px `accent-soft` halo.
- **Do** set machine output in Geist Mono with tabular figures.
- **Do** keep destructive actions as outlines isolated at the end of the action row, with their keyboard shortcut shown.
- **Do** use the 150ms `cubic-bezier(.2,.8,.2,1)` transition for state changes and honour `prefers-reduced-motion`.

### Don't:
- **Don't** use a light ground anywhere; the product is dark only.
- **Don't** lay a session out as a centred chat-bubble column with a history sidebar; panes stay fixed.
- **Don't** put drop shadows on resting cards, rows or panes; shadows belong to the palette and popovers only.
- **Don't** spend the accent on decoration, headings or emphasised words.
- **Don't** introduce a hue outside the token set for a new tool, state or chart.
- **Don't** hard-code hex values in components (the duck's `#FCCC0A` body is the mascot's own colour, not a token).

## Raster provenance

| File | Source |
|---|---|
| `site/assets/screens/*.png` | Copies of `docs/mockups/screens/` (approved mockups, synthetic data, labelled "maquette, données fictives") |
| `site/assets/favicon-32.png`, `icon-128.png` | Copies of `desktop/src-tauri/icons/32x32.png`, `128x128.png` |
| `site/assets/og.png` | Rendered by headless Edge from `.impeccable/og/og.html` at 1200x630 |
