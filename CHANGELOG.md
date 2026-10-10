# Changelog

## [0.3.0](https://github.com/Atypical-Consulting/ClaudeCodeUI/compare/v0.2.0...v0.3.0) (2026-10-10)


### Features

* **session:** open the session first, write the prompt in its composer ([#98](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/98)) ([e6c8c8c](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/e6c8c8c502692a64924ae6ddd4d3ba3d4b5d6011))
* **session:** parity quick wins with the Claude Code CLI ([#101](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/101)) ([9aba062](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/9aba06298bccbcb9e847db0e01c5b360c989caef))


### Bug Fixes

* **thread:** hide the skill body the CLI injects as a user message ([#95](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/95)) ([6fa9262](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/6fa9262e2c46f43df037555688e57cd3885ccdc7))
* **ui:** 36 visual bugs from a WebKit /impeccable audit ([#97](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/97)) ([2f0d9c8](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/2f0d9c895ba4f7b3d258b14e1873abfd7947bffb))

## [0.2.0](https://github.com/Atypical-Consulting/ClaudeCodeUI/compare/v0.1.0...v0.2.0) (2026-10-10)


### Features

* **composer:** @ file mentions with autocomplete ([#81](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/81)) ([ce23299](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/ce23299e9fb99942719af01838b0eeddf029d722))
* **composer:** prompt history with ArrowUp / ArrowDown ([#82](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/82)) ([f568796](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/f56879639cc608f663a87ecd9be763462d7e7a6f))
* **desktop:** check for updates with the Tauri updater ([#72](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/72)) ([9b41b8b](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/9b41b8bc9fb8ab2aa1d5f6eba79648f94aeed78e))
* **extensions:** read-only Hooks tab and hook activity rows ([#88](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/88)) ([d59acd5](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/d59acd5fd7e942781f73c1ecddf5735b685e3d0e))
* **extensions:** sign in to needs-auth MCP servers ([#89](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/89)) ([7bf84c4](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/7bf84c4194d7aa3dc9df626f0c56d44c47fd4531))
* **memory:** view and edit the session's CLAUDE.md memory files in the inspector ([#90](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/90)) ([51594d5](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/51594d57b89ad14e86715da51f7ac566e78eadc4))
* **notifications:** desktop and browser notifications when a session needs a decision or finishes a turn ([#84](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/84)) ([6f40cd1](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/6f40cd1e9c7ba4a73eadd7c268b684a89b17938d))
* **overview:** accurate run cost, actionable decision queue, empty states and session navigation ([#67](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/67)) ([93b9b6e](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/93b9b6e4ccb15889e973233511c2ae68e35e96b2))
* **palette:** search past session messages from Ctrl K ([#92](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/92)) ([18dd61b](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18dd61be40bdba21309044e5b0ec0aa06e27f9ad))
* paste, drop or pick images in the composer and send them as stream-json image blocks ([#80](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/80)) ([9dbdd2d](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/9dbdd2ddd73c223a0fc1dd27a8292e97f49999b3))
* **permissions:** plan approval card for ExitPlanMode ([#77](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/77)) ([80c1d63](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/80c1d63c338ed12bb24fe989fb999242c89babce))
* **session:** additional working directories (--add-dir), live add, kept on resume ([#91](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/91)) ([d57fbb5](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/d57fbb58205f818b6bd5abef042ad7fdd32f5312))
* **session:** AskUserQuestion answer form ([#78](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/78)) ([8a518a9](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/8a518a90a9fbb989c7c5eb3efc0592c58926a952))
* **session:** background tasks panel (shell run_in_background and Monitor) with live output and Stop ([#87](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/87)) ([84477bd](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/84477bd026fcab0a0ac8fe67b0ce3b91de6e2787))
* **session:** end a session, keep viewed history out of Active, flag sessions running elsewhere ([#56](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/56)) ([aaf20ae](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/aaf20ae88c50f058f8cf1a8638856869ddfecc8e))
* **session:** fork a session from the header, the palette or any reply ([#86](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/86)) ([7901c96](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/7901c96ac153f6ef8b0878ea621919b09b5c8933))
* **session:** pinned task list from TodoWrite / TaskCreate / TaskUpdate ([#79](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/79)) ([a130e22](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/a130e2222bda4dcb359cadc0c2b8ae49e36bc75b))
* **session:** rewind a conversation and its files to a user message ([#85](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/85)) ([68318a5](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/68318a56289ec5c30818dbaa444b7df21708f78a))
* **session:** send messages while a turn is running ([#83](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/83)) ([ed7db9a](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/ed7db9ad197ce53c721f11ecc2e277d02d7d6c99))
* **workflow:** ultracode Workflow runs in the thread and the inspector ([#94](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/94)) ([8d121a7](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/8d121a733271c0e685a18500c0b1fa6e76b74fe0))


### Bug Fixes

* **a11y:** announce streaming, status, completion, permission requests and form errors ([#57](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/57)) ([c33056a](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/c33056a00dc4f02769e5e12cbfccca2de2b2c3e7))
* **a11y:** landmarks, h1, modal command palette, tabs/radios keyboard and control names ([#65](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/65)) ([1518068](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/1518068f9843b44639dc9cbaae712174b1f7cbd1))
* **a11y:** unclipped focus rings, reduced motion, WCAG contrast fixes ([#64](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/64)) ([5b58186](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/5b5818640b323c99f253038fc9592650bd8b9ea7))
* **composer:** key session children, keep drafts per session and never lose a prompt ([#50](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/50)) ([c860b2c](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/c860b2c8b0c79bac2a9bfb04d990012f118ddb9f))
* **composer:** raise SignalR receive limit so large pastes do not drop the circuit ([#49](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/49)) ([9d14bfc](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/9d14bfce9466690c1deda6835cc73f6b96120625))
* **core:** bound session memory, stderr capture and shutdown time ([#61](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/61)) ([24134b5](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/24134b56ee732f5b8a353ac6d847d8c8aaa341e8))
* **core:** guard reader loop, parsing, spawn and open races, and thread-safety ([#63](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/63)) ([89a0349](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/89a034956deab8f8fd43e06119eb5b09814b9160))
* **desktop:** enable CSP, open links externally, time-box login shell, surface server crashes ([#60](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/60)) ([55760a5](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/55760a50270543064624a56b1ac49fbba17ac935))
* **diff:** scrollable permission diff and bounded, safe diff preview reads ([#55](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/55)) ([5c67484](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/5c67484b372fa672b2eac6aa9697cca118d696f1))
* **i18n:** localize hard-coded labels, plurals, French typography and platform key hints ([#69](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/69)) ([4938652](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/4938652133157907c7224009733c386e6584cc5e))
* **i18n:** localized, actionable error and exit messages; no French protocol strings ([#68](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/68)) ([a9aed69](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/a9aed69a97651a10a8d63f984f5c89963cddd59e))
* **layout:** responsive columns and overflow handling for narrow windows and long content ([#66](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/66)) ([3af9ea7](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/3af9ea784355cb7bdadf576add83c9709f6593a8))
* **new-session:** fresh branch decision on Start, unique worktree names, model/effort pick ([#59](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/59)) ([7cd3e0c](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/7cd3e0c2fdd857b272378e9afd92478cc4c1c165))
* **permissions:** keep a pending request until the CLI reply succeeds; label deny-and-send ([#53](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/53)) ([08cf36f](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/08cf36fd27cfb2418ad2b50300f854e44fd166a1))
* **permissions:** safe approve/deny shortcuts and an always-reachable permission card ([#52](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/52)) ([e364f6e](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/e364f6eb256e271802400c6b04ca52f20f6b9bae))
* post-merge regressions from the audit wave ([#74](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/74)) ([06c1377](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/06c13774c6eedd3a796a728c85780824dcd0119f))
* pre-release polish (palette focus trap, fresh rail costs, update-check answers) ([#75](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/75)) ([fc82b91](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/fc82b91029f710efda104ac63112cfe4719797f5))
* **security:** token-gate web mode, restrict AllowedHosts, block framing and cross-site state changes ([#51](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/51)) ([cd84604](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/cd84604ffdfc2606b3df4c9d09c3c6946e2b329a))
* **session:** distinct [@key](https://github.com/key) for the inspector so session pages render ([#73](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/73)) ([6a0844b](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/6a0844bf76a7392678ccf68bb96424ea8d82b789))
* **session:** keep permission mode on resume, allow changing it mid-session, confirm auto ([#54](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/54)) ([45d7038](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/45d7038436e9388218899e6e9e6c84ab910a3316))
* **ui:** v0.2.0 screen review + folder picker, worktree naming, effort slider ([#93](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/93)) ([5be3dd3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/5be3dd3db0cef0c486bb57f69706fc0eb3e5c7f7))
* **worktrees:** honour git exit codes, guard re-entrancy, move scans off the render thread ([#62](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/62)) ([7483dbb](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/7483dbba88669bff750d27efda449d5515e1c67e))


### Performance

* **stream:** stop the per-token render storm and O(n^2) streaming text ([#58](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/58)) ([268622f](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/268622f60510af3e63b04cb46ac4f5adb9d21103))

## 0.1.0 (2026-10-09)


### Features

* composer and inspector (WP2) ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* extensions and appearance (WP5) ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* Graphite console (Blazor) and Tauri desktop app ([#11](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/11)) ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* graphite console foundations (WP0) ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* **i18n:** English UI alongside French ([#34](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/34)) ([b4fdcc0](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/b4fdcc004d5bfe54ef8c5c149e61221c3e2841b9))
* integrate work packages WP1 to WP5 ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* navigation, start screen, overview, history (WP3) ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* **probe:** verify the unverified CLI controls with --probe-cli ([#7](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/7)) ([#35](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/35)) ([9d5c471](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/9d5c471dd7a715aac2cce389ac40ccda00bac08b))
* server desktop mode (loopback, --parent-pid) and a help screen when claude is missing ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* session thread (WP1) ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* show API errors as a friendly card in the thread ([#31](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/31)) ([af34894](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/af34894f68674da82bccb385d073dddd88338f53))
* **site:** documentation site and GitHub Pages deployment ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* Tauri v2 desktop app (standalone server, dark splash, duck icon) ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* **ui:** styled reconnect modal and error pages ([#46](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/46)) ([2c2c149](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/2c2c1495d1ca8b8c5ee2bea32fd7ba87cc632242))
* worktrees, classification and safe cleanup (WP4) ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))


### Bug Fixes

* **agents:** keep background subagents running until their task-notification ([#26](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/26)) ([a56385d](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/a56385ddf3bff0c1d8a98f941a5ea94c15f82f19))
* **composer:** keyboard navigation, aria-activedescendant and outside-click close for the model menu ([#20](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/20)) ([2031701](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/203170175120a784408c68375a8f8fdef4ce8ee0))
* **cost:** read past-session cost as max(cost-state, UI ledger) ([#29](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/29)) ([4d37b74](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/4d37b747bbca6b6e6266ceba14b09b8e44ef8c11))
* protect the desktop server with a per-launch token and loopback host filter ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* **release:** set-apple-secrets.sh reads a password piped from a file without a newline ([#47](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/47)) ([ee4692f](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/ee4692f4707ff341f143d22b7607fa0cb9e48f1a))
* repo name and root from worktree paths with either separator ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* review fixes (cost after --resume, queued turns, safe Markdown, keyboard) ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* **session:** trace claude boot, wait once on initialize, add --boot-probe ([#30](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/30)) ([c51b33e](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/c51b33e87aabc5a32a7e0b85115e9ebda5e9f4ff))
* **site:** app pane ground, honest no-release card, docs screen column ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* **worktrees:** keep repos discovered through -w sessions across worktree removal and restarts ([#28](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/28)) ([150fc53](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/150fc5306e4e10cd076ee7a14713f86d0fbc8365))
* **worktrees:** kill the claude process tree on exit and surface orphan worktree-* branches ([#32](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/32)) ([b37c082](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/b37c08294e93d148cd86aa1c1596bd3635562a5d))
* **worktrees:** refresh the rail's cleanable tag on worktree scans ([#16](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/16)) ([8f94914](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/8f94914f9d4af172169cb929af31845010c4d1e7))


### Documentation

* add English README and English site pages ([#33](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/33)) ([529b70c](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/529b70c80901d65770b9ddf17dc7f1942ff35bf3))
* **design:** record Console Graphite system in DESIGN.md and sidecar ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* graphite console implementation plan ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* **impeccable:** record the concept-seed roll behind the site direction ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* README (installation, prerequisites, development, publishing) and MIT license ([18584b3](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/18584b3085f5147278f06444a7a02644f563b33e))
* signed macOS installers, Apple Silicon only (drop Intel builds) ([#43](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/43)) ([a9fbd7a](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/a9fbd7ae9207ad18a36f38a36928bb78d83a4db2))
* **site:** English screenshots and a bilingual 404 ([#42](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/42)) ([794bbea](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/794bbeaabc4a08e43fecdc260efffc4c90d6e359))
* **site:** read the known-limitations list live from open GitHub issues ([#37](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/37)) ([bc088c4](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/bc088c43a28afd25149bb9c21222154197c8a580))
* translate developer docs, comments and self-check messages to English ([#40](https://github.com/Atypical-Consulting/ClaudeCodeUI/issues/40)) ([d9f6599](https://github.com/Atypical-Consulting/ClaudeCodeUI/commit/d9f65998fda3d7fa8268a3fb9479e8a6574d5be3))
