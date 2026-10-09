---
version: 1
slug: "site-index-html"
primary_target: "site/index.html"
related_targets: ["site/docs.html"]
---

# Surface: site de documentation (GitHub Pages)

Scope: `site/index.html` (accueil + téléchargement, mode Persuade) et `site/docs.html` (documentation, mode Read). Public : développeurs qui utilisent déjà Claude Code en terminal ; ils arrivent depuis le README ou un lien partagé. Action : télécharger l'installeur de leur OS (dernière release GitHub). Preuve : les vraies captures/maquettes de la console, les vrais noms d'outils, les vrais raccourcis. Contraintes : sombre uniquement, français, aucune affirmation inventée (pas de chiffres d'usage, pas de signature de code : avertissement SmartScreen/Gatekeeper dit tel quel), le CLI `claude` reste un prérequis.

## Direction contract

THESIS: La page se lit comme le journal d'une session Claude Code : chaque fonctionnalité est une ligne d'outil du `.ledger` qui se déplie sur son écran. Refuse le héros « titre + sous-titre + bouton + rangée de cartes ».

OWN-WORLD: L'univers graphite de l'app tel quel : fond des volets #0E0F11 (`--ground`, inspecteur en `--sunk` #0B0C0E ; #16181B est `--raise`, réservé aux blocs surélevés) et coutures, Geist / Geist Mono, couleurs d'outils (Read = syn-fn bleu, Grep = syn-kw, Edit = vert ok, Bash = syn-type ambre, Write = syn-attr), accent #E8875B, carte de permission avec Autoriser / Toute la session / Refuser, canard RubberDuck dont l'humeur suit la page. Les 5 thèmes sombres restent commutables.

STORY: Le visiteur voit un prompt « installe Claude Code UI », comprend en une ligne que c'est le vrai CLI rendu lisible, autorise le téléchargement, puis parcourt le journal pour voir permissions, worktrees, sous-agents, extensions ; la doc répond au « comment ».

FIRST VIEWPORT: Rail étroit à gauche (marque, liens Docs / GitHub / Releases, canard). Colonne centrale : bloc `.you` avec le prompt, puis une ligne de statut et un ledger court (Read README, Grep prérequis, Bash vérification `claude --version`). À droite, colonne inspecteur : carte de permission « Télécharger Claude Code UI pour Windows ? » (OS détecté) listant fichier, taille, version ; Autoriser = téléchargement direct, « Autres systèmes » = liste des installeurs, Refuser = lien vers la doc. Action primaire dans le premier écran.

FORM: Journal d'outils (ledger), position 3 de ma liste ordonnée, dealt lead ; seed key e3d1a8aa (sortie de `impeccable concept-seed --scope surface --mode persuade` du 2026-10-09, choix enregistré par `--kind assigned --from e3d1a8aa`).

Signature interaction : chaque ligne du ledger se déplie (clavier ⏎ / clic) sur la capture et l'explication ; le texte du premier bloc assistant arrive en flux avec le caret, une seule fois, respecte prefers-reduced-motion.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance

## Adaptations

- Téléphone (< 860 px) : la carte de permission passe sous l'introduction. Un téléphone ne peut pas installer une application de bureau Windows/macOS/Linux, donc l'action primaire n'a pas à tenir dans le premier écran ; la ligne de statut, placée entre le texte et le ledger comme le veut FIRST VIEWPORT, porte le lien « Voir la demande » vers la carte dès que celle-ci n'est plus à côté (< 1240 px).
- Aucune version publiée : le troisième verbe de la carte devient « Compiler depuis les sources » (rien à refuser) ; Refuser revient dès qu'un installeur existe.
- Documentation (mode Read) : la colonne inspecteur affiche l'écran de l'app correspondant à la section lue, et un accès au téléchargement ; la prose est plafonnée à 62ch. Sous 1240 px, la colonne disparaît.
