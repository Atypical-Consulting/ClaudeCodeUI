# Mockups · Graphite console

Mockups of the Claude Code UI web interface. Open [`index.html`](index.html) in a browser: 11 screens, 5 dark themes, theme picker at the top of the page. Names, costs and durations are fictional.

| # | Screen | |
|--:|---|---|
| 1 | Starting a session (folder, worktree, permission mode) | ![](screens/01.png) |
| 2 | Running session (streaming, tool ledger, inspector) | ![](screens/02.png) |
| 3 | Permission request (full diff) | ![](screens/03.png) |
| 4 | Multi-session overview and decision queue | ![](screens/04.png) |
| 5 | `Ctrl K` picker and stopped process | ![](screens/05.png) |
| 6 | Markdown rendering (Markdig + highlight.js output) | ![](screens/06.png) |
| 7 | Appearance and themes | ![](screens/07.png) |
| 8 | Worktrees: risk-free cleanup | ![](screens/08.png) |
| 9 | Composer: model, effort, ultracode, `/` commands | ![](screens/09.png) |
| 10 | Ultracode and sub-agents | ![](screens/10.png) |
| 11 | Extensions: MCP, skills, agents, plugins, hooks | ![](screens/11.png) |

## Regenerate

The sources are in `.impeccable/mockups/` (`graphite.src.html` + `build.js`). The duck SVGs (BlazorKawaii 2.2.0) and the Markdig HTML are pre-rendered in `inputs/`.

```bash
bash .impeccable/mockups/shoot.sh   # rebuilds index.html and captures the screens into .impeccable/review/
```

Design direction and contract: [`PRODUCT.md`](../../PRODUCT.md), [`.impeccable/surfaces/`](../../.impeccable/surfaces/).
