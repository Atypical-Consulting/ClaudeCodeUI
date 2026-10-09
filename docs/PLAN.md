# Plan d'implémentation : console graphite

Branche `feat/graphite-console`. Cible : remplacer la page unique `Components/Pages/Home.razor` par l'application des 11 écrans de `.impeccable/mockups/graphite.html` (captures dans `docs/mockups/screens/01..11.png`), sur le vrai CLI `claude` 2.1.295 en stream-json.

Sources de vérité, par ordre de priorité :
1. Le rapport protocole (captures `s1..s7.jsonl` dans le scratchpad). Seuls les points **VÉRIFIÉS** deviennent des fonctionnalités.
2. `PRODUCT.md` : pas de fonctionnalité inventée, thèmes sombres uniquement, interface en français.
3. `.impeccable/mockups/graphite.src.html` : la seule source CSS (lignes 10–526) et le balisage de référence.

Quand les rapports se contredisent, ce plan tranche en faveur du protocole vérifié. Exemples : le flux partiel est **dans** le périmètre, et `set_model` / `apply_flag_settings` sont vérifiés.

---

## 1. Périmètre

### 1.1 Construit pour de vrai

| Écran | Fonction | Mécanisme vérifié |
|---|---|---|
| s1 | Nouvelle session : dossier, worktree, mode, premier prompt | flags `--permission-mode` (toujours explicite ; `default` est envoyé comme `manual`), `-w <nom>`, `--name`, `--session-id` |
| s2 | Texte en direct + curseur `.caret` | `--include-partial-messages` (`stream_event` / `text_delta`) |
| s2 | Journal d'outils, inspecteur Sortie/Entrée/JSON | `assistant` `tool_use` + `user` `tool_result` (+ `tool_use_result` pour le diff) |
| s2 | Indicateur de réflexion | `system/thinking_tokens` (pas le texte, qui est toujours vide) |
| s3 | Permission avec diff, Autoriser / Refuser | `control_request can_use_tool` → `control_response allow/deny` |
| s3 | « Toute la session » quand la suggestion est `setMode` | `set_permission_mode` puis `allow` |
| s4 | Vue d'ensemble, file des décisions | état en mémoire de toutes les sessions |
| s5 | Palette Ctrl K, carte de crash, relance | sortie du processus + `--resume <id>` |
| s6 | Markdown Markdig + blocs de code + hljs | existant, emballage `.cb` fait côté serveur |
| s7 | 5 thèmes + taille du code, mémorisés | `localStorage` + JS interop |
| s8 | Worktrees : classement et nettoyage sûr | `git` (commandes listées dans le rapport worktrees) |
| s9 | Choix du modèle | `initialize.models` + `set_model` |
| s9 | Effort | `--effort` au lancement, `apply_flag_settings {effortLevel}` en cours de session, lecture via `get_settings.applied.effort` |
| s9 | Mode rapide | `apply_flag_settings {fastMode}`. Affiché selon `fast_mode_state` / `fast_mode_disabled_reason` : souvent désactivé, avec la raison en infobulle |
| s9 | Ultracode | `apply_flag_settings {ultracode:true}`, disponibilité via `get_settings.applied.ultracodeAvailable` (seul le réglage est vérifié, pas son effet sur le tour : voir §1.2) |
| s9 | Popover `/` avec descriptions | `initialize.commands[{name,description,argumentHint}]` ; `/xxx` envoyé comme texte utilisateur |
| s9 | Panneau contexte | `get_context_usage` |
| s10 | Sous-agents : ligne par agent, sous-outils, texte de l'agent | `tool_use name:"Agent"`, `system/task_*`, `parent_tool_use_id`, `--forward-subagent-text` |
| s11 | Liste MCP (état, outils, erreur, transport) | `mcp_status` |
| s11 | Skills / Agents / Plugins | `initialize` (`agents`, `commands`) + init (`skills`, `plugins`) |
| rail | Quota 5 h / 7 j | `rate_limit_event.rate_limit_info.unifiedWindows`, plus `get_usage` au démarrage |
| rail | « Récentes » et reprise avec historique | lecture des `~/.claude/projects/<slug>/*.jsonl` + `--resume` (le CLI **ne rejoue pas** l'historique) |

### 1.2 Construit, mais à valider une fois en vrai (repli : contrôle désactivé)

Ces requêtes ont été acceptées mais jamais testées sur le cas réel. Le work package qui en est propriétaire les teste une fois. S'il constate un échec, il désactive le contrôle (`aria-disabled`, infobulle « bientôt ») au lieu de l'inventer.

- `mcp_toggle {serverName,enabled}` et `mcp_reconnect {serverName}` (bascule et bouton « Réessayer » de s11).
- « Toute la session » pour les suggestions autres que `setMode` (par exemple `addDirectories`). Il faut renvoyer `updatedPermissions`, ce qui n'a pas été testé.
- « Compacter maintenant » : envoi de `/compact` en texte utilisateur. Le mécanisme est vérifié pour `/cost` et `/context`, pas pour `/compact`.
- Ultracode : que `{ultracode:true}` déclenche bien un workflow multi-agents, et que `{ultracode:false}` après le `result` soit accepté. Seule la valeur `get_settings.applied.ultracode` est vérifiée.
- Relecture d'un transcript : la forme des lignes `user`/`assistant` du `.jsonl` est supposée identique au flux, mais le champ du diff (`tool_use_result` dans le flux) n'a pas été vérifié dans le fichier. Repli : `EditDiff` recalcule depuis l'entrée de l'outil.

### 1.3 Affiché désactivé (« bientôt ») ou omis, parce que le CLI ne le fournit pas

| Élément de la maquette | Traitement | Raison |
|---|---|---|
| Texte de réflexion | omis (seul l'indicateur reste) | `thinking_delta` est toujours vide |
| Phases du workflow, nom `review-changes`, cartes « trouvaille », coût par agent | omis ; coût affiché `—` | aucune donnée CLI (seuls des tokens et une durée par agent existent) |
| « Auto à 80 % » | libellé en lecture seule `Auto à {autoCompactThreshold}` si `isAutoCompactEnabled`, sinon rien | aucune requête de réglage vérifiée |
| « Se connecter » (MCP en `needs-auth`), « Ouvrir la configuration », onglet Hooks | bouton désactivé « bientôt » ; onglet Hooks masqué | pas de flux d'authentification ; forme de `get_hooks_listing` inconnue |
| « Parcourir », « Joindre » | omis | un navigateur ne peut pas parcourir les dossiers du serveur ; pièces jointes hors périmètre |
| « PR #212 mergée », toggles « Automatiquement » (s8) | omis | il faudrait `gh` ; spéculatif |
| `Ctrl ⏎ ouvrir à côté` (palette) | omis | pas de vue scindée |
| Retour arrière sur les fichiers | omis | `rewind_files` : « not enabled » |
| Titre IA de session | omis ; on passe `--name` nous-mêmes | pas d'`ai-title` en `-p` ; `generate_session_title` n'enregistre rien |
| Modes `bypassPermissions` / `dontAsk` | non proposés (s1 propose `default`, `acceptEdits`, `plan`, `auto`) | non montrés dans la maquette ; risque de sécurité |

---

## 2. Architecture

### 2.1 Fichiers (état cible)

```
Program.cs                     DI + --self-check
ClaudeSession.cs               processus + stdin/stdout (modifié)
Core/Events.cs                 records typés + Parse(JsonElement)
Core/LiveSession.cs            état d'une session + réducteur Apply(ClaudeEvent)
Core/SessionManager.cs         singleton : sessions vivantes, file des décisions
Core/TranscriptStore.cs        static : sessions passées + relecture d'un .jsonl
Core/WorktreeService.cs        singleton : scan / classement / plan / exécution git
Core/UiState.cs                scoped : inspecteur, palette, sélection
Core/Fmt.cs                    formats fr-FR (durées, tailles, %), coûts invariants
Core/ToolKinds.cs              couleur + cible d'un outil
Core/EditDiff.cs               lignes de diff pour Edit/Write/MultiEdit
Core/Md.cs                     Markdig + emballage .cb + titres d'alertes FR
Core/SelfCheck.cs              vérifications exécutables (§6)
Components/...                 voir §4
wwwroot/app.css, app.js, fonts/
```

Pas d'interface (une seule implémentation par service) et aucun réglage côté serveur : le thème et la taille du code vivent dans `localStorage`, les dossiers récents viennent de `TranscriptStore`.

### 2.2 `Program.cs`

```csharp
if (args is ["--self-check"]) { SelfCheck.Run(); return; }   // exit code != 0 en cas d'échec
builder.Services.AddRazorComponents().AddInteractiveServerComponents();
builder.Services.AddSingleton<SessionManager>();   // IAsyncDisposable : tue les processus à l'arrêt
builder.Services.AddSingleton<WorktreeService>();
builder.Services.AddScoped<UiState>();
```

### 2.3 `ClaudeSession` (modifications)

- Nouvelle signature : `ClaudeSession(string cwd, IReadOnlyList<string> args, Func<JsonElement,Task> onEvent, Action<int,string> onExit)`. Les flags de base restent dans la classe. `args` est construit par `LiveSession` :
  - toujours : `--permission-mode <mode>` (`default` est envoyé comme `manual`), `--include-partial-messages`, `--forward-subagent-text`
  - nouvelle session : `--session-id <uuid>` et `--name <nom>`
  - reprise : `--resume <id>` (sans `--session-id` ; l'id reste le même)
  - selon le formulaire s1 : `-w <nom>`, `--model <m>`, `--effort <e>`
- `Task<JsonElement> Request(string subtype, JsonObject? fields = null)` : écrit un `control_request` avec un `request_id` unique et attend la réponse via un `ConcurrentDictionary<string, TaskCompletionSource<JsonElement>>`. Le lecteur intercepte les `control_response` (`success` → résultat de `response.response` ; `error` → `ClaudeRequestException(error)`), puis transmet quand même l'événement. Délai d'expiration : 15 s.
- `Interrupt()` devient `Request("interrupt")`.
- `Respond(requestId, allow, input, JsonNode? updatedPermissions = null)`. Message de refus : `"Refusé par l’utilisateur"`.
- `onExit(exitCode, "claude exited {code} {stderr}")`. Ne rien appeler si le processus a été disposé volontairement : un booléen `disposing` suffit.

### 2.4 Modèle d'événements (`Core/Events.cs`)

Un seul point de parsing, `static ClaudeEvent? Parse(JsonElement e)`. Il renvoie `null` pour ce qu'on ignore : `hook_*`, `system/notification`, `message_start/stop` et types inconnus.

```csharp
abstract record ClaudeEvent;
record InitEvt(string SessionId, string Cwd, string Model, string PermissionMode, string[] Tools,
               McpBrief[] Mcp, string[] SlashCommands, string[] Skills, string[] Agents, string[] Plugins,
               string FastModeState, string? FastModeReason) : ClaudeEvent;
record McpBrief(string Name, string Status);                       // connected|failed|needs-auth|pending
record StatusEvt(string? Status, string? PermissionMode) : ClaudeEvent;   // "requesting", changement de mode
record ThinkingEvt(int EstimatedTokens) : ClaudeEvent;
record TextDeltaEvt(string Text, string? ParentToolUseId) : ClaudeEvent;  // stream_event content_block_delta text_delta
record AssistantTextEvt(string MessageId, string Text, string? ParentToolUseId, bool Synthetic) : ClaudeEvent;
record ToolUseEvt(string Id, string Name, JsonElement Input, string? ParentToolUseId) : ClaudeEvent;
record ToolResultEvt(string ToolUseId, string Text, bool IsError, JsonElement? Structured) : ClaudeEvent; // Structured = tool_use_result
record UserTextEvt(string Text) : ClaudeEvent;                     // relecture du transcript / "[Request interrupted by user]"
record PermissionEvt(string RequestId, string Tool, JsonElement Input, string? Description,
                     string? ToolUseId, JsonElement? Suggestions) : ClaudeEvent;
record ResultEvt(string Subtype, bool IsError, decimal TotalCostUsd, int DurationMs, int NumTurns,
                 long ContextTokens, long? ContextWindow, string? FastModeState, string? FastModeReason) : ClaudeEvent;
record RateLimitEvt(double FiveHour, DateTimeOffset FiveHourReset, double SevenDay, DateTimeOffset SevenDayReset) : ClaudeEvent; // 0..1
record TaskStartedEvt(string TaskId, string ToolUseId, string Description, string SubagentType) : ClaudeEvent;
record TaskProgressEvt(string TaskId, long TotalTokens, int ToolUses, int DurationMs) : ClaudeEvent;
record TaskDoneEvt(string TaskId, string ToolUseId, string Status) : ClaudeEvent;   // task_notification
record TitleEvt(string Title) : ClaudeEvent;                       // session_title_changed
```

Règles de parsing :
- Le coût `total_cost_usd` est **cumulé** sur la session et à travers `--resume`. On l'affecte, on ne l'additionne jamais. Le coût d'un tour est la différence avec la valeur précédente.
- `ContextTokens` = `usage.input_tokens + cache_read_input_tokens + cache_creation_input_tokens`. `ContextWindow` = `modelUsage[*].contextWindow`.
- `assistant` arrive **un bloc par message** (même `message.id`). Chaque bloc `text` donne un `AssistantTextEvt` et chaque `tool_use` un `ToolUseEvt`. `model:"<synthetic>"` donne `Synthetic=true` (sortie des commandes `/`).
- Les messages `user` et `assistant` d'un transcript `.jsonl` ont la même forme. `TranscriptStore` les passe donc au même `Parse` (champ du diff non vérifié dans le fichier, §1.2).
- `control_cancel_request` n'a jamais été observé : pas d'événement dédié. Un `PendingPermission` est retiré quand on y répond, ou au `ResultEvt` / à la sortie du processus.
- Outils (via `ToolKinds`) : le shell est **`PowerShell`** sur ce poste (`Bash` n'est pas dans `tools`), les deux ont la couleur `--bash` et la vue `pre.cmd`. Le sous-agent s'appelle **`Agent`** dans les messages (et `Task` dans `tools`) : les deux noms donnent une `.wf`.
- `system/status`, `system/task_updated`, `message_delta` et `stream_event` autres que `text_delta` sont ignorés sauf mention ci-dessus.

### 2.5 État par session (`Core/LiveSession.cs`)

```csharp
enum SessionStatus { Starting, Idle, Running, Waiting, Exited, Crashed }
sealed class LiveSession {
  string Id;                 // == --session-id == nom du .jsonl == route /session/{Id}
  string Name, Cwd, Mode; string? Branch, Repo, Worktree;      // Cwd remplacé par init.cwd (cas -w)
  SessionStatus Status;
  ImmutableList<Item> Items;                 // copie à l'écriture : rendu sans verrou
  ImmutableList<PendingPermission> Pending;
  string StreamingText;                      // texte partiel du bloc en cours (parent null)
  int ThinkingTokens;
  decimal CostUsd; decimal LastTurnCostUsd; int ToolCount;
  DateTimeOffset StartedAt, LastEventAt; DateTimeOffset? TurnStartedAt; TimeSpan? LastTurn; string? LastResultSubtype;
  int? ExitCode; string? ExitText;
  InitEvt? Init; JsonElement? InitializeInfo;          // réponse "initialize" (models, commands, agents, account)
  string? Model, Effort; bool Ultracode; string FastModeState; string? FastModeReason;
  JsonElement? Context; JsonElement? McpStatus;       // réponses brutes de get_context_usage / mcp_status, lues par WP2 / WP5
  DateTimeOffset? LastResultAt;                         // humeur « Tour terminé » du canard
  event Action? Changed;
  Task Send(string text); Task Answer(PendingPermission p, Decision d); Task Interrupt(); Task Restart();
  Task SetModel(string m); Task SetEffort(string e); Task SetFast(bool on); Task SetUltracode(bool on);
  Task RefreshContext(); Task RefreshMcp(); Task<JsonElement> Request(string subtype, JsonObject? f = null);
  internal void Apply(ClaudeEvent e);        // réducteur pur sur l'état : testé par SelfCheck
}
abstract record Item;
record UserItem(string Text, DateTimeOffset At, bool Ultracode) : Item;
record TextItem(string Markdown, string? ParentToolUseId) : Item;
sealed record ToolItem(string Id, string Name, JsonElement Input, string? ParentToolUseId) : Item {
  public ToolState State; public string? ResultText; public JsonElement? Structured;
  public DateTimeOffset StartedAt; public DateTimeOffset? EndedAt;
  public string? TaskId; public long Tokens; public int SubToolUses;     // Agent uniquement
}
enum ToolState { Running, Done, Error, Waiting, Denied }
record PendingPermission(string RequestId, string Tool, JsonElement Input, string? Description, string? ToolUseId, JsonElement? Suggestions);
enum Decision { Allow, AllowSession, Deny }
```

Transitions :
- `Send` → `Running`, `TurnStartedAt = now`.
- `PermissionEvt` → `Waiting` ; la `ToolItem` correspondante passe à `ToolState.Waiting`.
- Un refus marque la `ToolItem` `Denied`.
- `ResultEvt` → `Idle`, ou `Idle` + « interrompu » si `terminal_reason == aborted_streaming`.
- Sortie inattendue du processus → `Crashed`. `Dispose` volontaire → `Exited`.

Concurrence et rendu :
- Le réducteur tourne sur le thread lecteur du processus (un seul écrivain par session). Les listes sont des `ImmutableList` remplacées à chaque changement, et les composants les lisent sans verrou.
- `Changed` est **limité à 1 notification / 50 ms** pendant le flux de `TextDeltaEvt`. Notification immédiate pour tout le reste.

Démarrage :
- Une session neuve lance le processus tout de suite, puis envoie `initialize`, `get_settings` et `get_usage`. Cela remplit le modèle, les commandes, l'effort et le quota avant le premier message ; `system/init` n'arrive qu'après ce premier message.
- `initialize` est attendu **une seule fois** (même `request_id`, 180 s, ligne « still waiting » à 60 s) : renvoyer sous un nouvel id perdait la réponse tardive au premier. Pendant `Starting`, la chronologie du démarrage (spawn, écritures, stderr en direct, réponses tardives) part sur la console en `[<id>] boot …`. `dotnet run -- --boot-probe <dossier> [s]` rejoue un démarrage à froid hors de l'UI ; la cause du blocage de 60 s n'est pas reproduite par la sonde (issue #6).
- Une session passée (reprise) est créée avec `Items` chargés par `TranscriptStore.Load(id)`. Son processus `--resume` n'est lancé qu'au premier `Send` ou au clic sur « Reprendre ».

Ultracode « pour ce tour » :
- `Send` avec le toggle actif appelle `apply_flag_settings {ultracode:true}` avant d'envoyer le message.
- Au `ResultEvt`, on renvoie `{ultracode:false}` et on remet le toggle à zéro.
- `UserItem.Ultracode=true` affiche la puce `.ultra-chip`.

« Toute la session » :
- Suggestion `setMode` : `set_permission_mode {mode}` puis `allow`.
- Sinon : `allow` + `updatedPermissions = suggestions` (§1.2, à valider).

`Restart` crée un nouveau `ClaudeSession` avec `--resume Id` dans `Cwd` **tel que remplacé par `init.cwd`** (le dossier du worktree en cas de `-w` : c'est ce cwd qui donne le slug du `.jsonl`). Ne pas repasser `-w`. Si le processus est mort avant le premier message (pas de `.jsonl`), relancer en session neuve avec le même `--session-id`.

WP0 implémente **entièrement** `LiveSession` (réducteur + toutes les méthodes : ce sont des enveloppes de quelques lignes autour de `Request`). Les WP1–5 ne font que les appeler et les valider en vrai.

### 2.6 `SessionManager` (singleton)

```csharp
IReadOnlyList<LiveSession> All;                    // ordre de création, jamais retrié (s4)
LiveSession? Get(string id);
LiveSession Start(string cwd, string mode, string name, string? worktree, string? model, string? effort);
LiveSession Open(PastSession p);                   // historique chargé, processus paresseux
Task Stop(string id);
IEnumerable<(LiveSession S, PendingPermission P)> DecisionQueue;   // toutes sessions, ordre d'arrivée
RateLimitEvt? Limits;                              // dernier reçu, toutes sessions confondues
event Action? Changed;                             // relayé depuis chaque LiveSession.Changed
```

`Branch` vient de `git -C cwd branch --show-current`. `Repo` est le nom du dossier parent de `rev-parse --path-format=absolute --git-common-dir`. On les recalcule après `InitEvt`, car le `cwd` peut changer avec `-w`.

### 2.7 `TranscriptStore` (static)

```csharp
record PastSession(string Id, string Cwd, string? Branch, string Title, decimal? CostUsd, DateTimeOffset LastWrite, string? WorktreePath);
static IReadOnlyList<PastSession> Recent(int take = 30);   // *.jsonl de premier niveau, par mtime, 30 derniers jours
static IReadOnlyList<Item> Load(string id);                // cherche ~/.claude/projects/*/{id}.jsonl, relit user/assistant via Events.Parse + réducteur
static string Slug(string cwd) => Regex.Replace(cwd, "[^A-Za-z0-9]", "-");
```

- Lire au plus 64 Ko en tête et 64 Ko en queue par fichier.
- Titre : `custom-title` → `agent-name` → `ai-title` → `last-prompt` (tronqué) → premier texte `user`.
- Coût : max de `cost-state.totalCostUSD` et du registre de l'UI `%LOCALAPPDATA%\ClaudeCodeUI\costs\<id>.txt` (écrit à chaque `result` coûteux). `cost-state` n'est écrit que si le CLI sort proprement ; `PersistedCost` reste le `cost-state` brut que `--resume` restaure.
- Mettre le résultat de `Recent` en cache 30 s.

### 2.8 `WorktreeService` (singleton)

C'est l'API du rapport worktrees, en un seul fichier. La logique de classement est extraite en fonction **pure**, `static (WtState, string Why) Classify(WtFacts f)`, testée par SelfCheck. Les faits (`Dirty`, `Ahead`, `Merged`, `SquashMerged`, `UpstreamGone`, `Locked`, `LockPid`, `PidAlive`, `Exists`, `Prunable`, `ActiveSessionName`) sont collectés à part, par les commandes git du rapport.

Garde-fous non négociables :
- jamais `--force` ni `-D` ;
- revérification de chaque ligne juste avant exécution ;
- l'échec de `branch -d` n'est pas fatal ;
- confirmation après affichage des commandes exactes ;
- le worktree principal n'est jamais proposé.

Le cas « verrou à pid mort » est classé « À vérifier », avec `git worktree unlock` dans le plan. C'est le cas normal des worktrees créés par nos propres sessions `-w` une fois terminées.

Signatures figées par WP0 (corps WP4) : `DiscoverReposAsync`, `ScanAsync`, `SizeAsync`, `Plan`, `RunAsync`, `PushAsync` (rapport worktrees §5, `SessionManager` au lieu de `sessions`), plus `static Classify(WtFacts)` et **`int? CleanableCount`** (Sûrs + Orphelins du dernier scan, `null` avant le premier ; lu par le rail de WP3).

Actions par ligne (s8) : Supprimer / Élaguer → ajoutent la ligne au plan ; Aller → `/session/{id}` ; Pousser → `PushAsync` après confirmation ; Ouvrir → `/` avec le dossier pré-rempli.

### 2.9 `UiState` (scoped, par circuit)

`bool InspectorOpen = true; bool PaletteOpen; string? SelectedToolId; event Action? Changed; event Action<string>? Key;`

`MainLayout` (WP0) reçoit tous les raccourcis de `app.js` et les traite **lui-même** : Ctrl K / Ctrl I (bascule `UiState`), Ctrl N / Alt N (`/`), Ctrl ⇧ O (`/overview`), Ctrl ⇧ A (session de `DecisionQueue.First()`). Les autres (`Escape`, `Enter`, `Shift+Enter`, `Delete`) sont relayés tels quels par `UiState.Key` : `SessionPage` interrompt sur Échap, `PermissionCard` répond sur ⏎ / Maj ⏎ / Suppr, la palette se ferme sur Échap.

`SelectedToolId` désigne aussi une ligne `.agent` (id du `tool_use` `Agent`) : c'est ce qui fait afficher `AgentPanel`.

---

## 3. CSS, polices, thèmes, JS

- **`wwwroot/app.css`** : lignes 10–526 de `graphite.src.html` copiées telles quelles (elles incluent déjà `md_themes.css`, `parity.css` et le mapping hljs des lignes 336–349). On exclut les lignes 71–82 (`.doc .screen .frame .synthetic`) et 352–356 (`.themebar`). Retouches :
  - retirer `.focus-demo` du sélecteur de la ligne 281 ;
  - ligne 276 : `.btn[aria-disabled=true],.btn:disabled` ;
  - `.effort span`→`.effort>*`, `.seg span`→`.seg>*`, `.subtabs span`→`.subtabs>*`, y compris dans les listes de transition et de survol de « polish 2 ».

  On ajoute ensuite le bloc « app glue » du rapport maquettes §1.4, plus `.search{width:100%}` et `.app:not(:has(>.insp)){grid-template-columns:256px minmax(0,1fr)}`. De l'ancien `app.css`, on garde seulement `#blazor-error-ui` et `.blazor-error-boundary`. `app.css` appartient à WP0 seul : le CSS de la maquette est final, donc les WP1–5 n'y touchent pas. Un WP qui a besoin d'une règle la note dans son PR et l'intégrateur l'ajoute (des sections `/* WPn */` voisines feraient des conflits git, les hunks adjacents se chevauchant).
- **Supprimer** `Home.razor.css`, `MainLayout.razor.css` et le lien `vs2015.min.css`. Aucun CSS scopé par composant.
- **Polices** : Geist et Geist Mono auto-hébergées en `wwwroot/fonts/Geist[wght].woff2` et `GeistMono[wght].woff2` (OFL, depuis le paquet npm `geist` ou le dépôt vercel/geist-font), déclarées par `@font-face` en tête d'`app.css`. Si le téléchargement échoue, utiliser le `<link>` Google Fonts des lignes 7–8 de la maquette. Les piles de repli système sont déjà dans les tokens.
- **Thèmes** : `data-theme` sur `<html>` (graphite, encre, ristretto, mousse, contraste). Un script inline dans `<head>` (avant la peinture) lit `localStorage['claude-ui.theme']` et `['claude-ui.code-size']`. `<html lang="fr">`.
- **`wwwroot/app.js`** (remplace l'observer inline de `App.razor`) :
  - `window.claudeUi = { getTheme, setTheme, getCodeSize, setCodeSize, copy, registerShortcuts(dotnetRef), focus(el) }`.
  - MutationObserver avec débounce rAF :
    1. `pre>code:not(.hljs)` → `hljs.highlightElement` ;
    2. `[data-hl] code.lc:not([data-done])` → `hljs.highlight(text,{language,ignoreIllegals:true})`.
  - Clic délégué sur `.cb .copy` et `[data-copy]` (libellé « Copié » pendant 1400 ms).
  - Raccourcis globaux transmis à .NET : Ctrl K, Ctrl I, Échap, Ctrl ⇧ A, Ctrl ⇧ O, Ctrl N **et Alt N** (le navigateur capture Ctrl N), ainsi que ⏎ / Maj ⏎ / Suppr quand aucun champ n'a le focus et qu'une permission est affichée.
  - **Ne jamais** remplacer de nœud de premier niveau dans un `MarkupString`. L'emballage `.cb` est fait côté serveur par `Md.cs`.
- **Langage hljs** : `.razor` / `.cshtml` → `csharp` ; table des extensions dans `EditDiff.Lang(path)`.
- **Mascotte** : `<PackageReference Include="BlazorKawaii" Version="2.2.0" />`. Dans `_Imports.razor`, ajouter `@using BlazorKawaii.Common` et `@using RubberDuck = BlazorKawaii.Components.RubberDuck`. Pas de `@using BlazorKawaii.Components`, à cause du conflit avec `System.IO.File`. Toujours passer `Color="#FCCC0A"` : la couleur par défaut du composant n'est pas confirmée.

---

## 4. Arbre de composants

Rendu interactif global : `<Routes @rendermode="InteractiveServer" />` et `<HeadOutlet @rendermode="InteractiveServer" />` dans `App.razor`. Plus aucun `@rendermode` par page. Dans `Routes.razor`, `FocusOnNavigate Selector="h1"` devient `".ph .ttl"`.

| Fichier | Classes maquette | Écrans | WP |
|---|---|---|---|
| `Components/App.razor` | head, script de thème | tous | 0 |
| `Components/Layout/MainLayout.razor` | `.app` + `<Rail/>` + `@Body` + `<CommandPalette/>` + `<IconSprite/>` ; enregistre les raccourcis | tous | 0 |
| `Components/Layout/IconSprite.razor` | sprite SVG (src 530–553) | tous | 0 |
| `Components/Shared/Icon.razor` | `svg.i(.sm)` | tous | 0 |
| `Components/Layout/Rail.razor` | `.pane .brand .rail-actions .grp .list` | tous | 3 |
| `Components/Layout/RailSessionRow.razor` | `.s .dot .n .branch .pill .c` | tous | 3 |
| `Components/Layout/QuotaMeter.razor` | `.quota .qrow .bar` | tous | 3 |
| `Components/Layout/Mascot.razor` | `.mascot .duck` | tous | 3 |
| `Components/Layout/CommandPalette.razor` | `.scrim .palette .q-in .pg .pi .pfoot` | s5 | 3 |
| `Components/Pages/NewSession.razor` `/` | `.start .start-card .hello .field .input .recents .modes .mode .toggle .hint` | s1 | 3 |
| `Components/Pages/Overview.razor` `/overview` | `.ov .ovhead .tot table .stt .mtag .queue .q` | s4 | 3 |
| `Components/Pages/SessionPage.razor` `/session/{Id}` | `.pane` + `.pane.insp` | s2 s3 s5 s6 s9 s10 | 1 |
| `Components/Session/SessionHeader.razor` | `.ph .ttl .sub .end` | s2… | 1 |
| `Components/Session/Thread.razor` | `.thread`, `.you`, `.ultra-chip`, `.caret` | s2 s3 s5 s6 s10 | 1 |
| `Components/Session/Ledger.razor` | `.ledger .lh` | s2 s3 | 1 |
| `Components/Session/ToolRow.razor` | `.t(.sel/.wait/.void) .kind .p .m .badge` | s2 s3 | 1 |
| `Components/Session/Workflow.razor` | `.wf .wh .agent .subrun .sl` (sans `.phases`) | s10 | 1 |
| `Components/Session/StatusLine.razor` | `.status(.wait) .spin` | s2 s3 s10 | 1 |
| `Components/Session/CrashBanner.razor` | `.crash` (`h4`) | s5 | 1 |
| `Components/Shared/Markdown.razor` + `Core/Md.cs` | `.prose.md .cb` | s6 | 1 |
| `Components/Session/Composer.razor` | `.composer.rich .box .tools-row .chip .pick .effort .go .meta` | s2 s3 s5 s9 s10 | 2 |
| `Components/Session/SlashPopover.razor` | `.pop.slash .sh .si` | s9 | 2 |
| `Components/Session/ModelMenu.razor` | `.pop` + `.si` (non maquetté, styles réutilisés) | s9 | 2 |
| `Components/Session/Inspector.razor` | `.pane.insp` (aiguillage) | s2 s3 s9 s10 | 2 |
| `Components/Session/ToolDetail.razor` | `.tabs .tab .cmdlog .errbox .kv` | s2 s6 | 2 |
| `Components/Session/PermissionCard.razor` | `.section .acts .cmd` + « Aussi en attente » | s3 | 2 |
| `Components/Session/ContextPanel.razor` | `.ctx .ctxbar .legend` | s9 | 2 |
| `Components/Session/AgentPanel.razor` | `.kv` + `.prose.md` (sans `.finding`) | s10 | 2 |
| `Components/Shared/DiffView.razor` + `Core/EditDiff.cs` | `.diff .fh .hunk .l .add .del .g .lc` | s3 s2 | 2 |
| `Components/Shared/FileView.razor` | `.filev .lc` | s2 | 2 |
| `Components/Pages/Worktrees.razor` | `.wt .wt-head .filters .wtt .grp-row .state .st-* .why .rowacts .plan .it .cmdlog .guard` | s8 | 4 |
| `Components/Pages/Extensions.razor` `/extensions` | `.ext .subtabs .filters .mcp .ms .errbox .toggle` | s11 | 5 |
| `Components/Pages/Appearance.razor` `/settings/appearance` | `.settings .set-row .themes .tc .pv .seg .sample` | s7 | 5 |

On supprime `Components/Pages/Home.razor` (WP1, une fois la session portée). `Error.razor` et `NotFound.razor` restent.

Le balisage, les textes français et les règles d'affichage de chaque composant suivent le **rapport maquettes §2–§3**, qui sert de spécification annexe. Ce plan en corrige les points suivants :
- le flux partiel et `.caret` sont dans le périmètre ;
- `ToolKinds` : `PowerShell` traité comme `Bash` (couleur, `pre.cmd`, verbe « Lancer cette commande ? », « une commande » dans `.status.wait`), `Agent` comme `Task` ;
- s1 : la branche affichée sous le toggle worktree est `worktree-<nom>` (ce que crée `-w`), pas `claude/<slug>` ;
- un diff **après coup** (inspecteur, onglet Diff) se construit depuis `tool_use_result.structuredPatch` (vérifié) ; `EditDiff` ne calcule que le diff **avant** autorisation (carte de permission) et le repli de relecture ;
- l'effort, le modèle, le mode rapide et ultracode passent par les requêtes vérifiées (§2.5), pas par le préfixe « ultracode » dans le prompt ;
- le mode rapide est désactivé seulement si `fast_mode_state != "on"` ou si le modèle n'a pas `supportsFastMode`. L'infobulle donne la raison en français : `sdk_opt_in_required` → « opt-in SDK requis », `extra_usage_disabled` → « usage supplémentaire désactivé », sinon le code brut ;
- l'effort n'affiche que `supportedEffortLevels` du modèle courant et disparaît si `!supportsEffort` ;
- Extensions utilise `mcp_status` pour l'erreur, le transport et les outils ;
- pour le quota, `rate_limit_event` donne `utilization` entre 0 et 1, alors que `get_usage` donne une valeur entre 0 et 100. Normaliser.

---

## 5. Lots de travail

### 5.0 Contrats partagés (créés par WP0, figés ensuite)

Tous les autres WP codent contre ces contrats. Une modification passe par l'intégrateur.

1. **C#** : tout le §2. On fige `Events.cs`, `LiveSession` (complète), `SessionManager`, `UiState`, `Fmt`, `ToolKinds`, les **signatures** de `TranscriptStore`, `WorktreeService`, `EditDiff` et `Md` (corps : implémentation minimale renvoyant du vide, **pas** `NotImplementedException`, pour que l'app tourne pendant que les WP avancent), et `SelfCheck.Run()`. Signatures des utilitaires :
   - `Fmt` : `Dur(TimeSpan)` → `2,4 s` / `2 min 14`, `Size(long)` → `1,34 Go`, `Pct(double)` → `32 %`, `Tokens(long)` → `212k`, `Cost(decimal, int decimals)` → `$0.0391`, `Time(DateTimeOffset)` → `14:31` ;
   - `ToolKinds` : `Color(name)`, `Target(JsonElement input, string cwd)`, `IsShell(name)`, `IsAgent(name)`, `IsEdit(name)` ;
   - `EditDiff` : `IReadOnlyList<DiffLine> FromInput(string tool, JsonElement input)`, `FromPatch(JsonElement structuredPatch)`, `Lang(string path)` ;
   - `Md` : `string Render(string markdown)`. Ce dernier appelle `Events.Check()`, `LiveSession.Check()`, `Md.Check()`, `EditDiff.Check()` et `WorktreeService.Check()` ; chaque `Check` vit dans le fichier de sa logique et reste vide tant que le WP propriétaire ne l'a pas écrit.
2. **Paramètres des composants transverses**, créés en *stubs* compilables par WP0 (balisage minimal) :
   - `Icon(Name, Sm, Class)`
   - `Markdown(Text)`
   - `DiffView(Path, IReadOnlyList<DiffLine> Lines)`, avec `record DiffLine(DiffKind Kind, int? N, string Text)`, `enum DiffKind{Ctx,Add,Del,Hunk}`
   - `FileView(Path, string ReadResultText)`
   - `Composer(LiveSession Session)`
   - `Inspector(LiveSession Session)`
   - `Thread(LiveSession Session)`
   - `SessionHeader(LiveSession Session)`
   - `CrashBanner(LiveSession Session)`
   - `Mascot()`, `Rail()`, `CommandPalette()`, toutes les pages avec leur `@page`.
3. **Mêmes conventions partout** :
   - abonnement à `Changed` dans `OnInitialized`, désabonnement dans `Dispose`, rafraîchissement par `InvokeAsync(StateHasChanged)` ;
   - formats uniquement via `Fmt` ;
   - couleurs d'outils via `ToolKinds.Color`.
4. **CSS** : les noms de classes sont ceux de la maquette (colonne « Classes » du §4). Aucun WP n'invente de classe si la maquette en a une, et aucun WP1–5 ne modifie `app.css` (§3).
5. **Propriété des fichiers** : WP0 crée tous les fichiers (stubs compris) ; ensuite chaque fichier a **un seul** propriétaire, celui de la liste « Fichiers » de son WP. Un stub de WP0 n'est plus touché par WP0.

### WP0 : fondations (séquentiel, avant tout le reste)

**Fichiers :**
- `ClaudeCodeUI.csproj`, `Program.cs`, `ClaudeSession.cs`
- `Core/Events.cs`, `Core/LiveSession.cs`, `Core/SessionManager.cs`, `Core/UiState.cs`, `Core/Fmt.cs`, `Core/ToolKinds.cs`, `Core/SelfCheck.cs`
- les stubs `Core/TranscriptStore.cs`, `Core/WorktreeService.cs`, `Core/EditDiff.cs`, `Core/Md.cs` (port de la pipeline actuelle)
- `wwwroot/app.css`, `wwwroot/app.js`, `wwwroot/fonts/*`
- `Components/App.razor`, `Components/Routes.razor`, `Components/_Imports.razor`
- `Layout/MainLayout.razor` (raccourcis complets, §2.9), `Layout/IconSprite.razor`, `Shared/Icon.razor`
- les stubs de tous les composants et pages du §4
- suppression de `MainLayout.razor.css`
- `.gitignore` : ajouter `.claude/worktrees/` (sinon les worktrees `-w` et ceux des WP deviennent des gitlinks au premier `git add .`)

**Critères d'acceptation :**
- `dotnet build` : 0 erreur, aucun nouvel avertissement.
- Ctrl K / Ctrl I / Alt N / Ctrl ⇧ O / Ctrl ⇧ A agissent (palette et inspecteur en stub suffisent).
- `dotnet run -- --self-check` passe. `Events.Check()` parse un exemple de chaque type de ligne JSON du rapport protocole (init, text_delta, tool_use, tool_result avec `tool_use_result`, can_use_tool, result succès + interrompu, rate_limit_event, task_started/notification) et vérifie les champs clés. `LiveSession.Check()` rejoue une séquence (send → tool_use → permission → deny → result) et vérifie le statut, la `ToolItem` `Denied`, le coût affecté (pas additionné) et `Pending` vide.
- `dotnet run` : la coquille `.app` s'affiche avec le thème graphite et les polices Geist. Changer `localStorage['claude-ui.theme']='mousse'` puis recharger recolore toute l'interface sans flash.
- `SessionManager.Start` appelé par le stub `NewSession` (dossier courant, `haiku`) reçoit la réponse `initialize` : `InitializeInfo` rempli, nombre de modèles et de commandes écrit dans le log. Pas de bouton de test à retirer.
- `Home.razor` existe encore mais a perdu son `@page` (route reprise par `NewSession`).

### WP1 : fil de session

**Fichiers :**
- `Pages/SessionPage.razor`
- `Session/SessionHeader.razor`, `Session/Thread.razor`, `Session/Ledger.razor`, `Session/ToolRow.razor`, `Session/Workflow.razor`, `Session/StatusLine.razor`, `Session/CrashBanner.razor`
- `Shared/Markdown.razor`, `Core/Md.cs` (+ `Md.Check`)
- suppression de `Pages/Home.razor` et `Home.razor.css`

**Critères d'acceptation :**
- Avec un vrai prompt en `--model haiku` :
  - le texte apparaît en direct avec `.caret`, puis est remplacé par le bloc complet ;
  - les outils consécutifs forment un `.ledger`, le dernier déplié et les anciens repliés ;
  - durées et compteurs au format `Fmt` (`2,4 s`).
- Cliquer une `.t` met `UiState.SelectedToolId` et `.t.sel`. Échap interrompt : statut « interrompu », pas de crash.
- Un sous-agent (« lance un agent qui répond pong ») produit une `.wf` avec une `.agent` (tokens issus de `task_progress`, coût `—`) et ses sous-outils dépliables en `.sl`.
- Tuer le processus `claude` à la main affiche `.crash` avec le canard Ko, le texte de sortie, « Copier l'erreur » (`data-copy`) et « Relancer la session ». La relance reprend la session (`--resume`) et le modèle se souvient du contexte.
- Une session passée ouverte depuis l'URL affiche son historique relu.
- `Md.Check` couvre `` ```csharp `` → `.cb` avec l'en-tête « C# », un bloc sans langage → « texte », et `[!WARNING]` → « Attention ».
- Rendu conforme à `screens/02.png`, `06.png` et `10.png` (sans `.phases` ni cartes « trouvaille »), à la donnée près.

### WP2 : composer et inspecteur

**Fichiers :**
- `Session/Composer.razor`, `Session/SlashPopover.razor`, `Session/ModelMenu.razor`
- `Session/Inspector.razor`, `Session/ToolDetail.razor`, `Session/PermissionCard.razor`, `Session/ContextPanel.razor`, `Session/AgentPanel.razor`
- `Shared/DiffView.razor`, `Shared/FileView.razor`, `Core/EditDiff.cs` (+ `EditDiff.Check`)

**Critères d'acceptation :**
- Composer :
  - Ctrl ⏎ envoie ;
  - placeholders et bouton principal suivent l'état (table du rapport maquettes §2.5) ;
  - la méta affiche tour / session / outils / contexte, avec le coût **cumulé**.
- Modèle : le menu liste `initialize.models` (`displayName`) ; un choix appelle `set_model`, et le tour suivant indique le nouveau modèle dans `system/init`.
- Effort : un clic appelle `apply_flag_settings {effortLevel}`, puis `get_settings.applied.effort` reflète la valeur, avec `.on` sur le bon bouton.
- Rapide : désactivé avec la raison en infobulle quand `fast_mode_state=="off"`.
- Ultracode : un tour avec le toggle actif envoie `{ultracode:true}` (lu dans `get_settings.applied.ultracode`), puis `{ultracode:false}` après le `result`. Un essai réel (coûteux, modèle par défaut) note dans le PR si des sous-agents apparaissent ; sinon le toggle reste, l'infobulle disant qu'il active le réglage CLI.
- Saisir `/` ouvre `.pop.slash` filtré, avec descriptions et étiquette `intégrée` ou nom de plugin. ↑ / ↓ / Tab fonctionnent. `/cost` envoyé affiche la sortie synthétique dans le fil.
- Permission :
  - Edit montre un `DiffView` (`EditDiff.FromInput`) dont les lignes `+`/`−` sont celles du `structuredPatch` reçu après coup ;
  - PowerShell/Bash montre `pre.cmd` ;
  - Autoriser, Refuser (message FR, ligne `.t.void` « refusé ») et « Toute la session » via `setMode` fonctionnent, au clic comme au clavier (⏎, Maj ⏎, Suppr) ;
  - « 1 sur 2 » et « Aussi en attente » reflètent `SessionManager.DecisionQueue`.
- Le cas `updatedPermissions` est testé une fois et le résultat noté dans le PR. S'il échoue, le bouton est masqué pour ces suggestions.
- `ToolDetail` : onglet Diff ou Sortie selon l'outil, Entrée et JSON brut. Read est rendu en `FileView` coloré par hljs.
- `ContextPanel` : affiche `get_context_usage` (barre + légende). « Compacter maintenant » est testé ; s'il échoue, il est désactivé.
- `AgentPanel` : affiche le dernier texte de l'agent et le `.kv` type / outils / tokens / coût `—`.
- `EditDiff.Check` couvre un remplacement d'une ligne au milieu, un ajout pur, Write sur un fichier absent, MultiEdit à deux hunks et `FromPatch` sur le `structuredPatch` du rapport protocole.
- Rendu conforme à `screens/03.png` et `09.png`.

### WP3 : navigation, démarrage, vue d'ensemble, historique

**Fichiers :**
- `Layout/Rail.razor`, `Layout/RailSessionRow.razor`, `Layout/QuotaMeter.razor`, `Layout/Mascot.razor`, `Layout/CommandPalette.razor`
- `Pages/NewSession.razor`, `Pages/Overview.razor`
- `Core/TranscriptStore.cs`

**Critères d'acceptation :**
- s1 :
  - les dossiers récents viennent de `TranscriptStore.Recent` ;
  - un dossier inexistant affiche « Ce dossier n'existe pas. » ;
  - le toggle worktree propose un nom slugifié, modifiable, et passe `-w` ;
  - les 4 modes sont présents (`default` envoyé comme `manual`) ;
  - « Démarrer » crée la session, envoie le prompt et navigue vers `/session/{id}`.
- Rail :
  - « Actives » : sessions vivantes dans l'ordre de création. « Récentes » : `TranscriptStore.Recent` moins les ids vivants ;
  - pastilles et suffixes de droite selon le statut ;
  - compteurs « à nettoyer » (`WorktreeService.CleanableCount`, masqué si `null` ou 0) et MCP en échec (`Init.Mcp` de la session vivante la plus récente) masqués à 0 ;
  - le quota apparaît seulement après un `rate_limit_event` ou `get_usage` ;
  - l'humeur du canard suit la table de priorité du rapport maquettes §2.2.
- Palette Ctrl K :
  - filtre insensible à la casse avec `<mark>` encodé ;
  - ↑ / ↓ / ⏎ / Échap ;
  - actions Nouvelle session, Vue d'ensemble, Worktrees, Extensions, Apparence ;
  - « Thème : X » passe au thème suivant via `claudeUi.setTheme`.
- s4 : totaux, tableau stable, file des décisions masquée si vide, « aujourd'hui » = somme des coûts des sessions démarrées aujourd'hui.
- Rendu conforme à `screens/01.png`, `04.png` et `05.png`.

### WP4 : worktrees

**Fichiers :** `Core/WorktreeService.cs` (+ `WorktreeService.Check`), `Pages/Worktrees.razor`.

**Critères d'acceptation :**
- `WorktreeService.Check` couvre `Classify` sur au moins ces cas :
  - session active dans l'app → Actif ;
  - verrou avec pid vivant → Actif ;
  - dossier absent → Orphelin ;
  - sale → À vérifier ;
  - commits non poussés et non mergés → À vérifier ;
  - verrou à pid mort mais tout propre et mergé → À vérifier, plan avec `unlock` ;
  - mergé et propre → Sûr ;
  - squash → Sûr, branche gardée ;
  - frais → Sûr ;
  - inconnu → À vérifier.
- Contre un dépôt jetable avec un worktree par état (celui du scratchpad de recherche, `scratchpad\wt\repo`, s'il existe encore ; sinon le recréer par un petit script dans le scratchpad du WP), chaque ligne est classée comme dans le rapport worktrees §3.
- La page s8 affiche les groupes, les filtres et la taille calculée en arrière-plan (`…` en attendant). L'inspecteur « Nettoyage » montre le plan et les commandes exactes avant confirmation. Les actions par ligne suivent §2.8.
- L'exécution diffuse chaque commande et son résultat dans `.cmdlog`. `WorktreeService.Check` vérifie qu'aucune étape de `Plan` (tous cas confondus) ne contient `--force`, `-f` ou `-D` (un `Debug.Assert` ne tournerait pas en Release).
- Avertissement si `.claude/worktrees/` n'est pas ignoré par git.
- Rendu conforme à `screens/08.png`.

### WP5 : extensions et apparence

**Fichiers :** `Pages/Extensions.razor`, `Pages/Appearance.razor`.

**Critères d'acceptation :**
- s11 :
  - source = la session vivante la plus récente, via `RefreshMcp()` (`mcp_status`) ; sans session, l'état vide « Démarre une session pour voir ce que le CLI charge. » ;
  - onglets MCP / Skills / Agents / Plugins avec leurs compteurs, filtres par état ;
  - l'inspecteur d'un serveur en échec montre `.errbox` avec `error` ;
  - `mcp_toggle` et `mcp_reconnect` testés une fois sur un vrai serveur ; contrôle désactivé si échec ;
  - « Se connecter » désactivé, « bientôt ».
- s7 :
  - 5 cartes `.tc[data-theme]` dont l'aperçu suit leurs propres tokens ;
  - un clic applique le thème immédiatement et le persiste ;
  - la taille du code (12 à 15) est persistée ;
  - l'aperçu passe par `Md.Render` ;
  - l'état initial est lu par `OnAfterRenderAsync(firstRender)`.
- Rendu conforme à `screens/07.png` et `11.png`.

### Ordre et intégration

```
WP0 ──► { WP1, WP2, WP3, WP4, WP5 } en parallèle ──► intégration (lead)
```

- Aucun fichier n'appartient à deux WP (§5.0 point 5), `app.css` compris (WP0 seul).
- Chaque WP travaille dans **son propre worktree git** créé depuis le commit de WP0 (`git worktree add ../ClaudeCodeUI-wpN`) : un seul dossier partagé ferait se battre les `dotnet build` sur `obj/` et les `dotnet run` sur le port. Chaque WP lance l'app sur son propre port (`--urls http://localhost:51N0`).
- Un WP qui a besoin d'un changement de contrat le signale au lieu de modifier un fichier qui ne lui appartient pas.
- Intégration :
  1. build ;
  2. self-check ;
  3. parcours complet s1→s11 ;
  4. passe de captures (§6) ;
  5. suppression des stubs restants.

---

## 6. Vérification

1. **Build** : `dotnet build` avec 0 erreur et aucun nouvel avertissement ; on peut ajouter `<TreatWarningsAsErrors>true</TreatWarningsAsErrors>` à l'intégration si c'est déjà propre.
2. **Contrôle exécutable** : `dotnet run -- --self-check` exécute les `Check()` décrits plus haut (événements, réducteur, Md, EditDiff, classement des worktrees). Il sort avec un code non nul et un message clair au premier échec. Pas de projet xunit : un dossier `tests/` sous la racine serait compilé dans le projet web (glob `**/*.cs` par défaut), et un seul point d'entrée suffit.
3. **Parcours manuel** avec `dotnet run` et `--model haiku` dans un dépôt jetable du scratchpad : nouvelle session avec worktree, Edit autorisé, Write refusé, interruption, sous-agent, `/cost`, crash forcé, relance, reprise depuis « Récentes », nettoyage du worktree créé.
4. **Captures** : navigateur headless (MCP chrome-devtools ou `msedge --headless --screenshot --window-size=1440,900`). On compare page par page avec `docs/mockups/screens/NN.png`, à données différentes près : structure, espacements, couleurs, polices. On fait passer les 5 thèmes au moins sur s2. Les écarts acceptés sont listés dans le PR.
5. **Pas de régression de sécurité** : Markdig garde `DisableHtml()`, et tout texte inséré dans un `MarkupString` (palette, `Md`) est encodé avec `HtmlEncoder`.

---

## 7. Risques et questions ouvertes

| Risque | Mitigation |
|---|---|
| WP0 est gros et bloque tout | il ne livre que contrats, réducteur, coquille et CSS ; les stubs permettent de démarrer les WP dès qu'il compile |
| Débit du flux partiel (une notification par delta) | limitation à 50 ms dans `LiveSession` ; Markdown rendu une fois par bloc complet, texte brut pendant le flux |
| Lecture concurrente des listes pendant le rendu | `ImmutableList` à copie à l'écriture, un seul écrivain par session |
| `updatedPermissions`, `mcp_toggle`, `mcp_reconnect`, `/compact`, effet d'ultracode, relecture du diff depuis le `.jsonl` non testés en vrai | test unique par le WP propriétaire ; sinon contrôle désactivé (§1.2) |
| Mode rapide presque toujours indisponible sur ce compte (`extra_usage_disabled`) | affiché désactivé avec la raison, ce qui correspond à la maquette |
| Les sessions vivantes meurent au redémarrage du serveur | elles réapparaissent dans « Récentes » et se reprennent via `--resume` ; pas de persistance propre |
| Worktrees `-w` verrouillés par pid, puis verrou périmé | classés « À vérifier », `unlock` explicite dans le plan, jamais `--force` |
| Scan de `~/.claude/projects` lent (865 dossiers) | 30 derniers jours, 64 Ko par fichier, cache, rescan sur « Rafraîchir » |
| Ctrl N intercepté par le navigateur | Alt N en plus, libellé conservé |
| Hooks de l'utilisateur qui produisent du bruit (`hook_*`, `stop-hook-error`) | ignorés par `Events.Parse` |
| `bypassPermissions` / `dontAsk` existent dans le CLI | volontairement non exposés dans l'interface |

**Décisions par défaut (modifiables) :**
1. Ultracode s'applique « pour ce tour », comme dans la maquette.
2. Le mode `auto` est proposé dans s1, avec la mention « à utiliser avec prudence ».
3. Les polices Geist (licence OFL, ~100 Ko) sont auto-hébergées dans le dépôt.
4. Sans worktree, une session prend par défaut le slug des 4 premiers mots du prompt.
