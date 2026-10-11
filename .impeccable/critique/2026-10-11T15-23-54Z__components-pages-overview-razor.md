---
total_score: 34
round: 18
target_identity: "file:/Users/phmatray/Repositories/Atypical-Consulting/public/ClaudeCodeUI/Components/Pages/Overview.razor"
target_fingerprint: "sha256:d460b7d328b4319a3f1810cc51b64d8bea048345950dd0339104105064a53b83"
target_path: /Users/phmatray/Repositories/Atypical-Consulting/public/ClaudeCodeUI/Components/Pages/Overview.razor
timestamp: 2026-10-11T15-23-54Z
slug: components-pages-overview-razor
---
# Critique round 18: Overview at rest, 34/40

H1 4 · H2 3 · H3 4 · H4 3 · H5 4 · H6 3 · H7 4 · H8 3 · H9 3 · H10 3

Fixed after this round:
- **Worktree reasons:** each row uses the Worktrees page's own sentence (`t.Why`), so the two pages never disagree. Ordering and risk ignore merged commits. Temp-root scratch repos are left out.
- **Quota:** the hot line says what is left and until when; resets are relative ("today", "tomorrow"); the duplicate sr text is gone.
- **Keys:**
  - ↓ from the bottom row enters the worktrees.
  - The rows' keys show while a row has focus.
  - The Esc hint shows only while the list has focus.
- **Layout:** the worktree list is a ledger between hairlines; an unknown cost shows "—".
- **Rail:** NavLinks are Tab-reachable in WebKit.
