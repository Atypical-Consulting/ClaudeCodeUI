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
  syn-com: "#8A8F99"
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
- **Code** (Geist Mono, 13px, 1.65): code blocks, ledger rows and the inspector prose (13px), diffs, file views and meta lines (12px), `kbd` (11px). No half-pixel steps outside the 13.5px path inputs, the 14.5px assistant prose and the theme-card thumbnails.
- **Label** (600, 11px, 0.08em, uppercase): rail group captions and table headers. Status badges use 0.06em.

### Named Rules
**The Mono-for-Machines Rule.** Tool names, paths, commands, branches, model names, counts, costs, durations and shortcuts are set in Geist Mono with tabular figures. Human sentences never are.

## Layout

Fixed three-pane workbench. App: sessions rail 256px, thread `minmax(0,1fr)`, inspector 420px (the direction contract said 400px; the build is 420px); without an inspector the grid drops to two columns. Pane headers are 48px (52px on the site) with a 1px bottom seam. The thread column caps content at 760px, centred in the pane with 28-32px side padding (hiding the inspector widens the margins evenly, never a blank strip on one side); the composer is pinned at the bottom of the thread.

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
- **Rail items:** 7px 10px, 6px radius, 13px regular (session titles `fg-2`, branch and cost 11px mono `fg-4`; nav links 400 13px `fg-3`); hover `raise` + `fg`; current item `raise-2` with a 1px `seam-2` inset ring. Session rows lead with a status dot: idle ring, running green pulse, waiting Ember with halo, error red.
- **Tabs:** 500 13px, `fg-4`; active `fg` with a 2px accent underline.
- **Mobile (site):** rail becomes a sticky top bar, nav scrolls horizontally, the table of contents opens from a button.

