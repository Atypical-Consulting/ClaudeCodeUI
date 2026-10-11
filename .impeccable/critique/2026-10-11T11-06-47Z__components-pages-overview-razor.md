---
target: Overview au repos
total_score: 22
max_score: 40
na_heuristics: 
p0_count: 0
p1_count: 3
target_identity: "file:/Users/phmatray/Repositories/Atypical-Consulting/public/ClaudeCodeUI/Components/Pages/Overview.razor"
target_fingerprint: "sha256:7f718630e342adcc1ae90e68d669b747f991129e7780b9fb44ca633a449b75d6"
target_path: /Users/phmatray/Repositories/Atypical-Consulting/public/ClaudeCodeUI/Components/Pages/Overview.razor
timestamp: 2026-10-11T11-06-47Z
slug: components-pages-overview-razor
---
# Critique: Overview at rest (Components/Pages/Overview.razor)
Score 22/40. H1 2 · H2 3 · H3 2 · H4 2 · H5 2 · H6 3 · H7 1 · H8 3 · H9 2 · H10 2.
Specificity: bottom half authored (ending-aware resume cards, unsaved worktrees); top half generic SaaS usage header; page leads with absence ("No session running").
Detector: 0 findings on Overview.razor; browser 5 page-level/hidden false positives; contrast min 5.37 (AA), cards equal height, 18px block rhythm.
## Priority issues
- [P1] Only real alarm is grey: 7d quota 91% same grey bar; worktree risk (73 unpushed) same weight as 1 dirty file. Fix: .hot class >=80% accent bar+pct, qwarn line, sort worktrees by risk, role=meter. (colorize)
- [P1] Layout shift: week block lands after cards and pushes grid ~150px. Fix: reserve slots/skeleton or load week before recent. (layout)
- [P1] No keyboard at rest: 1-6 open card, J/K cursor + Enter, focusIfIdle, kbd hint. (harden)
- [P2] Hierarchy leads with absence, all cards equal: headline = resume, featured first card span full width with Resume button, trivial sessions folded. (layout)
- [P2] Spend chart near-mute: linear scale outlier, no values, tooltip-only, .day collides with chat divider app.css:171. Fix: values for today/peak, no bar at 0, sqrt scale, aria, rename .ovday; or fold into tot and make quota a slim strip. (distill)
## Minor
tot labels wrap at 900px; worktree machine names lead rows; "read 12:20" -> relative; span.cap -> h3; strip emoji; empty-excerpt fallback; card aria-labelledby; worktree count vs rail badge.
