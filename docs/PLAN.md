# Implementation plan: graphite console

Branch `feat/graphite-console`. Goal: replace the single page `Components/Pages/Home.razor` with the application of the 11 screens of `.impeccable/mockups/graphite.html` (screenshots in `docs/mockups/screens/01..11.png`), on the real `claude` 2.1.295 CLI in stream-json.

Sources of truth, in priority order:
1. The protocol report (captures `s1..s7.jsonl` in the scratchpad). Only **VERIFIED** points become features.
2. `PRODUCT.md`: no invented features, dark themes only, French interface.
   *Since then: the interface is in English and French.* Texts live in `Resources/Strings.resx` (English, neutral) and `Strings.fr.resx` (the original French, unchanged); components use `IStringLocalizer<Strings>`, `Core/` code uses `Strings.Get`. Language: cookie set by the picker on the Appearance page (`/culture`, reload), otherwise `Accept-Language`, otherwise English; `<html lang>` follows. `--self-check` verifies that both resx files have the same keys.
3. `.impeccable/mockups/graphite.src.html`: the only CSS source (lines 10–526) and the reference markup.

When the reports contradict each other, this plan sides with the verified protocol. Examples: partial streaming is **in** scope, and `set_model` / `apply_flag_settings` are verified.

---

## 1. Scope

### 1.1 Built for real

