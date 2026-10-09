**English** · [Français](README.fr.md)

# Claude Code UI

A desktop console for [Claude Code](https://docs.claude.com/en/docs/claude-code/overview). The app drives the real `claude` CLI (stream-json protocol, the same options as the SDK and the VS Code extension) and shows sessions more readably than a terminal: live text, a tool log, permissions with diffs, several sessions in parallel, git worktrees. The interface is available in French and English; dark themes only.

![Session in progress](docs/mockups/screens/02.png)

## Installation

Download the installer for your system from the [GitHub releases](https://github.com/Atypical-Consulting/ClaudeCodeUI/releases/latest) page. No .NET SDK or particular browser is needed: the app bundles its own server and uses the system web view.

| System | File | Note |
|---|---|---|
| Windows 10/11 | `.msi` or `-setup.exe` | Unsigned installer: SmartScreen shows "Windows protected your PC". Click "More info", then "Run anyway". WebView2 is already included in Windows 11. |
| macOS (Apple Silicon) | `.dmg` (`aarch64`) | Signed with a Developer ID and notarized by Apple: it opens normally, no workaround needed. Intel Macs are not supported (macOS 26 is their last version). |
| Linux x64 | `.AppImage` or `.deb` | Requires WebKitGTK 4.1 (`libwebkit2gtk-4.1-0` on Debian/Ubuntu, already installed on most desktops). For the AppImage: `chmod +x`, then run it. |

**Verify your download.** Each release ships a `SHA256SUMS` file (`shasum -a 256 -c SHA256SUMS`) and GitHub build-provenance attestations: `gh attestation verify <file> -R Atypical-Consulting/ClaudeCodeUI`.

### Requirement: Claude Code

The Claude Code CLI must be installed and signed in:

```sh
# macOS, Linux, WSL
curl -fsSL https://claude.ai/install.sh | bash
# Windows (PowerShell)
irm https://claude.ai/install.ps1 | iex
```

Then run `claude` once in a terminal to sign in. If `claude` is not found in the PATH, the app shows a help screen instead of the start screen. On macOS, the app reads the PATH of your login shell, so an installation done through `~/.zshrc` is picked up.

## Features

- New session: folder, isolated worktree, permission mode, first message.
- Live text, thinking indicator, tool log with an inspector (output, input, JSON).
- Permission requests with diffs: allow, deny, or allow for the whole session.
- Overview of all sessions and a queue of pending decisions (Ctrl ⇧ A).
- Command palette (Ctrl K), resume recent sessions with their history.
- Model, effort, subagents, context, MCP servers, skills and plugins.
- 5 h / 7 day quota, cost per turn.
- Worktrees: ranking and safe cleanup.
- Five dark themes and an adjustable code size.

## Development

Requirements: .NET 10 SDK. For the desktop app: Rust (stable), Node 20+ and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your system.

```sh
dotnet run                     # web server alone on http://localhost:5284
dotnet run -- --self-check     # built-in checks, non-zero exit code on failure
```

Desktop app (`desktop/` folder):

```sh
cd desktop
npm install
npm run server                 # publishes the self-contained server for your machine into src-tauri/server
npm run dev                    # launches the Tauri window (same as cargo tauri dev)
npm run build                  # builds the installers into src-tauri/target/release/bundle
```

The Tauri shell launches `ClaudeCodeUI --desktop-port 0 --parent-pid <pid>` (HTTP on 127.0.0.1 only, Production environment, access protected by a local-use token passed over standard output), shows a loading screen, then opens the interface. The server stops on its own when the window closes, and takes the `claude` processes down with it. Re-run `npm run server` after every change to the .NET code.

## Releasing

1. Commits follow [Conventional Commits](https://www.conventionalcommits.org/en/) (`feat:`, `fix:`, `docs:`, `ci:`, `chore:`).
2. On every push to `main`, [release-please](https://github.com/googleapis/release-please) opens or updates a release PR: CHANGELOG, and the version in `ClaudeCodeUI.csproj`, `tauri.conf.json` and `Cargo.toml`.
3. Merging that PR creates the tag and the GitHub release. The `release.yml` workflow then builds the Windows, macOS (Apple Silicon) and Linux installers and attaches them to the release.

## License

[MIT](LICENSE) © 2026 Atypical Consulting
