---
total_score: 33
max_score: 40
round: 1
target_identity: "file:/Users/phmatray/Repositories/Atypical-Consulting/public/ClaudeCodeUI/Components/Pages/Appearance.razor"
target_fingerprint: "sha256:b455993ddbe8773769dcf8db94892d86191b7f06d265587e34fc963a9d994424"
target_path: /Users/phmatray/Repositories/Atypical-Consulting/public/ClaudeCodeUI/Components/Pages/Appearance.razor
timestamp: 2026-10-11T22-36-46Z
slug: components-pages-appearance-razor
---
# Settings (/settings/appearance), round 1: 33/40

Method: dual-agent (A design review · B detector + measurements + truth audit)

A scored 35/40 (H1 3, H2 3, H3 4, H4 3, H5 4, H6 4, H7 3, H8 3, H9 4, H10 4). Corrected by B's evidence:
- H1 3 → 2: after ⌘K "theme" on this page the theme changes but the cards keep showing the old one selected (aria-pressed="true").
- H10 4 → 3: "Code blocks, diffs, inspector" is false (diffs and inspector stay 12px); "remembered for next time" is false on desktop (localStorage resets per launch); Thinking, Version (web) and Notifications hints misleading.

Priority issues:
- P1 With WebKit's default Tab, no button on the page is reachable (theme cards, sizes, toggles, Apply).
- P2 Code size effect invisible: Preview ~450px below. P2 "Appearance" holds behaviour and About rows; no grouping.
- P2 Truth: 1 false + 4 misleading hints, stale theme cards after the palette.
- P3 Focused card ringed in its own accent looks selected; Apply has no dirty state; no default marked; no hover on segments; hints not aria-describedby; theme grid unlabeled; single-choice groups are aria-pressed buttons without arrows; 46ms unselected flash on arrival; lead line ~93ch; preview frame inside frame.

Detector: CLI 0; overlay 51, 1 real (lead line length).
