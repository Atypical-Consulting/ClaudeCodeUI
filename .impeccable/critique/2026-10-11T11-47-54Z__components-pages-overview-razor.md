---
target: Overview au repos
total_score: 26
max_score: 40
na_heuristics: 
p0_count: 0
p1_count: 3
target_identity: "file:/Users/phmatray/Repositories/Atypical-Consulting/public/ClaudeCodeUI/Components/Pages/Overview.razor"
target_fingerprint: "sha256:d460b7d328b4319a3f1810cc51b64d8bea048345950dd0339104105064a53b83"
target_path: /Users/phmatray/Repositories/Atypical-Consulting/public/ClaudeCodeUI/Components/Pages/Overview.razor
timestamp: 2026-10-11T11-47-54Z
slug: components-pages-overview-razor
---
# Critique 3: Overview at rest (Components/Pages/Overview.razor)
Score 26/40. H1 3 · H2 3 · H3 2 · H4 2 · H5 2 · H6 3 · H7 3 · H8 3 · H9 3 · H10 2.
Specificity: form authored and faithful to DESIGN.md; judgement (what fills the hero slot, what counts as a card) not yet.
Detector: 0 on Overview.razor; 3 browser findings hidden/page-level. Contrast fail: .ovcap .of 3.04 (11px). Margins 4.98 (.rc .rm on sel, .flag.cut on lead). Layout stable (229px). Enter kbd follows sel; sr status correct. 4 Ember elements in .ov at rest.
Refuted in part: "cards show Mon, never relative" — Fmt.Day past = time today / yesterday / Nd ago. Confirmed: header New session primary until cards load (<300ms flash).
## Priority issues
- [P1] Lead card wastes hero slot: Cut with no text shows only "Interrupted before replying", duplicating its flag. Fix: last user prompt or last tool+target; flag or fallback, not both. (clarify)
- [P1] Curation leaks: 2 running-elsewhere + 2 probes among 7; lead line says "No session running". Fix: running-elsewhere out of grid into lead sentence; stricter probe rule. (distill)
- [P1] Primary action does not follow selection: Resume stays on card 1 after J/K. Fix: Resume on selected card while focused; drop duplicate kbd. (harden)
- [P2] Digits navigate at once; hint "1–7 ⏎ open" mis-teaches. Fix: digit selects, Enter opens, Esc clears; exact hint. (harden)
- [P2] Urgency by layout not risk: 73 unpushed commits last; counts mismatch with rail; .of contrast 3.04. Fix: risky worktree clause in lead line; .of fg-4; shared noun. (layout)
## Minor
quota strip misaligned at 900; repo hidden <1100; 4-col orphan row >1550; machine ids in worktree col 2; $635.38 precision; stale quota >24h unmarked; empty history has no in-body action.
