# Mockups · Console graphite

Maquettes de l'interface web de Claude Code UI. Ouvrir [`index.html`](index.html) dans un navigateur : 11 écrans, 5 thèmes sombres, sélecteur de thème en haut de page. Les noms, coûts et durées sont fictifs.

| # | Écran | |
|--:|---|---|
| 1 | Démarrage d'une session (dossier, worktree, mode de permission) | ![](screens/01.png) |
| 2 | Session en cours (streaming, journal d'outils, inspecteur) | ![](screens/02.png) |
| 3 | Demande de permission (diff complet) | ![](screens/03.png) |
| 4 | Vue d'ensemble multi-sessions et file des décisions | ![](screens/04.png) |
| 5 | Sélecteur `Ctrl K` et processus arrêté | ![](screens/05.png) |
| 6 | Rendu markdown (sortie Markdig + highlight.js) | ![](screens/06.png) |
| 7 | Apparence et thèmes | ![](screens/07.png) |
| 8 | Worktrees : ménage sans risque | ![](screens/08.png) |
| 9 | Composer : modèle, effort, ultracode, commandes `/` | ![](screens/09.png) |
| 10 | Ultracode et sous-agents | ![](screens/10.png) |
| 11 | Extensions : MCP, skills, agents, plugins, hooks | ![](screens/11.png) |

## Régénérer

Les sources sont dans `.impeccable/mockups/` (`graphite.src.html` + `build.js`). Les SVG du canard (BlazorKawaii 2.2.0) et le HTML Markdig sont pré-rendus dans `inputs/`.

```bash
bash .impeccable/mockups/shoot.sh   # reconstruit index.html et capture les écrans dans .impeccable/review/
```

Direction et contrat de design : [`PRODUCT.md`](../../PRODUCT.md), [`.impeccable/surfaces/`](../../.impeccable/surfaces/).