| Screen | Feature | Verified mechanism |
|---|---|---|
| s1 | New session: folder, worktree, mode, first prompt | flags `--permission-mode` (always explicit; `default` is sent as `manual`), `-w <name>`, `--name`, `--session-id` |
| s2 | Live text + `.caret` cursor | `--include-partial-messages` (`stream_event` / `text_delta`) |
| s2 | Tool ledger, Output/Input/JSON inspector | `assistant` `tool_use` + `user` `tool_result` (+ `tool_use_result` for the diff) |
| s2 | Thinking indicator | `system/thinking_tokens` (not the text, which is always empty) |
| s2 | Pinned task list (pending / in progress / completed), progress in the tool rows and the Overview | `--allowedTools=TodoWrite,TaskCreate,TaskGet,TaskUpdate,TaskList` opts newer models into the task tools (without it 2.1.296 exposes none); `TaskCreate {subject,activeForm?}` → `tool_use_result.task.id`, `TaskUpdate {taskId,status,…}`, or `TodoWrite {todos[]}` when `CLAUDE_CODE_ENABLE_TASKS=false`; rebuilt from the tool calls, so replay agrees; `/clear` (`conversation_reset`) starts a new list whose ids restart at 1; verified by `--probe-cli todo` (`todo-tools`, `todo-clear`) |
| s3 | Permission with diff, Allow / Deny | `control_request can_use_tool` → `control_response allow/deny` |
| s3 | "Whole session" when the suggestion is `setMode` | `set_permission_mode` then `allow` |
| s3 | "Whole session" for the other suggestions (`addRules`, `addDirectories`) | `allow` + `updatedPermissions`; verified by `--probe-cli permission-session` (0 new requests on the next turn, 1 without `updatedPermissions`) |
| s3 | Plan approval: the plan as Markdown, approve (auto-accept or review edits) or keep planning with feedback | `can_use_tool` `ExitPlanMode` → `set_permission_mode` then `allow`, or `deny` + `message`; verified by `--probe-cli plan` (§2.5) |
| s3 | AskUserQuestion answer form (radios / checkboxes + free-text "Other", Answer or Skip) | the tool's `can_use_tool` (`input.questions[{question,header,options[{label,description}],multiSelect}]`) answered `allow` + `updatedInput` = input + `answers {question text: answer}`, multi-select labels joined by `", "`; Skip = `deny`. Verified by `--probe-cli ask-user-question` (the next assistant turn quotes a free-text answer); a bare `allow` makes the tool answer "The user did not answer the questions." The answers come back in `tool_use_result.answers` (`toolUseResult` in the transcript) for the ledger and replay |
| s4 | Overview, decision queue | in-memory state of all sessions |
| s5 | Ctrl K palette, crash card, restart | process exit + `--resume <id>` |
| s6 | Markdig Markdown + code blocks + hljs | existing; `.cb` wrapper done server-side |
| s7 | 5 themes + code size, remembered | `localStorage` + JS interop |
| s8 | Worktrees: classification and safe cleanup | `git` (commands listed in the worktrees report) |
| s9 | Model choice | `initialize.models` + `set_model` |
| s9 | Effort | `--effort` at launch, `apply_flag_settings {effortLevel}` mid-session, read via `get_settings.applied.effort` |
| s9 | Fast mode | `apply_flag_settings {fastMode}`. Displayed according to `fast_mode_state` / `fast_mode_disabled_reason`: often disabled, with the reason in a tooltip |
| s9 | Ultracode | `apply_flag_settings {ultracode:true}` before the turn, `{ultracode:false}` after the `result`, availability via `get_settings.applied.ultracodeAvailable`; verified by `--probe-cli ultracode-on` / `ultracode-off` |
| s9 | Images in the composer (paste, drop, paperclip picker; png/jpeg/gif/webp, 5 MB and 10 per message), thumbnails on the user message, replayed from the transcript | user `content` = a `text` block (omitted when empty) then `image` blocks `{source:{type:"base64",media_type,data}}`; verified by `--probe-cli image` / `image-only` (haiku reads a random number drawn in a generated PNG and the JPEG/GIF/WebP fixtures, PASS on 2.1.296). The transcript stores the block (CLI re-encoded) on the same `user` line, plus an `isMeta` `[Image: source: …]` line that replay skips. The CLI re-encodes images itself (a 12 MB PNG and an 8550 px one were read): the 5 MB cap is the UI's own. Files reach the server through `InputFile` streaming, `MaximumReceiveMessageSize` untouched |
| s9 | `/` popover with descriptions | `initialize.commands[{name,description,argumentHint}]`; `/xxx` sent as user text |
| s9 | `@` file mentions (popover over the cwd: `git ls-files -co --exclude-standard`, else a bounded walk skipping `bin`/`obj`/`node_modules`/`.git`) | `@path` / `@"path with spaces"` sent as user text: the CLI attaches the file (or a folder listing) itself; verified by `--probe-cli file-mention` / `file-mention-quoted` / `file-mention-folder` on 2.1.296 (all file tools disallowed, 0 `tool_use`, the model quotes a random code word from the mentioned file, or a random file name from the mentioned folder). The mention is the one at the end of the text, not at the caret |
| s9 | Context panel | `get_context_usage` |
| s9 | Memory files (`/memory`): list, view, edit and save | `get_context_usage.memoryFiles[{path,type,tokens}]` (walk up from cwd, `@imports` included; types `User`/`Project`/`Local`), plus the standard locations it did not load; saving only writes a listed path, refused if the file changed since it was read. An edit is not seen by the running session until `/compact` or the next session: verified by `--probe-cli memory-files` / `memory-reload` |
| s9 | "Compact now" | `/compact` as user text; verified by `--probe-cli compact` (`system/compact_boundary`, context going down) |
| s2 | Send while a turn runs: queued chips, cancel | `user` message with our own `uuid`; its fate comes back as `command_lifecycle {command_uuid, state}`: `queued`, then `started` at the next tool result (folded into the running turn, one `result` lists both uuids) or after the `result` (its own turn, also after an interrupt, which answers `still_queued`); `cancel_async_message {message_uuid}` → `{cancelled:true}` + `cancelled`. Verified by `--probe-cli queue` (PASS on 2.1.296) |
| s2 | "Rewind to here" on a user message: confirmation listing the files, files restored, thread cut, text back in the Composer | env `CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING=1` at launch, a `uuid` on each sent `user` message, `rewind_files {user_message_id, dry_run}` (→ `canRewind`, `filesChanged`, `insertions`, `deletions`) then `rewind_conversation {target_message_uuid}` (→ `rewound`, `prefillText`; only the latest message, else `"stale target"`, so once per message from the newest back to the target) and only then `rewind_files {user_message_id}`; reopening from Récentes replays only the active branch (`last-prompt {rewound, leafUuid}`, `parentUuid`); verified by `--probe-cli rewind-files` / `rewind-conversation` (2.1.296). Only for messages sent by the current process: the transcript keeps the CLI's own uuid, not ours. After a background agent's notification turn (a user message the CLI has and the UI does not track), our newest uuid is no longer the latest: the first step answers `"stale target"` and the rewind stops safely (nothing cut, files untouched) until the next send (from the CLI's rule, not probed) |
| s10 | Sub-agents: one row per agent, sub-tools, agent text | `tool_use name:"Agent"`, `system/task_*`, `parent_tool_use_id`, `--forward-subagent-text` |
| insp | Background tasks: shell `run_in_background` and `Monitor` commands, status, elapsed, last output lines, Stop | `system/task_started` with `task_type:"local_bash"` + `is_backgrounded:true`, `system/task_updated` `patch.status`/`end_time`, `get_task_output {task_id}` → `{output,total_bytes,truncated}` (last 8 KiB), `stop_task {task_id}` → `task_updated` `killed` (it answers `{}` even for an id it ignores: only `task_updated` proves the stop), natural end → `task_updated` `completed` + `end_time`; verified by `--probe-cli background-tasks` (one run reports `background-tasks`, `stop-task`, `task-completed`) and `--probe-cli monitor-stop` (a `Monitor` is `local_bash`, backgrounded, has `input.command`, and stops the same way) and `--probe-cli exit-ends-tasks` (disposing the claude process takes its background shells with it: the UI marks them `killed`, no `task_updated` comes), all PASS on 2.1.296, macOS. "Clear" only drops ended rows from the UI |
| s11 | MCP list (state, tools, error, transport) | `mcp_status` |
| s11 | MCP toggle and "Retry" | `mcp_toggle {serverName,enabled}`, `mcp_reconnect {serverName}`; verified by `--probe-cli mcp-toggle` / `mcp-reconnect` (the state read in `mcp_status` follows the toggle) |
| s11 | MCP "Sign in" (`needs-auth`) | `mcp_authenticate {serverName}` answers `{authUrl, requiresUserAction, callbackExpected, redirectScheme, state, callbackPort}`; the CLI takes the browser redirect on its own localhost port and reconnects the server, so `mcp_status` is polled until it leaves `needs-auth`. Verified end to end by `--probe-cli mcp-auth` against a local OAuth-protected fake server (PASS on 2.1.296); `mcp_clear_auth` drops the token. claude.ai connectors (`config.type: claudeai-proxy`) answer `callbackExpected: false` and stay `needs-auth` after it (checked on a real connector, 2.1.296): no in-app button, the `/mcp` terminal route only |
| s11 | Skills / Agents / Plugins | `initialize` (`agents`, `commands`) + init (`skills`, `plugins`) |
| s11 | Hooks tab (read-only: event, matcher, command, type, source) | `get_hooks_listing` → `hooks[{event,matcher,type,commandText,displayText,source,sourceLabel,pluginName?,timeout?,disabled?}]` + `policy` (`disabled: true` on every row under `disableAllHooks`, checked by hand on 2.1.296); verified by `--probe-cli hooks` (`hooks-listing`) |
| s2 | Hook activity row (a hook that failed, blocked or printed) | `--include-hook-events` → `system/hook_response {hook_name,outcome,exit_code,output}`; without the flag only `SessionStart` hooks are streamed. Verified by `--probe-cli hooks` (`hooks-events`: exit 2 → `outcome:"error"`). Live stream only, not replayed: transcripts store hooks as `hook_success` / `hook_additional_context` attachments, so a session reopened from history shows no hook rows |
| rail | 5 h / 7 d quota | `rate_limit_event.rate_limit_info.unifiedWindows`, plus `get_usage` at startup |
| rail | "Recent" and resuming with history | reading `~/.claude/projects/<slug>/*.jsonl` + `--resume` (the CLI **does not replay** history) |
| header, palette, thread | "Fork" / "Fork from here": a new session `<name> (fork)` continuing the conversation, the original untouched | `--resume <id> --fork-session --session-id <new> --name <name>` (+ `--resume-session-at <assistant uuid>`); history = the source transcript, cut after that uuid; a fork not sent to yet has no transcript (`--resume` on it: "No conversation found"), so forking it forks its source at the same cut; verified by `--probe-cli fork` (5 PASS on 2.1.296) |

### 1.2 Built, but to be validated once for real (fallback: control disabled)

These requests were accepted but never tested on the real case. On failure, the control is disabled (`aria-disabled`, "coming soon" tooltip) instead of being invented. `dotnet run -- --probe-cli [id...]` (`Core/CliProbe.cs`) replays these tests against the real CLI (haiku, throwaway repo), all of them or only the given ids: rerun it on every CLI version bump. Ultracode, "Whole session" outside `setMode`, MCP toggle/reconnect and `/compact` passed there (PASS on 2.1.295) and now appear in §1.1.

- Transcript replay: the shape of the `user`/`assistant` lines of the `.jsonl` is assumed identical to the stream, but the diff field (`tool_use_result` in the stream) has not been verified in the file. Fallback: `EditDiff` recomputes from the tool input.

### 1.3 Shown disabled ("coming soon") or omitted, because the CLI does not provide it

| Mockup element | Treatment | Reason |
|---|---|---|
| Thinking text | omitted (only the indicator remains) | `thinking_delta` is always empty |
| Workflow phases, `review-changes` name, "finding" cards, cost per agent | omitted; cost shown as `—` | no CLI data (only tokens and a duration per agent exist) |
| "Auto at 80 %" | read-only label `Auto at {autoCompactThreshold}` if `isAutoCompactEnabled`, otherwise nothing | no verified settings request |
| "Open configuration" | disabled "coming soon" button | not built |
| Editing hooks | omitted (the Hooks tab is read-only) | out of scope: the CLI owns the settings files |
| "Browse", non-image "Attach" | omitted | a browser cannot browse the server's folders; only images are attached (§1.1) |
| "PR #212 merged", "Automatically" toggles (s8) | omitted | would need `gh`; speculative |
| `Ctrl ⏎ open alongside` (palette) | omitted | no split view |
| AI session title | omitted; we pass `--name` ourselves | no `ai-title` in `-p`; `generate_session_title` records nothing |
| `bypassPermissions` / `dontAsk` modes | not offered (s1 offers `default`, `acceptEdits`, `plan`, `auto`) | not shown in the mockup; security risk |

---

## 2. Architecture

### 2.1 Files (target state)

```
Program.cs                     DI + --self-check
ClaudeSession.cs               process + stdin/stdout (modified)
Core/Events.cs                 typed records + Parse(JsonElement)
Core/LiveSession.cs            state of one session + reducer Apply(ClaudeEvent)
Core/SessionManager.cs         singleton: live sessions, decision queue
Core/TranscriptStore.cs        static: past sessions + reading a .jsonl
Core/WorktreeService.cs        singleton: scan / classification / plan / git execution
Core/UiState.cs                scoped: inspector, palette, selection
Core/Fmt.cs                    fr-FR formats (durations, sizes, %), invariant costs
Core/ToolKinds.cs              colour + target of a tool
Core/EditDiff.cs               diff lines for Edit/Write/MultiEdit
Core/Md.cs                     Markdig + .cb wrapper + FR alert titles
Core/SelfCheck.cs              executable checks (§6)
Components/...                 see §4
wwwroot/app.css, app.js, fonts/
```

No interface (a single implementation per service) and no server-side settings: the theme and code size live in `localStorage`, recent folders come from `TranscriptStore`.

### 2.2 `Program.cs`

```csharp
if (args is ["--self-check"]) { SelfCheck.Run(); return; }   // exit code != 0 on failure
builder.Services.AddRazorComponents().AddInteractiveServerComponents();
builder.Services.AddSingleton<SessionManager>();   // IAsyncDisposable: kills the processes on shutdown
builder.Services.AddSingleton<WorktreeService>();
builder.Services.AddScoped<UiState>();
```

### 2.3 `ClaudeSession` (changes)

- New signature: `ClaudeSession(string cwd, IReadOnlyList<string> args, Func<JsonElement,Task> onEvent, Action<int,string> onExit)`. The base flags stay in the class. `args` is built by `LiveSession`:
  - always: `--permission-mode <mode>` (`default` is sent as `manual`), `--include-partial-messages`, `--forward-subagent-text`, `--include-hook-events`
  - new session: `--session-id <uuid>` and `--name <name>`
  - resume: `--resume <id>` (without `--session-id`; the id stays the same)
  - depending on the s1 form: `-w <name>`, `--model <m>`, `--effort <e>`
- `Task<JsonElement> Request(string subtype, JsonObject? fields = null)`: writes a `control_request` with a unique `request_id` and awaits the response via a `ConcurrentDictionary<string, TaskCompletionSource<JsonElement>>`. The reader intercepts `control_response` (`success` → result of `response.response`; `error` → `ClaudeRequestException(error)`), then still forwards the event. Timeout: 15 s.
- `Interrupt()` becomes `Request("interrupt")`.
- `Respond(requestId, allow, input, JsonNode? updatedPermissions = null)`. Denial message: `"Refusé par l’utilisateur"` (French text sent to the CLI, "Denied by the user"; kept unchanged because it is compared in tests).
- `onExit(exitCode, "claude exited {code} {stderr}")`. Call nothing if the process was disposed on purpose: a `disposing` boolean is enough.

### 2.4 Event model (`Core/Events.cs`)

A single parsing point, `static ClaudeEvent? Parse(JsonElement e)`. It returns `null` for what we ignore: `hook_started` (`hook_response` becomes a `HookEvt`), `system/notification`, `message_start/stop` and unknown types.

```csharp
abstract record ClaudeEvent;
record InitEvt(string SessionId, string Cwd, string Model, string PermissionMode, string[] Tools,
               McpBrief[] Mcp, string[] SlashCommands, string[] Skills, string[] Agents, string[] Plugins,
               string FastModeState, string? FastModeReason) : ClaudeEvent;
record McpBrief(string Name, string Status);                       // connected|failed|needs-auth|pending
record StatusEvt(string? Status, string? PermissionMode) : ClaudeEvent;   // "requesting", mode change
record ThinkingEvt(int EstimatedTokens) : ClaudeEvent;
record TextDeltaEvt(string Text, string? ParentToolUseId) : ClaudeEvent;  // stream_event content_block_delta text_delta
record AssistantTextEvt(string MessageId, string Text, string? ParentToolUseId, bool Synthetic) : ClaudeEvent;
record ToolUseEvt(string Id, string Name, JsonElement Input, string? ParentToolUseId) : ClaudeEvent;
record ToolResultEvt(string ToolUseId, string Text, bool IsError, JsonElement? Structured) : ClaudeEvent; // Structured = tool_use_result
record UserTextEvt(string Text) : ClaudeEvent;                     // transcript replay / "[Request interrupted by user]"
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

Parsing rules:
- The `total_cost_usd` cost is **cumulative** over the session and across `--resume`. It is assigned, never added. The cost of a turn is the difference from the previous value.
- `ContextTokens` = `usage.input_tokens + cache_read_input_tokens + cache_creation_input_tokens`. `ContextWindow` = `modelUsage[*].contextWindow`.
- `assistant` arrives **one block per message** (same `message.id`). Each `text` block yields an `AssistantTextEvt` and each `tool_use` a `ToolUseEvt`. `model:"<synthetic>"` yields `Synthetic=true` (output of `/` commands).
- The `user` and `assistant` messages of a `.jsonl` transcript have the same shape. `TranscriptStore` therefore feeds them to the same `Parse` (diff field not verified in the file, §1.2).
- `control_cancel_request` has never been observed: no dedicated event. A `PendingPermission` is removed when it is answered, or at `ResultEvt` / process exit.
- Tools (via `ToolKinds`): the shell is **`PowerShell`** on this machine (`Bash` is not in `tools`); both have the `--bash` colour and the `pre.cmd` view. The sub-agent is called **`Agent`** in messages (and `Task` in `tools`): both names yield a `.wf`.
- `system/task_updated` only feeds the background tasks panel (`LiveSession.Tasks`, `local_bash` tasks); agent rows keep ending on `task_notification`.
- `system/status`, `system/background_tasks_changed`, `message_delta` and `stream_event` other than `text_delta` are ignored unless mentioned above.

### 2.5 Per-session state (`Core/LiveSession.cs`)

```csharp
enum SessionStatus { Starting, Idle, Running, Waiting, Exited, Crashed }
sealed class LiveSession {
  string Id;                 // == --session-id == name of the .jsonl == route /session/{Id}
  string Name, Cwd, Mode; string? Branch, Repo, Worktree;      // Cwd replaced by init.cwd (-w case)
  SessionStatus Status;
  ImmutableList<Item> Items;                 // copy-on-write: lock-free rendering
  ImmutableList<PendingPermission> Pending;
  string StreamingText;                      // partial text of the current block (parent null)
  int ThinkingTokens;
  decimal CostUsd; decimal LastTurnCostUsd; int ToolCount;
  DateTimeOffset StartedAt, LastEventAt; DateTimeOffset? TurnStartedAt; TimeSpan? LastTurn; string? LastResultSubtype;
  int? ExitCode; string? ExitText;
  InitEvt? Init; JsonElement? InitializeInfo;          // "initialize" response (models, commands, agents, account)
  string? Model, Effort; bool Ultracode; string FastModeState; string? FastModeReason;
  JsonElement? Context; JsonElement? McpStatus;       // raw responses of get_context_usage / mcp_status, read by WP2 / WP5
  DateTimeOffset? LastResultAt;                         // duck mood "Turn finished"
  event Action? Changed;
  Task Send(string text); Task Answer(PendingPermission p, Decision d); Task Interrupt(); Task Restart();
  Task SetModel(string m); Task SetEffort(string e); Task SetFast(bool on); Task SetUltracode(bool on);
  Task RefreshContext(); Task RefreshMcp(); Task<JsonElement> Request(string subtype, JsonObject? f = null);
  internal void Apply(ClaudeEvent e);        // pure reducer on the state: tested by SelfCheck
}
abstract record Item;
record UserItem(string Text, DateTimeOffset At, bool Ultracode) : Item;
record TextItem(string Markdown, string? ParentToolUseId) : Item;
sealed record ToolItem(string Id, string Name, JsonElement Input, string? ParentToolUseId) : Item {
  public ToolState State; public string? ResultText; public JsonElement? Structured;
  public DateTimeOffset StartedAt; public DateTimeOffset? EndedAt;
  public string? TaskId; public long Tokens; public int SubToolUses;     // Agent only
}
enum ToolState { Running, Done, Error, Waiting, Denied }
record PendingPermission(string RequestId, string Tool, JsonElement Input, string? Description, string? ToolUseId, JsonElement? Suggestions);
enum Decision { Allow, AllowSession, Deny }
```

Transitions:
- `Send` → `Running`, `TurnStartedAt = now`. During a turn (`Running`/`Waiting`) `Send` only adds a `QueuedMessage` (chip): `QueueEvt started` moves it into `Items` as a `UserItem`; `cancelled`/`discarded`/`refused` drop it. A `ResultEvt` with messages still queued keeps (or, from `Waiting`, returns to) `Running` (the CLI starts the next one itself). A queued message started inside or right after an ultracode turn is labelled ultracode (the reset lands after the CLI started it; not probed). Process exit clears the queue. On replay, a folded message is the `attachment {type:"queued_command", prompt, commandMode:"prompt"}` line after the tool result, not a `user` line.
- `PermissionEvt` → `Waiting`; the matching `ToolItem` goes to `ToolState.Waiting`.
- A denial marks the `ToolItem` `Denied`.
- `ResultEvt` → `Idle`, or `Idle` + "interrupted" if `terminal_reason == aborted_streaming`.
- Unexpected process exit → `Crashed`. Deliberate `Dispose` → `Exited`.

Concurrency and rendering:
- The reducer runs on the process reader thread (a single writer per session). The lists are `ImmutableList`s replaced on every change, and components read them without a lock.
- `Changed` is **throttled to 1 notification / 50 ms** during the `TextDeltaEvt` stream. Immediate notification for everything else.

Startup:
- A new session launches the process right away, then sends `initialize`, `get_settings` and `get_usage`. This fills in the model, commands, effort and quota before the first message; `system/init` only arrives after that first message.
- `initialize` is awaited **only once** (same `request_id`, 180 s, "still waiting" line at 60 s): resending under a new id lost the late response to the first one. During `Starting`, the startup timeline (spawn, writes, live stderr, late responses) goes to the console as `[<id>] boot …`. `dotnet run -- --boot-probe <folder> [s]` replays a cold start outside the UI; the cause of the 60 s stall is not reproduced by the probe (issue #6).
- A past (resumed) session is created with `Items` loaded by `TranscriptStore.Load(id)`. Its `--resume` process is only launched on the first `Send` or on clicking "Resume".

Ultracode "for this turn":
- `Send` with the toggle active calls `apply_flag_settings {ultracode:true}` before sending the message.
- On `ResultEvt`, we send `{ultracode:false}` back and reset the toggle.
- `UserItem.Ultracode=true` shows the `.ultra-chip` chip.

"Whole session":
- `setMode` suggestion: `set_permission_mode {mode}` then `allow`.
- Otherwise: `allow` + `updatedPermissions = suggestions` (verified by `--probe-cli permission-session`).

Plan approval (`can_use_tool` for `ExitPlanMode`, verified on 2.1.296 by `--probe-cli plan`):
- The request carries `input.plan` (Markdown) and `input.planFilePath`, and **no** `permission_suggestions`. A bare `allow` makes the CLI drop to `default` by itself (`system/status permissionMode:"default"`).
- "Approve and auto-accept edits": `set_permission_mode {mode:"acceptEdits"}` (answers `{"mode":"acceptEdits"}`) then `allow`; `system/status` follows and the next edit is not prompted.
- "Approve and review each edit": the same with `default`; the next edit is prompted.
- "Keep planning": `deny` with the user's feedback as `message`; the model gets it verbatim as the `ExitPlanMode` `tool_result` (`is_error`), the mode stays `plan`.

`Restart` creates a new `ClaudeSession` with `--resume Id` in `Cwd` **as replaced by `init.cwd`** (the worktree folder in the `-w` case: that cwd gives the slug of the `.jsonl`). Do not pass `-w` again. If the process died before the first message (no `.jsonl`), restart as a new session with the same `--session-id`.

WP0 implements `LiveSession` **entirely** (reducer + all methods: they are few-line wrappers around `Request`). WP1–5 only call them and validate them for real.

### 2.6 `SessionManager` (singleton)

```csharp
IReadOnlyList<LiveSession> All;                    // creation order, never re-sorted (s4)
LiveSession? Get(string id);
LiveSession Start(string cwd, string mode, string name, string? worktree, string? model, string? effort);
LiveSession Open(PastSession p);                   // history loaded, lazy process
Task Stop(string id);
IEnumerable<(LiveSession S, PendingPermission P)> DecisionQueue;   // all sessions, arrival order
RateLimitEvt? Limits;                              // last received, all sessions combined
event Action? Changed;                             // relayed from each LiveSession.Changed
```

`Branch` comes from `git -C cwd branch --show-current`. `Repo` is the name of the parent folder of `rev-parse --path-format=absolute --git-common-dir`. They are recomputed after `InitEvt`, because the `cwd` may change with `-w`.

### 2.7 `TranscriptStore` (static)

```csharp
record PastSession(string Id, string Cwd, string? Branch, string Title, decimal? CostUsd, DateTimeOffset LastWrite, string? WorktreePath);
static IReadOnlyList<PastSession> Recent(int take = 30);   // top-level *.jsonl, by mtime, last 30 days
static IReadOnlyList<Item> Load(string id);                // searches ~/.claude/projects/*/{id}.jsonl, re-reads user/assistant via Events.Parse + reducer
static string Slug(string cwd) => Regex.Replace(cwd, "[^A-Za-z0-9]", "-");
```

- Read at most 64 KB at the head and 64 KB at the tail of each file.
- Title: `custom-title` → `agent-name` → `ai-title` → `last-prompt` (truncated) → first `user` text.
- Cost: max of `cost-state.totalCostUSD` and of the UI registry `%LOCALAPPDATA%\ClaudeCodeUI\costs\<id>.txt` (written on every costly `result`). `cost-state` is only written if the CLI exits cleanly; `PersistedCost` remains the raw `cost-state` that `--resume` restores.
- Cache the result of `Recent` for 30 s.

### 2.8 `WorktreeService` (singleton)

This is the API of the worktrees report, in a single file. The classification logic is extracted into a **pure** function, `static (WtState, string Why) Classify(WtFacts f)`, tested by SelfCheck. The facts (`Dirty`, `Ahead`, `Merged`, `SquashMerged`, `UpstreamGone`, `Locked`, `LockPid`, `PidAlive`, `Exists`, `Prunable`, `ActiveSessionName`) are collected separately, by the git commands of the report.

Non-negotiable safeguards:
- never `--force` or `-D`;
- recheck every row just before execution;
- the failure of `branch -d` is not fatal;
- confirmation after displaying the exact commands;
- the main worktree is never offered.

The "lock with dead pid" case is classified "To check", with `git worktree unlock` in the plan. This is the normal case for worktrees created by our own `-w` sessions once they have finished.

Signatures frozen by WP0 (bodies in WP4): `DiscoverReposAsync`, `ScanAsync`, `SizeAsync`, `Plan`, `RunAsync`, `PushAsync` (worktrees report §5, `SessionManager` instead of `sessions`), plus `static Classify(WtFacts)` and **`int? CleanableCount`** (Safe + Orphan of the last scan, `null` before the first; read by the WP3 rail).

Per-row actions (s8): Delete / Prune → add the row to the plan; Go → `/session/{id}`; Push → `PushAsync` after confirmation; Open → `/` with the folder pre-filled.

### 2.9 `UiState` (scoped, per circuit)

`bool InspectorOpen = true; bool PaletteOpen; string? SelectedToolId; event Action? Changed; event Action<string>? Key;`

`MainLayout` (WP0) receives all the shortcuts from `app.js` and handles them **itself**: Ctrl K / Ctrl I (toggle `UiState`), Ctrl N / Alt N (`/`), Ctrl ⇧ O (`/overview`), Ctrl ⇧ A (session of `DecisionQueue.First()`). The others (`Escape`, `Enter`, `Shift+Enter`, `Delete`) are relayed as-is by `UiState.Key`: `SessionPage` interrupts on Esc, `PermissionCard` answers on ⏎ / Shift ⏎ / Del, the palette closes on Esc.

`SelectedToolId` also designates an `.agent` row (id of the `Agent` `tool_use`): this is what makes `AgentPanel` appear.

---

## 3. CSS, fonts, themes, JS

- **`wwwroot/app.css`**: lines 10–526 of `graphite.src.html` copied as-is (they already include `md_themes.css`, `parity.css` and the hljs mapping of lines 336–349). Lines 71–82 (`.doc .screen .frame .synthetic`) and 352–356 (`.themebar`) are excluded. Tweaks:
  - remove `.focus-demo` from the selector on line 281;
  - line 276: `.btn[aria-disabled=true],.btn:disabled`;
  - `.effort span`→`.effort>*`, `.seg span`→`.seg>*`, `.subtabs span`→`.subtabs>*`, including in the "polish 2" transition and hover lists.

  Then add the "app glue" block from mockups report §1.4, plus `.search{width:100%}` and `.app:not(:has(>.insp)){grid-template-columns:256px minmax(0,1fr)}`. From the old `app.css`, keep only `#blazor-error-ui` and `.blazor-error-boundary`. `app.css` belongs to WP0 alone: the mockup CSS is final, so WP1–5 do not touch it. A WP that needs a rule notes it in its PR and the integrator adds it (neighbouring `/* WPn */` sections would cause git conflicts, as adjacent hunks overlap).
- **Delete** `Home.razor.css`, `MainLayout.razor.css` and the `vs2015.min.css` link. No per-component scoped CSS.
- **Fonts**: Geist and Geist Mono self-hosted as `wwwroot/fonts/Geist[wght].woff2` and `GeistMono[wght].woff2` (OFL, from the `geist` npm package or the vercel/geist-font repo), declared by `@font-face` at the top of `app.css`. If the download fails, use the Google Fonts `<link>` from lines 7–8 of the mockup. The system fallback stacks are already in the tokens.
- **Themes**: `data-theme` on `<html>` (graphite, encre, ristretto, mousse, contraste). An inline script in `<head>` (before paint) reads `localStorage['claude-ui.theme']` and `['claude-ui.code-size']`. `<html lang="fr">`.
- **`wwwroot/app.js`** (replaces the inline observer of `App.razor`):
  - `window.claudeUi = { getTheme, setTheme, getCodeSize, setCodeSize, copy, registerShortcuts(dotnetRef), focus(el) }`.
  - MutationObserver with rAF debounce:
    1. `pre>code:not(.hljs)` → `hljs.highlightElement`;
    2. `[data-hl] code.lc:not([data-done])` → `hljs.highlight(text,{language,ignoreIllegals:true})`.
  - Delegated click on `.cb .copy` and `[data-copy]` ("Copied" label for 1400 ms).
  - Global shortcuts forwarded to .NET: Ctrl K, Ctrl I, Esc, Ctrl ⇧ A, Ctrl ⇧ O, Ctrl N **and Alt N** (the browser captures Ctrl N), as well as ⏎ / Shift ⏎ / Del when no field has focus and a permission is displayed.
  - **Never** replace a top-level node inside a `MarkupString`. The `.cb` wrapper is done server-side by `Md.cs`.
- **hljs language**: `.razor` / `.cshtml` → `csharp`; extension table in `EditDiff.Lang(path)`.
- **Mascot**: `<PackageReference Include="BlazorKawaii" Version="2.2.0" />`. In `_Imports.razor`, add `@using BlazorKawaii.Common` and `@using RubberDuck = BlazorKawaii.Components.RubberDuck`. No `@using BlazorKawaii.Components`, because of the conflict with `System.IO.File`. Always pass `Color="#FCCC0A"`: the component's default colour is not confirmed.

---

## 4. Component tree

Global interactive rendering: `<Routes @rendermode="InteractiveServer" />` and `<HeadOutlet @rendermode="InteractiveServer" />` in `App.razor`. No more per-page `@rendermode`. In `Routes.razor`, `FocusOnNavigate Selector="h1"` becomes `".ph .ttl"`.

| File | Mockup classes | Screens | WP |
|---|---|---|---|
| `Components/App.razor` | head, theme script | all | 0 |
| `Components/Layout/MainLayout.razor` | `.app` + `<Rail/>` + `@Body` + `<CommandPalette/>` + `<IconSprite/>`; registers the shortcuts | all | 0 |
| `Components/Layout/IconSprite.razor` | SVG sprite (src 530–553) | all | 0 |
| `Components/Shared/Icon.razor` | `svg.i(.sm)` | all | 0 |
| `Components/Layout/Rail.razor` | `.pane .brand .rail-actions .grp .list` | all | 3 |
| `Components/Layout/RailSessionRow.razor` | `.s .dot .n .branch .pill .c` | all | 3 |
| `Components/Layout/QuotaMeter.razor` | `.quota .qrow .bar` | all | 3 |
| `Components/Layout/Mascot.razor` | `.mascot .duck` | all | 3 |
| `Components/Layout/CommandPalette.razor` | `.scrim .palette .q-in .pg .pi .pfoot` | s5 | 3 |
| `Components/Pages/NewSession.razor` `/` | `.start .start-card .hello .field .input .recents .modes .mode .toggle .hint` | s1 | 3 |
| `Components/Pages/Overview.razor` `/overview` | `.ov .ovhead .tot table .stt .mtag .queue .q` | s4 | 3 |
| `Components/Pages/SessionPage.razor` `/session/{Id}` | `.pane` + `.pane.insp` | s2 s3 s5 s6 s9 s10 | 1 |
| `Components/Session/SessionHeader.razor` | `.ph .ttl .sub .end` | s2… | 1 |
| `Components/Session/Thread.razor` | `.thread`, `.you`, `.ultra-chip`, `.caret` | s2 s3 s5 s6 s10 | 1 |
| `Components/Session/Ledger.razor` | `.ledger .lh` | s2 s3 | 1 |
| `Components/Session/ToolRow.razor` | `.t(.sel/.wait/.void) .kind .p .m .badge` | s2 s3 | 1 |
| `Components/Session/Workflow.razor` | `.wf .wh .agent .subrun .sl` (without `.phases`) | s10 | 1 |
| `Components/Session/StatusLine.razor` | `.status(.wait) .spin` | s2 s3 s10 | 1 |
| `Components/Session/CrashBanner.razor` | `.crash` (`h4`) | s5 | 1 |
| `Components/Shared/Markdown.razor` + `Core/Md.cs` | `.prose.md .cb` | s6 | 1 |
| `Components/Session/Composer.razor` | `.composer.rich .box .tools-row .chip .pick .effort .go .meta` | s2 s3 s5 s9 s10 | 2 |
| `Components/Session/SlashPopover.razor` | `.pop.slash .sh .si` | s9 | 2 |
| `Components/Session/ModelMenu.razor` | `.pop` + `.si` (not in the mockup, styles reused) | s9 | 2 |
| `Components/Session/Inspector.razor` | `.pane.insp` (switch) | s2 s3 s9 s10 | 2 |
| `Components/Session/ToolDetail.razor` | `.tabs .tab .cmdlog .errbox .kv` | s2 s6 | 2 |
| `Components/Session/PermissionCard.razor` | `.section .acts .cmd` + "Also waiting" | s3 | 2 |
| `Components/Session/ContextPanel.razor` | `.ctx .ctxbar .legend` | s9 | 2 |
| `Components/Session/AgentPanel.razor` | `.kv` + `.prose.md` (without `.finding`) | s10 | 2 |
| `Components/Shared/DiffView.razor` + `Core/EditDiff.cs` | `.diff .fh .hunk .l .add .del .g .lc` | s3 s2 | 2 |
| `Components/Shared/FileView.razor` | `.filev .lc` | s2 | 2 |
| `Components/Pages/Worktrees.razor` | `.wt .wt-head .filters .wtt .grp-row .state .st-* .why .rowacts .plan .it .cmdlog .guard` | s8 | 4 |
| `Components/Pages/Extensions.razor` `/extensions` | `.ext .subtabs .filters .mcp .ms .errbox .toggle` | s11 | 5 |
| `Components/Pages/Appearance.razor` `/settings/appearance` | `.settings .set-row .themes .tc .pv .seg .sample` | s7 | 5 |

`Components/Pages/Home.razor` is deleted (WP1, once the session is ported). `Error.razor` and `NotFound.razor` stay.

The markup, French texts and display rules of each component follow the **mockups report §2–§3**, which serves as an annex specification. This plan corrects the following points:
- partial streaming and `.caret` are in scope;
- `ToolKinds`: `PowerShell` treated as `Bash` (colour, `pre.cmd`, the verb "Run this command?", "a command" in `.status.wait`), `Agent` as `Task`;
- s1: the branch shown under the worktree toggle is `worktree-<name>` (what `-w` creates), not `claude/<slug>`;
- a diff **after the fact** (inspector, Diff tab) is built from `tool_use_result.structuredPatch` (verified); `EditDiff` only computes the **pre-authorization** diff (permission card) and the replay fallback;
- effort, model, fast mode and ultracode go through the verified requests (§2.5), not through the "ultracode" prefix in the prompt;
- fast mode is disabled only if `fast_mode_state != "on"` or if the model lacks `supportsFastMode`. The tooltip gives the reason in French: `sdk_opt_in_required` → "opt-in SDK requis" (SDK opt-in required), `extra_usage_disabled` → "usage supplémentaire désactivé" (extra usage disabled), otherwise the raw code;
- effort only shows the `supportedEffortLevels` of the current model and disappears if `!supportsEffort`;
- Extensions uses `mcp_status` for the error, transport and tools;
- for the quota, `rate_limit_event` gives `utilization` between 0 and 1, whereas `get_usage` gives a value between 0 and 100. Normalize.

---

## 5. Work packages

### 5.0 Shared contracts (created by WP0, frozen afterwards)

All the other WPs code against these contracts. Any change goes through the integrator.

1. **C#**: all of §2. Frozen: `Events.cs`, `LiveSession` (complete), `SessionManager`, `UiState`, `Fmt`, `ToolKinds`, the **signatures** of `TranscriptStore`, `WorktreeService`, `EditDiff` and `Md` (bodies: minimal implementation returning empty, **not** `NotImplementedException`, so the app runs while the WPs progress), and `SelfCheck.Run()`. Utility signatures:
   - `Fmt`: `Dur(TimeSpan)` → `2,4 s` / `2 min 14`, `Size(long)` → `1,34 Go`, `Pct(double)` → `32 %`, `Tokens(long)` → `212k`, `Cost(decimal, int decimals)` → `$0.0391`, `Time(DateTimeOffset)` → `14:31`;
   - `ToolKinds`: `Color(name)`, `Target(JsonElement input, string cwd)`, `IsShell(name)`, `IsAgent(name)`, `IsEdit(name)`;
   - `EditDiff`: `IReadOnlyList<DiffLine> FromInput(string tool, JsonElement input)`, `FromPatch(JsonElement structuredPatch)`, `Lang(string path)`;
   - `Md`: `string Render(string markdown)`. `SelfCheck.Run()` calls `Events.Check()`, `LiveSession.Check()`, `Md.Check()`, `EditDiff.Check()` and `WorktreeService.Check()`; each `Check` lives in the file of its logic and stays empty until the owning WP has written it.
2. **Parameters of the cross-cutting components**, created as compilable *stubs* by WP0 (minimal markup):
   - `Icon(Name, Sm, Class)`
   - `Markdown(Text)`
   - `DiffView(Path, IReadOnlyList<DiffLine> Lines)`, with `record DiffLine(DiffKind Kind, int? N, string Text)`, `enum DiffKind{Ctx,Add,Del,Hunk}`
   - `FileView(Path, string ReadResultText)`
   - `Composer(LiveSession Session)`
   - `Inspector(LiveSession Session)`
   - `Thread(LiveSession Session)`
   - `SessionHeader(LiveSession Session)`
   - `CrashBanner(LiveSession Session)`
   - `Mascot()`, `Rail()`, `CommandPalette()`, all pages with their `@page`.
3. **Same conventions everywhere**:
   - subscribe to `Changed` in `OnInitialized`, unsubscribe in `Dispose`, refresh through `InvokeAsync(StateHasChanged)`;
   - formats only via `Fmt`;
   - tool colours via `ToolKinds.Color`.
4. **CSS**: class names are those of the mockup ("Mockup classes" column of §4). No WP invents a class if the mockup has one, and no WP1–5 modifies `app.css` (§3).
5. **File ownership**: WP0 creates all files (stubs included); after that each file has **a single** owner, the one in the "Files" list of its WP. A WP0 stub is no longer touched by WP0.

### WP0: foundations (sequential, before everything else)

**Files:**
- `ClaudeCodeUI.csproj`, `Program.cs`, `ClaudeSession.cs`
- `Core/Events.cs`, `Core/LiveSession.cs`, `Core/SessionManager.cs`, `Core/UiState.cs`, `Core/Fmt.cs`, `Core/ToolKinds.cs`, `Core/SelfCheck.cs`
- the stubs `Core/TranscriptStore.cs`, `Core/WorktreeService.cs`, `Core/EditDiff.cs`, `Core/Md.cs` (port of the current pipeline)
- `wwwroot/app.css`, `wwwroot/app.js`, `wwwroot/fonts/*`
- `Components/App.razor`, `Components/Routes.razor`, `Components/_Imports.razor`
- `Layout/MainLayout.razor` (full shortcuts, §2.9), `Layout/IconSprite.razor`, `Shared/Icon.razor`
- the stubs of all components and pages of §4
- deletion of `MainLayout.razor.css`
- `.gitignore`: add `.claude/worktrees/` (otherwise the `-w` worktrees and those of the WPs become gitlinks on the first `git add .`)

**Acceptance criteria:**
- `dotnet build`: 0 errors, no new warnings.
- Ctrl K / Ctrl I / Alt N / Ctrl ⇧ O / Ctrl ⇧ A act (stub palette and inspector are enough).
- `dotnet run -- --self-check` passes. `Events.Check()` parses an example of each JSON line type from the protocol report (init, text_delta, tool_use, tool_result with `tool_use_result`, can_use_tool, result success + interrupted, rate_limit_event, task_started/notification) and checks the key fields. `LiveSession.Check()` replays a sequence (send → tool_use → permission → deny → result) and checks the status, the `Denied` `ToolItem`, the assigned (not added) cost and an empty `Pending`.
- `dotnet run`: the `.app` shell is displayed with the graphite theme and Geist fonts. Setting `localStorage['claude-ui.theme']='mousse'` then reloading recolours the whole interface without a flash.
- `SessionManager.Start` called by the `NewSession` stub (current folder, `haiku`) receives the `initialize` response: `InitializeInfo` filled, number of models and commands written to the log. No test button to remove.
- `Home.razor` still exists but has lost its `@page` (route taken over by `NewSession`).

### WP1: session thread

**Files:**
- `Pages/SessionPage.razor`
- `Session/SessionHeader.razor`, `Session/Thread.razor`, `Session/Ledger.razor`, `Session/ToolRow.razor`, `Session/Workflow.razor`, `Session/StatusLine.razor`, `Session/CrashBanner.razor`
- `Shared/Markdown.razor`, `Core/Md.cs` (+ `Md.Check`)
- deletion of `Pages/Home.razor` and `Home.razor.css`

**Acceptance criteria:**
- With a real prompt on `--model haiku`:
  - the text appears live with `.caret`, then is replaced by the complete block;
  - consecutive tools form a `.ledger`, the last one expanded and the older ones collapsed;
  - durations and counters in `Fmt` format (`2,4 s`).
- Clicking a `.t` sets `UiState.SelectedToolId` and `.t.sel`. Esc interrupts: "interrupted" status, no crash.
- A sub-agent ("launch an agent that answers pong") produces a `.wf` with an `.agent` (tokens from `task_progress`, cost `—`) and its sub-tools expandable in `.sl`.
- Killing the `claude` process by hand shows `.crash` with the KO duck, the exit text, "Copy error" (`data-copy`) and "Restart session". Restarting resumes the session (`--resume`) and the model remembers the context.
- A past session opened from the URL shows its replayed history.
- `Md.Check` covers `` ```csharp `` → `.cb` with the "C#" header, a block without language → "texte" (plain text), and `[!WARNING]` → "Attention" (Warning).
- Rendering matches `screens/02.png`, `06.png` and `10.png` (without `.phases` or "finding" cards), data aside.

### WP2: composer and inspector

**Files:**
- `Session/Composer.razor`, `Session/SlashPopover.razor`, `Session/ModelMenu.razor`
- `Session/Inspector.razor`, `Session/ToolDetail.razor`, `Session/PermissionCard.razor`, `Session/ContextPanel.razor`, `Session/AgentPanel.razor`
- `Shared/DiffView.razor`, `Shared/FileView.razor`, `Core/EditDiff.cs` (+ `EditDiff.Check`)

**Acceptance criteria:**
- Composer:
  - Ctrl ⏎ sends;
  - ↑ / ↓ recall previous prompts (`PromptHistory`, `claudeUi.history`): caret on the first / last line, no popover open; this session's prompts, then `TranscriptStore.Prompts` of the same cwd (verified by `--probe-cli prompt-history`); the draft comes back after the newest entry, and is put back in `LiveSession.Draft` when the Composer goes away mid-walk;
  - placeholders and main button follow the state (table in mockups report §2.5);
  - the meta shows turn / session / tools / context, with the **cumulative** cost.
- Model: the menu lists `initialize.models` (`displayName`); a choice calls `set_model`, and the next turn reports the new model in `system/init`.
- Effort: a click calls `apply_flag_settings {effortLevel}`, then `get_settings.applied.effort` reflects the value, with `.on` on the right button.
- Fast: disabled with the reason in a tooltip when `fast_mode_state=="off"`.
- Ultracode: a turn with the toggle active sends `{ultracode:true}` (read in `get_settings.applied.ultracode`), then `{ultracode:false}` after the `result`. A real (costly, default model) trial notes in the PR whether sub-agents appear; otherwise the toggle stays, the tooltip saying it enables the CLI setting.
- Typing `/` opens a filtered `.pop.slash`, with descriptions and a `built-in` label or plugin name. ↑ / ↓ / Tab work. Sending `/cost` shows the synthetic output in the thread.
- Permission:
  - Edit shows a `DiffView` (`EditDiff.FromInput`) whose `+`/`−` lines are those of the `structuredPatch` received afterwards;
  - PowerShell/Bash shows `pre.cmd`;
  - Allow, Deny (FR message, `.t.void` "refusé" row, i.e. "denied") and "Whole session" via `setMode` work, by click and by keyboard (⏎, Shift ⏎, Del);
  - "1 of 2" and "Also waiting" reflect `SessionManager.DecisionQueue`.
- The `updatedPermissions` case is tested once and the result noted in the PR. If it fails, the button is hidden for those suggestions.
- `ToolDetail`: Diff or Output tab depending on the tool, Input and raw JSON. Read is rendered as a `FileView` coloured by hljs.
- `ContextPanel`: shows `get_context_usage` (bar + legend). "Compact now" is tested; if it fails, it is disabled.
- `AgentPanel`: shows the agent's last text and the `.kv` type / tools / tokens / cost `—`.
- `EditDiff.Check` covers a one-line replacement in the middle, a pure addition, Write on a missing file, MultiEdit with two hunks and `FromPatch` on the `structuredPatch` from the protocol report.
- Rendering matches `screens/03.png` and `09.png`.

### WP3: navigation, start, overview, history

**Files:**
- `Layout/Rail.razor`, `Layout/RailSessionRow.razor`, `Layout/QuotaMeter.razor`, `Layout/Mascot.razor`, `Layout/CommandPalette.razor`
- `Pages/NewSession.razor`, `Pages/Overview.razor`
- `Core/TranscriptStore.cs`

**Acceptance criteria:**
- s1:
  - recent folders come from `TranscriptStore.Recent`;
  - a non-existent folder shows "This folder does not exist.";
  - the worktree toggle offers a slugified, editable name, and passes `-w`;
  - the 4 modes are present (`default` sent as `manual`);
  - "Start" creates the session, sends the prompt and navigates to `/session/{id}`.
- Rail:
  - "Active": live sessions in creation order. "Recent": `TranscriptStore.Recent` minus live ids;
  - dots and right-hand suffixes according to status;
  - "to clean up" counters (`WorktreeService.CleanableCount`, hidden if `null` or 0) and failing MCP (`Init.Mcp` of the most recent live session) hidden at 0;
  - the quota appears only after a `rate_limit_event` or `get_usage`;
  - the duck's mood follows the priority table of mockups report §2.2.
- Ctrl K palette:
  - case-insensitive filter with encoded `<mark>`;
  - ↑ / ↓ / ⏎ / Esc;
  - actions New session, Overview, Worktrees, Extensions, Appearance;
  - "Theme: X" switches to the next theme via `claudeUi.setTheme`.
- s4: totals, stable table, decision queue hidden when empty, "today" = sum of the costs of sessions started today.
- Rendering matches `screens/01.png`, `04.png` and `05.png`.

### WP4: worktrees

**Files:** `Core/WorktreeService.cs` (+ `WorktreeService.Check`), `Pages/Worktrees.razor`.

**Acceptance criteria:**
- `WorktreeService.Check` covers `Classify` on at least these cases:
  - active session in the app → Active;
  - lock with live pid → Active;
  - missing folder → Orphan;
  - dirty → To check;
  - unpushed and unmerged commits → To check;
  - lock with dead pid but everything clean and merged → To check, plan with `unlock`;
  - merged and clean → Safe;
  - squash → Safe, branch kept;
  - fresh → Safe;
  - unknown → To check.
- Against a throwaway repo with one worktree per state (the one in the research scratchpad, `scratchpad\wt\repo`, if it still exists; otherwise recreate it with a small script in the WP's scratchpad), each row is classified as in worktrees report §3.
- The s8 page shows the groups, the filters and the size computed in the background (`…` while waiting). The "Cleanup" inspector shows the plan and the exact commands before confirmation. The per-row actions follow §2.8.
- Execution streams each command and its result into `.cmdlog`. `WorktreeService.Check` verifies that no `Plan` step (all cases combined) contains `--force`, `-f` or `-D` (a `Debug.Assert` would not run in Release).
- Warning if `.claude/worktrees/` is not ignored by git.
- Rendering matches `screens/08.png`.

### WP5: extensions and appearance

**Files:** `Pages/Extensions.razor`, `Pages/Appearance.razor`.

**Acceptance criteria:**
- s11:
  - source = the most recent live session, via `RefreshMcp()` (`mcp_status`); without a session, the empty state "Start a session to see what the CLI loads.";
  - MCP / Skills / Agents / Plugins tabs with their counters, filters by state;
  - the inspector of a failing server shows `.errbox` with `error`;
  - `mcp_toggle` and `mcp_reconnect` tested once on a real server; control disabled on failure;
  - "Sign in" via `mcp_authenticate` (`--probe-cli mcp-auth`), except claude.ai connectors: `/mcp` in a terminal.
- s7:
  - 5 `.tc[data-theme]` cards whose preview follows their own tokens;
  - a click applies the theme immediately and persists it;
  - the code size (12 to 15) is persisted;
  - the preview goes through `Md.Render`;
  - the initial state is read by `OnAfterRenderAsync(firstRender)`.
- Rendering matches `screens/07.png` and `11.png`.

### Order and integration

```
WP0 ──► { WP1, WP2, WP3, WP4, WP5 } in parallel ──► integration (lead)
```

- No file belongs to two WPs (§5.0 point 5), `app.css` included (WP0 alone).
- Each WP works in **its own git worktree** created from the WP0 commit (`git worktree add ../ClaudeCodeUI-wpN`): a single shared folder would make the `dotnet build`s fight over `obj/` and the `dotnet run`s over the port. Each WP runs the app on its own port (`--urls http://localhost:51N0`).
- A WP that needs a contract change reports it instead of modifying a file it does not own.
- Integration:
  1. build;
  2. self-check;
  3. full walkthrough s1→s11;
  4. screenshot pass (§6);
  5. removal of the remaining stubs.

---

## 6. Verification

1. **Build**: `dotnet build` with 0 errors and no new warnings; `<TreatWarningsAsErrors>true</TreatWarningsAsErrors>` can be added at integration if it is already clean.
2. **Executable check**: `dotnet run -- --self-check` runs the `Check()`s described above (events, reducer, Md, EditDiff, worktree classification). It exits with a non-zero code and a clear message on the first failure. No xunit project: a `tests/` folder under the root would be compiled into the web project (default `**/*.cs` glob), and a single entry point is enough.
3. **Manual walkthrough** with `dotnet run` and `--model haiku` in a throwaway repo in the scratchpad: new session with worktree, Edit allowed, Write denied, interruption, sub-agent, `/cost`, forced crash, restart, resume from "Recent", cleanup of the created worktree.
4. **Screenshots**: headless browser (MCP chrome-devtools or `msedge --headless --screenshot --window-size=1440,900`). Compare page by page with `docs/mockups/screens/NN.png`, data aside: structure, spacing, colours, fonts. Run the 5 themes at least on s2. Accepted differences are listed in the PR.
5. **No security regression**: Markdig keeps `DisableHtml()`, and any text inserted into a `MarkupString` (palette, `Md`) is encoded with `HtmlEncoder`.

---

## 7. Risks and open questions

| Risk | Mitigation |
|---|---|
| WP0 is big and blocks everything | it only delivers contracts, reducer, shell and CSS; the stubs allow starting the WPs as soon as it compiles |
| Partial-stream throughput (one notification per delta) | 50 ms throttling in `LiveSession`; Markdown rendered once per complete block, plain text during the stream |
| Concurrent reading of the lists during rendering | copy-on-write `ImmutableList`, a single writer per session |
| Diff replay from the `.jsonl` not tested for real | `EditDiff` recomputes from the tool input (§1.2) |
| A CLI version that would change the behaviour of ultracode, `updatedPermissions`, `mcp_toggle`/`mcp_reconnect`, `/compact`, `@path` expansion or the message queue | rerun `dotnet run -- --probe-cli`; on a FAIL, control disabled (§1.2) |
| Fast mode almost always unavailable on this account (`extra_usage_disabled`) | shown disabled with the reason, which matches the mockup |
| Live sessions die on server restart | they reappear in "Recent" and resume via `--resume`; no dedicated persistence |
| `-w` worktrees locked by pid, then stale lock | classified "To check", explicit `unlock` in the plan, never `--force` |
| Slow scan of `~/.claude/projects` (865 folders) | last 30 days, 64 KB per file, cache, rescan on "Refresh" |
| Ctrl N intercepted by the browser | Alt N in addition, label kept |
| User hooks that produce noise (`hook_*`, `stop-hook-error`) | `hook_started` ignored; a `hook_response` that succeeds silently, a successful `SessionStart` or `SubagentStart`, and any successful output that is only `hookSpecificOutput.additionalContext` (prompt context, for the model) are dropped by the reducer (`LiveSession.Shown`); the others fold into one collapsed row per run of hooks. A hook row never splits a tool Ledger or an agent Workflow (`ThreadBlocks`); verified by `--probe-cli hooks` (`hooks-subagent`: SubagentStart streams between the Agent tool_use and its result, PostToolBatch after it) |
| `bypassPermissions` / `dontAsk` exist in the CLI | deliberately not exposed in the interface |

**Default decisions (changeable):**
1. Ultracode applies "for this turn", as in the mockup.
2. The `auto` mode is offered in s1, with the note "use with caution".
3. The Geist fonts (OFL licence, ~100 KB) are self-hosted in the repository.
4. Without a worktree, a session defaults to the slug of the first 4 words of the prompt.
