---
name: critique-loop
description: Improve one screen by successive critique rounds until its Impeccable/Nielsen score reaches a target (default 35/40), with a screenshot each round, then ship it through a guarded PR. Use whenever the user wants a screen, page or view made better "until it scores X", asks for successive iterations / rounds of critique and fixes, says things like "améliore cet écran", "score >= 35", "itérations successives", "critique → fix → re-critique", "capture à chaque tour", or sets a /goal on a critique score, even if they don't name this skill.
---

# Critique loop

Raise one screen's critique score round after round until it reaches the target, then ship.
Each round: rebuild, screenshot, two independent assessments, a score, fix every finding.
It wraps Impeccable's `critique` protocol. It doesn't replace it: read
`<impeccable-skill-dir>/reference/critique.md` once for the scoring guide, then follow this loop.

Two reference runs on ClaudeCodeUI: the Overview took 19 rounds (22 to 37/40), the Worktrees page 7
(27, 27, 33, 31, 31, 35, then a confirmation round). Their history is in `.impeccable/critique/`, and
the advice below comes from those runs.

## Inputs (settle them before round 1, don't ask mid-loop)

- **Target:** the source file and its route, e.g. `Components/Pages/Worktrees.razor` at `/worktrees`.
  Also name the state under review ("at rest", "with a running session"…). A screen in two states
  is two targets.
- **Threshold:** default 35/40. Keep the user's operator: ">35" means 36 is required, "≥ 35" accepts 35.
- **Slug** for the screenshots: `~/Desktop/<slug>-v<N>.png`.

The user asked for a loop, not a conversation. Don't stop to ask what to do next. Report one
line per round and keep going until the threshold or a real blocker.

## Setup (once)

1. **Isolated branch.** Create a worktree with a branch that tracks nothing:
   `git worktree add --no-track -b <branch> .claude/worktrees/<slug> origin/main`.
   Without `--no-track` the branch tracks `origin/main` and `guarded-push.sh` fails (exit 128).
