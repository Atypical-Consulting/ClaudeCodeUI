# Changelog

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
