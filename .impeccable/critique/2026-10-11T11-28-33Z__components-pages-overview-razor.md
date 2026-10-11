---
target: Overview au repos
total_score: 27
max_score: 40
na_heuristics: 
p0_count: 0
p1_count: 3
target_identity: "file:/Users/phmatray/Repositories/Atypical-Consulting/public/ClaudeCodeUI/Components/Pages/Overview.razor"
target_fingerprint: "sha256:10feb57b8c05969cc72dfd20c6370d6289a8d177e48411cca2a61acaf28a37e5"
target_path: /Users/phmatray/Repositories/Atypical-Consulting/public/ClaudeCodeUI/Components/Pages/Overview.razor
timestamp: 2026-10-11T11-28-33Z
slug: components-pages-overview-razor
---
# Critique 2: Overview at rest (Components/Pages/Overview.razor)
Score 27/40. H1 3 · H2 3 · H3 3 · H4 2 · H5 3 · H6 3 · H7 3 · H8 3 · H9 2 · H10 2.
Specificity: now authored (ending flags, risk-sorted worktrees, money sentence); triage half-done: New session still the loudest object.
Detector: 0 findings on Overview.razor; 7 browser findings all hidden/shell/page-level (one dark-glow #ffba00 = detector's own overlay). Contrast min 4.98 (.rc .rm on selected card). No layout shift (258px). Focus lands on .resume. 10 truncations at 900px incl. worktree risk reason.
Refuted: "bare clock times hide the day" — Fmt.Day shows yesterday / Nd ago.
## Priority issues
- [P1] Resume ⏎ hint stays on card 1 after J/K while Enter opens the selected card. Fix: kbd on selected card only. (harden)
- [P1] Competing primaries: header New session filled Ember beats outlined Resume; Ember has 4 meanings. Fix: header btn secondary at rest, Resume primary, selection bar neutral. (colorize)
- [P1] a role=listitem erases link role; J/K silent to AT. Fix: div listitem wrapper, sr-only role=status, title on truncated names. (harden)
- [P2] Junk passes filters: null-cost probe card; worktree-use-the-bash-tool-N litter. Fix: drop null-cost short sessions; collapse patterned worktree names per repo. (distill)
- [P2] Wrong-empty flash before recent loads; quota sentence repeats meter; "as of 12:20" clock not age. Fix: skeleton until loaded; consequence or no sentence; relative age. (clarify)
## Minor
"open" twice in hint; lead card dead band and missing cost; incomplete last row; lead looks permanently selected at arrival; risk reason truncated at 900.