2. **Build outside the tree:** `dotnet build -v q --artifacts-path <scratch>/art-<slug>`
   (or the project's equivalent). A build in the repo can regenerate tracked files. Put the rebuild and
   the restart in one script that **stops on a build error**. Without that check, the old binary
   restarts with no warning and the round critiques a version that doesn't contain the fixes.
3. **Dedicated test server** on its own port, never the user's instance. For this project:
   `references/claudecodeui.md` (port, one-time token, keys never to press).
4. **Harness:** `npm i playwright` in a `<scratch>/pw` directory, then copy `scripts/harness.js`
   there. `node harness.js <route> <out.png> [w] [h] [engine]` takes a screenshot; `require` it for
   measurements. Default engine is WebKit, the one that runs the Mac app.
5. **Fixtures:** if the state under review needs data (a quota, a list), write it outside the repo
   and record every path in `<scratch>/fixtures.txt`. Shipping removes everything on that list.

## One round (N = 1, 2, …)

1. **Rebuild, run the project's self-test, restart the server, re-read the URL.** A server running
   the previous build gets the fix critiqued as if it weren't there. The self-test catches what the
   build lets through, for example a resource whose `{placeholder}` breaks the formatting. The token
   changes at each launch, so write the URL to a file (`<scratch>/url.txt`) that the agents read.
2. **Screenshot first:** take it at 1440 px, copy it to `~/Desktop/<slug>-v<N>.png` and `open` it.
   The user follows the progress through these captures. One per round, before the critique, so the
   image matches the score.
3. **Critique: two fresh agents, launched in parallel** (templates: `references/agent-prompts.md`):
   - **A, design review:** Nielsen's 10 heuristics out of 4, priority issues P1–P3 (what / why / fix),
     personas. Never give A the previous score or B's results: a reviewer who knows the last score
     anchors on it and the total drifts with expectations instead of with the screen.
   - **B, evidence:** the Impeccable detector (CLI + overlay), then numbers: contrast of every text
     class, clipping at 3 widths, focus order and focus rings, bursts of keys repeated (determinism),
     layout shift, console errors, consistency with the other screens that show the same data.
   Stay at 3 subagents at most. Don't kill or restart the server while they run, or B measures a
   dead server.
   **When the screen asserts facts** (states, reasons, "safe to delete"), B also runs a **truth
   audit**: every row and every help text, checked against the source (git, the API, the
   database), never against a cache or a rendering, with a count of false claims. That is what
   found the Worktrees page's biggest defect. A rule added in round 1 claimed "pushed to origin" on
   the strength of local refs that origin had deleted long before. Nothing else would have seen it.
4. **Synthesis:** the score is A's table, corrected where B's evidence contradicts it (a contrast
   failure caps H4/H8, a key race caps H3/H7). Persist it from the **main repo root**, not the
   worktree, or the file lands in the worktree and is lost:
   `IMPECCABLE_CRITIQUE_META='{"total_score":N,"max_score":40,"round":N}' <impeccable> critique-storage write <target> <body.md>`
5. **Decision:**
   - total meets the threshold → **Ship**.
   - otherwise fix **every** finding, P3 included. Fixing only the P1s lets the P3s come back next
     round and cap the score. Check each fix with the harness before the next round, and keep the
     screen's section in DESIGN.md in step with what changed.
   - When a critique questions a rule, **measure before fixing**: count the rows affected on the real
     data (for example 23 of the 86 worktrees held ignored files that weren't build output). The
     count decides the treatment: a filter, a state change, a warning.
   - A check script that clicks has to **wait for the server's round trip** (about 500 ms on Blazor)
     before reading the DOM. A read taken too early reports a broken feature that works, and can
     hide a real bug.
   - List the fixes you claim to B for the next round (fixed / partly / not). Renaming a CSS class
     had silently broken a keyboard shortcut; only this list caught it.
6. **Report:** one line, e.g. `Round 7: 31/40 (+2) · ~/Desktop/worktrees-v7.png · next: focus order, empty state`.

## When the score plateaus

Three rounds without progress means the fixes are cosmetic and A is still docking points on the
same things. Re-read the last three critiques: the issue that recurs is the ceiling. What broke the
31–33 plateau on the Overview:

- **Measure behaviour, not opinion.** Fast key bursts showed a race between the DOM and the render.
  Fixed client-side, 560 trials came back identical, and H3/H7 went from 3 to 4.
- **Agree with the other screens.** A reason or a count that differs from the dedicated page reads
  as a bug (H4). Reuse the same source rather than rewording.
- **Honest copy.** A label that promises more than the screen does (a "maybe", a stale figure shown
  as fresh) costs H1/H2. Say what you know and how old it is.
- **Remove instead of adding.** Duplicated debris (a list repeated in the rail) weighs on H8 more
  than any polish brings back. Thirty rows saying the same thing: fold them, and show the differences.
- **Look for a claim that's false.** A score that stagnates or drops despite correct fixes usually
  means the screen says something false somewhere, and A senses it without proving it. The truth
  audit finds it (Worktrees: 34, then 31, until it did; 35 once the claim was fixed).

## Ship

1. Remove the fixtures (`fixtures.txt`), stop the test server, run the detector one last time
   (expected: 0 findings).
2. Any edit after the passing round (copy, polish) is not scored. Either re-run a round or say so in
   the final summary. Never let it pass as part of the score. If the passing round reports a cheap,
   clear P1 (a layout broken at a common width), fix it and run **one** confirmation round rather
   than shipping it as is. Keep those fixes minimal, and check each one against the earlier
   findings: on the Worktrees page, two confirmation rounds fell back below the threshold because of
   regressions that the post-pass fixes themselves introduced (a tag that became truncatable, a
   verb shortened until it no longer warned). Two independent reviewers vary by ±1–2 points: once
   a confirmation round passes, ship that exact build, and list anything still open in the PR
   rather than reopening the loop.
3. Commit through `guarded-commit.sh <branch> -- -m …`, push through `guarded-push.sh <branch>`
   (paths given by the SessionStart hook), open a PR whose title carries the trajectory
   (`22 → 37/40`).
4. Wait for CI to finish (`gh run watch`, not `gh pr checks --watch`, which can return before the
   run appears), then `python3 ~/Repositories/repo-audit/pr_verdict.py <owner/repo>#<N>`. Merge
   through `guarded-pr-merge.sh` only on `vert` for the head sha.
5. Remove the worktree and the branch, pull `main`, write a `board_note`.
6. Final summary in the user's language: score trajectory, what made the difference, the last
   screenshot, any unscored edits, the P3 left open.