### Overview at Rest
With no session running the Overview is a "pick up where you left off" page, never an empty screen, and it does the triage. No headline renders until the transcripts are read (a flash of "no session yet" would be a wrong empty state); a `fg-4` status line says "Reading recent sessions…" meanwhile. At rest the page body owns the one primary and the header carries no button (the rail's New session, `⌥N`, stays); with no history at all, the empty state carries its own primary New session.

**State line.** The headline names the purpose ("Pick up where you left off"); one line under it, held to 75ch, states the machine's truth: "Nothing running here · $9.44 today · $635 over 7 days" (sessions that may be running in another terminal are counted here only when they are not already named under the cards). It never wraps (ellipsis). It carries no alarm: each risk is said once, where it lives (the quota meter, the worktree row).

**Recent sessions.** Ages tick once a minute; the cards do not move until `R` takes a fresh snapshot (the lead and digits renumber), and after 2 minutes an `fg-4` line under them says so: "Read 12 min ago · R to read them again". The page is the recent list: on the Overview at rest the rail drops its own Recent group (an uncurated second list beside the cards would undo their triage; ⌘K still searches everything). A snapshot taken once the transcripts are read: cards, lead and counts never reshuffle while the page is open, so a digit always names the session it showed. A lead and full rows of three (1, 4 or 7 cards); with 2 or 3, the others share one row; in a two-column pane a last odd card takes the whole row. Left out, and counted under the cards in `fg-4` when the state line has not already said it ("Left out: 2 probes · ⌘K finds them."): sessions another terminal may be running (named on a `claude` command line, written in the last minute, or written in the last 30 min in a folder where a plain `claude` runs: doubt counts as running, since resuming a live session starts a second writer; the state line counts them as "may be running", and a `fg-4` line under the cards names them as links, "Maybe still open in a terminal: …" (each link's tooltip carries the warning), so the deliberate choice to open one anyway stays one click or one Tab away), a known cost under 5 cents, and an unknown cost that looks like a probe (no branch, or almost nothing written). The lead is the unfinished session of the last 24 h: one waiting on an `AskUserQuestion` first (a closing "?" in prose is an offer, not a wait), then one you interrupted or left without a reply; otherwise the newest. It spans the full width as two columns: the title (600 15px, flag beside it) and the excerpt at a reading measure on the left, and a right column (30 %, at least 200px) holding its facts top to bottom (age · cost on one line, branch · repo); grid cards carry age · cost at the right of their title row (an unknown cost is an `fg-4` "—", so the pair is always whole), the same pair in the same place, and their meta line is branch · repo then Resume with Resume at its foot, the card's bottom-right corner as on every card. Grid cards put the age (11px mono `fg-3`) at the right of the title row and their flag on its own 11px line under the excerpt, so it never competes with the title or the meta with its body saying what was happening: the last thing Claude wrote (three lines of 14px prose held to 75ch, read up to 512 KB back; every excerpt keeps the end of Claude's last message (about 220 characters on the lead, 110 on grid cards), from a sentence start or at least a word start, and is anchored at its end: when it is longer than the clamp the overflow falls off the top, under a fade that says so (an ellipsis would be clipped with it), never the last words, which carry the ask; a message barely over the budget (a fifth) is shown from its own start; when a question sits near the end it is the ask and stands alone), else the user's last prompt ("› …"), else "Last step:" and the last tool call as a ledger row; an unreadable transcript says so in italic `fg-4`. The others follow in a grid of at most three columns (500 13px title, two clamped lines, 11px mono meta). Flags: Ember "waiting on your answer"; `fg-3` "stopped mid-step", "you interrupted it" (the transcript ends on the user's interruption), or "no reply to your last message" (the user wrote last: the excerpt is then that message, "› …", since it is what is pending, not Claude's earlier question), or "ends on a question" (Claude closed on an offer: said, so its "?" is not mistaken for a wait). When you interrupted it, the excerpt is labelled `fg-4` "When you stopped it:", so a question there reads as where it stood, not as a pending ask.

**Keys.** The list is focused on arrival. Digits `1`–`7`, `J`/`K` and ←/→ (linear), ↑/↓ (the card above or below, nearest column, resolved on the client so a fast second key reads where the first went), `Home` and `End` only select, and stop at the ends (no wrap), except that `J` past the last card, or ↓ from the bottom row, steps into the worktree rows (focus moves on the client, so a key typed right after already acts there); `Enter` (spelled out: the ⏎ glyph reads as a broken icon at 11px) resumes; the first `Esc` goes back to the lead and keeps focus, the next one leaves the list. The one Ember button, **Resume** (one 12px size everywhere), sits at the right end of the chosen card's meta line, the same place on every card, lead included, and follows the selection (every meta line keeps a 76 × 24px slot for it, so nothing reflows or truncates differently when it lands; a long branch gives way three times faster than the repo, both with an ellipsis and neither below 4ch, the repo held to 17ch; each name keeps a floor of its own length up to 8ch, never more, so no blank sits before the "·"; full names in titles), so the primary and the key always name the same session. The flag is part of each card's accessible description. The selected card takes `raise`, an `fg-4` outline and a neutral 2px `fg-3` bar while the list has focus (the keys act), and a 60 % `raise` fill and a dim `fg-5` bar on its resting border when it does not (Resume never points at an unmarked card, and steps back to an outlined `fg-2` button so the selection never outweighs where the keys now act; the missing outline says the keys are elsewhere, so with nothing focused at all, where digits and `J`/`K` still act, the focused look stays); the selected link carries `aria-current`; hover only outlines a card (`seam-3` border), so it never outranks the selection; `W` moves focus into the worktree rows, which are Tab stops; digit `kbd`s and the hint line (digits, J/K, Enter, R read again, W; each key and its label never part, the caption and the line wrap as wholes, and below a 720px pane the Esc segment drops; the rest of the keys are in its tooltip) are always shown at full `fg-4` (brighter while the list has focus; the "Esc back to card 1" segment shows only while the list itself has focus, comes first and keeps its room when hidden, so the right-aligned line never jumps and its right edge stays flush); with nothing focused, a digit, `J`, `K`, `R` or `Enter` brings focus back to the list and acts there (nothing focused keeps the focused look, so Enter keeps its promise; from any other control it never opens), and a digit typed in a worktree row selects that card; `J`/`K` and the arrows walk the worktree rows too (`K` on the first row goes back up to the cards), and `Esc` in a row returns to the list; `W` works from anywhere too; an sr-only status announces "2 of 4 · title" when the selection moves (silent on arrival: the focused list names itself). Cards are links inside `listitem`s.

**Quota strip.** Under the sessions (the resume path comes first), except when a window is at 80 % or more: then it sits above them, since it decides whether resuming is wise. When a reading exists, a slim 8px `seam` strip: 5 h and 7 d meters (120px bar, mono figure, reset time; `role="meter"` with the reset in its value text) and the reading's age right ("read 1 h ago", the clock in a tooltip). A window at 80 % or more turns its bar and figure Ember; the meter is the alarm, no sentence repeats it, and its reset time brightens to `fg-2` (when it is safe again is the useful half). Staleness is per window, dimmed by colour, never opacity (the text stays above AA): the 5 h meter dims once the reading is an hour old, both dim after a day and the age then says "may be out of date". Below 1100px the caption sits left of the two meters, which stack aligned. While hot, a visible `fg-4` sentence under it says what is left and until when ("9 % of this week left."; the reset stays on its meter, said once); otherwise what it measures is in its caption's tooltip. Resets are said from today ("today 23:00", "tomorrow 19:59", then the weekday).

**Worktrees.** Last, the worktrees holding work git cannot get back elsewhere, by the Worktrees page's own rule (unpushed commits count only when not merged or squash-merged), riskiest first, each row giving the Worktrees page's own sentence for it (one source: the row and the page it opens never disagree), a repo under the temp root left out (an agent's scratch is never real work; the Worktrees page still lists it) (a cache under 2 minutes old shows at once, then a fresh scan patches the counts in place when the order holds, so the Worktrees page a row opens says the same), under a visible `fg-4` sentence saying what counts ("Uncommitted, or committed but never pushed."), which adds the rows' keys (`J K` move · `Esc` back to the cards) while a row has focus; the rows are a ledger between two hairlines, not a bordered card, so the sessions stay the visual centre, in fixed columns: the branch as the label (the folder for a detached HEAD; the Worktrees page leads with the same name, so the link lands on what was clicked), then the repo; focus on a row is the cards' neutral 2px `fg-3` bar plus an `fg-4` outline, the same weight as the focused card (`Home`/`End` jump to the first and last row); the caption's only link is "All worktrees →" (the cleanup count stays on the rail's badge, a different population, which says so: "70 safe to remove"); ages relative like the cards, a numbered series (`worktree-x`, `-2`, `-3`) folded into one row with "+2", at most 5 with "5 of N" (`fg-4`). Each row opens the Worktrees page on that worktree at once (it renders the last scan, then reconciles): the row is scrolled to the centre, takes focus itself (never its first button, which may be Delete) and pulses once from `raise-2` down to the focus `raise` (with the neutral bar), so it never flickers, and the fresh scan's re-render does not replay it; no lasting selection is left behind. Below 1100px the repo folds under the branch as an 11px second line, so same-named worktrees in two repos stay apart. Ten or more unpushed commits put their count in Ember. While the scan runs a `fg-4` status line says so; a failed scan names its cause with a Refresh button.

### Worktrees Page

**Rule.** A row is Safe when removing it loses nothing git cannot get back: already in the base branch (locally or on origin), squash-merged, or every commit held by a remote branch origin confirms it still has (a detached HEAD on `origin/x`, a clean branch pushed but not merged: the folder goes, the branch stays). Squash merges are found by content: the branch's whole diff, or its upstream's when the PR outgrew the local branch, has the patch-id of a base commit (read-only, and it survives the base moving on). A remote-tracking ref is a local cache, so origin is asked (`git ls-remote`, cached 2 min) before a ref counts: a ref to a branch origin deleted is named as such ("origin deleted origin/x"), and when origin cannot be reached the reason says the ref is unconfirmed; neither is Safe. Where git alone would say "not in the base", GitHub is asked when `gh` is there (read-only, merged PRs cached 10 min): a PR merged into the base whose head is the tip, or that the tip leads to (compare API), makes the row Safe as "Merged as PR #52"; a branch-only row of such a PR is named and not offered, since `branch -d` refuses a squashed branch. `git worktree remove` deletes ignored files without `--force`, so ignored files that are not known build output (`bin`, `obj`, `target`, `node_modules`, caches…) hold a row in Needs review and are named ("Ignored files that would go with it: .env"). Anything whose HEAD moved under an hour ago (created, committed, checked out: the worktree's reflog, which a `git status` elsewhere never touches) is held in Needs review ("Used 12 min ago, may still be in use"): a fresh agent worktree is clean and "merged" while the agent works. A process whose working directory is inside (a shell, an editor, a claude CLI) makes it Active, as does a live session or a lock held by a live pid. Unpushed commits count against every remote when the upstream is gone. A `worktree-<folder>` branch whose folder moved to another branch or a detached HEAD says whose it was ("Made for agent-x"), never "with no worktree"; while that folder is in use the branch is Active too, since there is nothing to decide. A stale lock held back only by recency says recency. One word for a merged branch no worktree checks out: orphaned (chip, fold, reason); its action is Delete, and Prune is kept for a folder already gone (git's own word).

**Layout.** Three groups, Safe to remove · Needs review · Active, each header followed by one `fg-4` line saying what the group means and what to do. Inside a group, riskiest first, in the Overview's order and tie-break (commits nothing else holds by count, then uncommitted files, newest first, unreadable, ignored files, recent, the rest; a merge already holds its commits; Safe lists branches, then detached folders), then repo and name: a fixed order, so the cached rows and the fresh scan list the same rows in the same place. No State column (the group header names it): the worktree (branch, then folder · repo, the folder first because it tells two rows on one branch apart; the folder gives way before the repo does, from its start, since agent folders share long prefixes ("…b43-34"); a detached HEAD is a short tag ("detached") that never truncates; full path as the cell's title), the reason (two lines at most, full sentence as title), the size, the row's one action (every verb that only plans ends in an ellipsis: "Delete…", "Delete all…", "Delete anyway…", and its title says nothing runs until Clean up; a row with uncommitted files names the first ones and says first when its commits are already safe, so no clamp hides it ("Branch already in main · 7 files changed: Composer.razor, CliProbe.cs +5", the idle time after its own "·") and offers Copy path beside New session; a row held by ignored files offers Copy path, confirmed in place, then "Delete anyway", which plans it with its ignored files named in the plan and re-checked under the same terms when it runs; an Active row held by a process offers Copy path, to find what uses it). The "N more need your decision" count in the headline line filters to them, and "Add the safe ones" sits beside the headline it acts on (it wraps under it in a narrow pane, never past the edge), then hands focus to the plan, never to Clean up, and keeps its place (hidden) while a plan is open, so the table never jumps; Cancel hands focus back to the row or fold that started the plan; a row's Delete… or Keep returns focus to its row, and every action that plans opens the inspector; the header counts the repos that have rows (the scanned count is its title). Orphaned branches, detached Safe folders, squash-merged Safe folders, squash-merged branches whose folder went (named, with their `git branch -D` to copy: the page never forces) and worktrees held only by the same ignored file (one "Delete all anyway…") fold into one row per repo and kind ("54 orphaned branches · ClaudeCodeUI", "30 squash-merged worktrees · ClaudeCodeUI", `Delete all…`), each with its own one-line meaning; a kind with one row in a repo stays a row. A fold takes the place of its first row in the group's order, so it keeps the risk order and the size sort. An open fold's rows are indented under a `fg-4` rule, so they read as inside it. The fold's toggle is not a second Tab stop (the row's keys open it). The worktrees held only by an ignored file keep the warning verb, "Delete all anyway…", wrapping in its cell rather than truncating. A chosen filter shows its rows unfolded (the fold toggles only without a filter); below 532px a fold's size joins its help line; a fold a deep link opened stays closable. The Size header sorts by size. Below a 680px pane the reason moves under the name; below 532px the size joins it there; hidden columns take zero width rather than `display:none`, so the full-width group rows keep no phantom track. The header row is sticky. Sizes are kept across visits and re-measured over the old ones, so the totals never blank. The filter chips carry their state's mark. Temp-root repos (an agent's scratch) get no `.gitignore` advice.

**Plan.** The inspector names rows as the table does (branch, then folder) and groups them per repo, folded once the plan passes 8 rows; past 12 commands the git preview folds behind "The N git commands"; the Clean button stays pinned at the bottom. Unplanning is "Keep", never a word that sounds like deleting.

**Keys.** The table is one Tab stop: the current row, then its buttons. `J`/`K`/↑/↓ walk the rows (folds included), `Home`/`End` jump, Enter, Space or → opens a fold, ← closes it and, from a row inside a fold, returns to the fold row, Home also scrolls the heading back, `Esc` from a button returns to its row and from a row to the filters; focus is never dropped on the page (Refresh stays focusable while it scans). Page controls carry an explicit `tabindex="0"` so WebKit's default Tab reaches them.

**Status.** Cached rows render at once; while a scan runs, "Looking for worktrees… 7 of 24 repositories" (a repository counts once read; the last scan's age is its title) replaces the header totals while it runs, beside Refresh, whose icon turns (still focusable); the "to reclaim" clause waits for the sizes, and the `.gitignore` warnings of the last scan show with the cached rows, so nothing pushes the table down when the scan lands. Row buttons name the row as it is shown (the branch); "New session" says it starts one in that folder.

### Tool Ledger (signature)
Bordered 8px container with a `sunk` header line ("3 outils · ..."). Each row is a mono grid: chevron, tool kind in its syntax colour (500), target in `fg` with ellipsis, meta in `fg-4`. Hover `raise`; selected or expanded rows get `raise` plus the 2px accent inset bar; a row waiting for permission is tinted 6% accent with accent meta; voided rows strike through. On the site rows expand (Enter or click) on a screenshot and explanation, animating `grid-template-rows` over 320ms.

### Permission Card (signature)
Inspector section on `sunk`: pane header "Permission" with a waiting pill ("1 sur N"), a 17px question, a one-line description, then the exact diff, command well or JSON, then the action row Autoriser (primary, `⏎`) · Toute la session (secondary, `Maj ⏎`) · Refuser (danger, `Suppr`, isolated right). On the site the same card performs the download; with no release published its third verb reads « Compiler depuis les sources ».

For `AskUserQuestion` the header reads "Question" and the body is the answer form instead: per question a `cap` header and a 15px question in a `fieldset`, then one option tile per choice (`field` fill, `seam-2` stroke, label in `fg` over its description in `fg-3`; picked = accent stroke on `accent-soft`), radios or checkboxes, and a last tile holding the free-text "Other". Answer (primary, `⏎`, disabled until every question has an answer) · Skip (danger). The form never takes focus on its own (a draft in the composer keeps it); `⏎` outside a field moves into the first option. In the Overview decision queue a question row has no Allow and `A` does nothing on it (Deny and Open stay): it is answered only from its session, and `LiveSession.Answer` refuses a bare allow for it. Answered calls show their answers as the ledger row's meta and as a question/answer list in the inspector's Output tab.

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
- **Don't** lay a session out as a chat-bubble column with a history sidebar; panes stay fixed (the reading column is centred in its pane, the panes are not).
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
