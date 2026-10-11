# ClaudeCodeUI specifics

## Test server

```bash
S=<scratch>; P=5323   # a port that isn't the user's instance
dotnet build -v q --artifacts-path $S/art-<slug>
pkill -f "$S/art-<slug>/bin/ClaudeCodeUI" ; \
(ASPNETCORE_ENVIRONMENT=Development ASPNETCORE_URLS=http://localhost:$P \
  nohup $S/art-<slug>/bin/ClaudeCodeUI/debug/ClaudeCodeUI > $S/server.log 2>&1 &)
# wait for the URL, then (and stop here if the build failed: never restart the old binary):
grep -o "http://localhost:$P/?token=[a-f0-9]*" $S/server.log | head -1 > $S/url.txt
```

Each launch prints a new one-time token (`Program.cs`): any URL captured earlier goes stale. The
harness takes it from `CCUI_URL` and swaps it for a cookie.

## Forbidden in the test server

- **Enter** on a session card/row: it opens a real `claude` session. Tell agents A and B.
- Starting a session, deleting a worktree, any button that acts on the disk.

## Fixtures

User data lives in `~/Library/Application Support/ClaudeCodeUI/`. Example: a quota on alert for the
Overview, `limits.json`:
`{"Limits":{"FiveHour":0.12,"FiveHourReset":"…","SevenDay":0.91,"SevenDayReset":"…","Status":"allowed"},"At":"…"}`.
Check whether the file exists before writing it: if it does, back it up and restore it at ship
time, don't delete it.

## Project traps

- The Read tool is blocked on `.cs`: read them with `sed`/`cat`.
- French resx: NBSP before `:` and the typographic apostrophe `’`. To replace, use regex, not an
  exact match.
- Razor trims the whitespace at the end of an expression: `@(L["x"] + " ")`.
- Keyboard on a Blazor Server page: the server round-trip races a burst of keys. Keep the selection
  state client-side (app.js) and move focus synchronously.
- WebKit doesn't always re-lay out a `flex-end` on resize. `display:grid; align-content:end` does.
- A Razor lambda that contains a string goes in a single-quoted attribute: `@onclick='() => x = "a"'`.
- A C# resource formatted with `{0}` that also contains `{base}` (filled in later) must be written
  `{{base}}`, otherwise formatting fails (the self-test catches it).
- WebKit doesn't reflect `ariaExpanded`: read `getAttribute('aria-expanded')`.
- A probe that lists processes (lsof) sees itself and the app's git commands: exclude them.
- Esc first leaves the focused control (app-wide convention): don't count it as a focus loss.
- The screen's spec lives in DESIGN.md (e.g. "### Overview at Rest"). Update it in the same PR.
