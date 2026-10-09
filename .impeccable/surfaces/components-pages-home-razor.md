---
version: 1
slug: "components-pages-home-razor"
primary_target: "Components/Pages/Home.razor"
related_targets: []
---

Scope: Claude Code UI web app shell (sessions, transcript, inspector). Mode: Operate.
Audience: one developer, local, dark environment, long agent sessions next to IDE.
Screens: start/new session, running session, permission request, multi-session overview, session switcher + error state.
Mascot: BlazorKawaii RubberDuck (#FCCC0A), mood mirrors session state.

## Direction contract
THESIS: The session is a workbench, not a chat. Fixed panels (sessions · thread · inspector) persist; navigation changes panel contents, never the layout. Refuses the centered chat-bubble column with a history sidebar.
OWN-WORLD: Graphite ground #0E0F11, panels separated by 1px seams #1F2125, one elevation step #16181B. Geist UI + Geist Mono for code/data. Tool kinds coloured from syntax tokens (Read #82AAFF, Grep #C792EA, Edit #C3E88D, Bash #FFCB6B, Write #89DDFF, error #F07178). Single primary accent #E8875B reserved for "needs you" and primary action. Destructive actions isolated by space, outline until focused. Radius 6-8px, no shadows except overlays.
STORY: Developer sees every session's state at a glance, reads what the agent did as a compact tool ledger, inspects any step in the inspector, and approves with the exact diff/command in front of them, keyboard-first.
FIRST VIEWPORT: 256px sessions rail left (status dots, waiting badge, duck at bottom), thread centre (prompt, prose, tool ledger, composer pinned bottom with turn meta), 400px inspector right showing the pending decision or the selected tool's output.
FORM: dark-first developer console (catalog challenger digital-design-canon-dark-first-developer-console), chosen by user after re-roll; seed key 060ba6af.
FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
