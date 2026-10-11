using System.Collections.Immutable;
using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

public enum SessionStatus { Starting, Idle, Running, Waiting, Exited, Crashed }
public enum ToolState { Running, Done, Error, Waiting, Denied, NotRun }   // NotRun: held back by auto mode (Events.NotRun)
public enum Decision { Allow, AllowSession, Deny }

public abstract record Item;
public record UserItem(string Text, DateTimeOffset At, bool Ultracode, IReadOnlyList<UserImage>? Images = null, string? Uuid = null) : Item;   // Uuid: sent with the message (Send)
public record TextItem(string Markdown, string? ParentToolUseId, string? Uuid = null, DateTimeOffset? At = null) : Item;   // Uuid: where "Fork from here" cuts
public record ThoughtItem(string Text) : Item;   // a thinking summary (AppFlag.Thinking)
// A compaction: "Compacted chat", its token counts and the summary the conversation goes on from (set when it arrives).
public record CompactItem(string Trigger, long PreTokens, long PostTokens, int DurationMs, string? Summary = null) : Item;
public record FallbackItem(string Text, string? Retry) : Item;   // a model fallback notice; Retry: the refused message's uuid
public record ApiErrorItem(ApiError Error) : Item;
public record ResetItem : Item;                                   // /clear went through: not rendered, it starts a new task list
public record ResumedItem : Item;                                 // the turn a crash cut off, re-run by the relaunched CLI
public record HookItem(HookEvt Hook) : Item;   // a hook that failed, blocked or printed something
public record NoticeItem(NoticeEvt Notice) : Item;   // a CLI notification or plugin toast
// Immutable like every item: an update replaces the instance in Items (LiveSession.Set), so a render never sees half of it.
public sealed record ToolItem(string Id, string Name, JsonElement Input, string? ParentToolUseId) : Item
{
    public ToolState State { get; init; }
    public string? ResultText { get; init; }
    public JsonElement? Structured { get; init; }
    public DateTimeOffset StartedAt { get; init; }
    public DateTimeOffset? EndedAt { get; init; }
    public string? TaskId { get; init; }          // Agent and Workflow
    public string? TaskStatus { get; init; }      // its task-notification status (completed, …)
    public JsonElement? Progress { get; init; }   // Workflow: the last task_progress.workflow_progress snapshot
    public long Tokens { get; init; }
    public int SubToolUses { get; init; }
    public bool Background { get; init; }         // Agent launched async: outlives its turn, ends on its task-notification
}
public record QueuedMessage(string Uuid, string Text, DateTimeOffset At);   // written to stdin mid-turn, not started yet
// rewind_files {dry_run:true}: what rewinding the files to a message would change, or why it can't (CLI text).
public record RewindPreview(bool CanRewind, IReadOnlyList<string> Files, int Insertions, int Deletions, string? Error)
{
    public static RewindPreview Parse(JsonElement r) => new(
        Events.Prop(r, "canRewind") is { ValueKind: JsonValueKind.True },
        Events.Prop(r, "filesChanged") is { ValueKind: JsonValueKind.Array } f ? [.. f.EnumerateArray().Select(x => x.ValueKind == JsonValueKind.String ? x.GetString() : null).OfType<string>().Where(x => x.Length > 0)] : [],
        Events.Prop(r, "insertions") is { ValueKind: JsonValueKind.Number } i ? i.GetInt32() : 0,
        Events.Prop(r, "deletions") is { ValueKind: JsonValueKind.Number } d ? d.GetInt32() : 0,
        Events.Str(r, "error"));

    // Something to put back on disk. A hosted or persistent transport drops filesChanged from the answer (CLI source,
    // rewind_files handler) while the line counts stay: count those too.
    public bool Changes => CanRewind && (Files.Count > 0 || Insertions + Deletions > 0);
}
// A background shell or Monitor command (task_type local_bash): outlives its turn, ends on task_updated or with the process.
public sealed record BgTask(string Id, string ToolUseId, string Description, DateTimeOffset StartedAt)
{
    public string Status { get; init; } = "running";   // task_updated patch.status: completed, failed, killed…
    public DateTimeOffset? EndedAt { get; init; }
    public bool Running => Status is "running" or "pending" or "paused";

    // The last lines of a get_task_output tail as plain text: no ANSI escapes, a \r-overwritten line keeps its last state.
    public static string Tail(string output, int lines)
    {
        var plain = System.Text.RegularExpressions.Regex.Replace(output, PermissionRules.Ansi, "");
        var all = plain.Split('\n').Select(l => l.TrimEnd('\r')).Select(l => l[(l.LastIndexOf('\r') + 1)..]).ToList();
        while (all.Count > 0 && all[^1].Length == 0) all.RemoveAt(all.Count - 1);
        return string.Join('\n', all.TakeLast(lines));
    }
}
public record PendingPermission(string RequestId, string Tool, JsonElement Input, string? Description, string? ToolUseId,
                                JsonElement? Suggestions, DateTimeOffset At = default, PermissionInfo? Info = null);

// One claude process and its state. The reducer (Apply) runs on the process reader thread; lists and their items are
// immutable snapshots replaced on write, so components read them without locking.
public sealed class LiveSession : IAsyncDisposable
{
    readonly Lock gate = new();
    readonly HashSet<string> answering = [];   // request ids whose reply is in flight
    ClaudeSession? proc;
    bool stopped;   // set by DisposeAsync (SessionManager.Stop): EnsureProcess refuses to spawn again
    readonly bool resumable; // opened from a transcript: always --resume
    bool turnUltra;
    bool crashedMidTurn;                                       // the process died during a turn: Restart re-runs it
    bool resumeTurn;                                           // ...and the relaunched CLI has not started it yet
    public bool CrashedMidTurn => crashedMidTurn;
    bool draining;                                             // a result came with messages still queued: the CLI runs the next one
    bool ultraCarry;                                           // ...and that result ended an ultracode turn
    ImmutableHashSet<string> sent = [];                        // UserItem.Uuid the current process knows (rewind targets)
    long lastNotify;
    int notifyQueued;
    int version;

    // Modes the UI offers. bypassPermissions / dontAsk are deliberately not offered.
    public static readonly string[] Modes = ["default", "acceptEdits", "plan", "auto"];
    // Modes that change files without asking: confirmed when the folder is not an isolated worktree. An --add-dir folder
    // is never isolated (acceptEdits writes there with no can_use_tool), so any extra folder makes the session unisolated.
    public static bool Risky(string mode) => mode is "auto" or "acceptEdits";
    public bool Isolated => (Worktree is not null || TranscriptStore.RootOf(Cwd) != Cwd) && AddDirs.Count == 0;

    public LiveSession(string id, string name, string cwd, string mode, string? worktree = null, string? model = null, string? effort = null,
                       bool resumable = false, IEnumerable<string>? addDirs = null)
    {
        Id = id; Name = name; Cwd = cwd; Mode = mode; Worktree = worktree; Model = model; Effort = effort;
        AddDirs = [.. addDirs ?? []];
        this.resumable = resumable;
        StartedAt = LastEventAt = DateTimeOffset.Now;
    }

    public string Id { get; }                        // == --session-id == .jsonl name == /session/{Id}
    public string Name { get; private set; }
    public string Cwd { get; private set; }          // replaced by init.cwd (-w)
    public string Mode { get; private set; }
    public string? Branch { get; private set; }
    public string? Repo { get; private set; }
    public string? Worktree { get; private set; }   // -w name; dropped when ExitWorktree leaves it
    public ImmutableList<string> AddDirs { get; private set; }   // --add-dir folders: Claude works there as in Cwd
    public SessionStatus Status { get; private set; } = SessionStatus.Exited;
    public ImmutableList<Item> Items { get; internal set; } = [];
    public ImmutableList<PendingPermission> Pending { get; private set; } = [];
    public ImmutableList<QueuedMessage> Queued { get; private set; } = [];
    // Appended per text_delta under gate; the string is only materialized when read (once per render, not per delta).
    readonly System.Text.StringBuilder stream = new();
    string? streamText = "";
    public string StreamingText { get { lock (gate) return streamText ??= stream.ToString(); } }
    void ClearStream() { stream.Clear(); streamText = ""; }
    public int ThinkingTokens { get; private set; }
    public decimal CostUsd { get; internal set; }
    public decimal LastTurnCostUsd { get; private set; }
    decimal costBase;                                          // cost of earlier processes, minus what --resume restores
    public decimal CostAtOpen { get; init; }                   // lifetime cost already in the transcript when opened: not spent in this run
    public int ToolCount { get; internal set; }
    public long ContextTokens { get; private set; }
    public long? ContextWindow { get; private set; }
    public DateTimeOffset StartedAt { get; init; }
    public DateTimeOffset LastEventAt { get; internal set; }
    public DateTimeOffset? TurnStartedAt { get; private set; }
    public TimeSpan? LastTurn { get; private set; }
    public const string Interrupted = "interrupted";   // protocol sentinel, never shown: display text lives in the resx
    public string? LastResultSubtype { get; private set; }   // Interrupted after an interrupt
    public DateTimeOffset? LastResultAt { get; private set; }
    public int? ExitCode { get; private set; }
    public string? ExitText { get; private set; }
    public bool StartFailed { get; private set; }   // the claude process never spawned: no turn ran
    public string ExitLabel => StartFailed ? Strings.Get("Session.NotStarted") : Strings.Get("Session.ExitShort", ExitCode);
    public InitEvt? Init { get; private set; }
    public JsonElement? InitializeInfo { get; internal set; }  // "initialize" response: models, commands, agents, account
    public string? Model { get; private set; }
    public string? Effort { get; private set; }
    public bool Ultracode { get; private set; }               // toggle "for this turn"
    public string? FallbackModel { get; init; }               // --fallback-model: the CLI switches to it when the model is overloaded
    public bool Restricted { get; init; }                     // --restricted: no Bash/code-running tools nor WebFetch, user/project/local settings ignored
    public bool SafeMode { get; init; }                       // --safe-mode: CLAUDE.md, skills, plugins, hooks, MCP… off, to troubleshoot a configuration
    public string? PrUrl { get; internal set; }               // the PR the session is linked to: its transcript's pr-link, then code_change_published
    public string? RemoteUrl { get; private set; }            // the claude.ai/code page while Remote Control is on (this process only)
    public IReadOnlyList<StatusSection>? StatusRows { get; private set; }   // get_status, asked when the panel shows
    // initialize.remote_control_available: false where Remote Control can never work (policy, 3P provider); absent = available.
    public bool RemoteAvailable => InitializeInfo is { } ri && Events.Prop(ri, "remote_control_available") is not { ValueKind: JsonValueKind.False };
    public bool Thinking { get; private set; } = true;        // false: set_max_thinking_tokens 0, live and again at each launch
    public string? Advisor { get; private set; }              // get_settings applied.advisor: the model actually attached, or null
    public string? OutputStyle { get; private set; }          // initialize / init output_style (default, Concise…)
    public JsonElement? MemoryDialog { get; private set; }
    public string? StatusText { get; private set; }           // what the settings' statusLine command printed (StatusCommand)
    string? statusCommand;
    int statusRunning;    // raw get_memory_dialog: instruction files, auto-memory folder and memories
    // Hooks announced by hook_started and not answered yet (hook_id → name, start), shown while they hold the turn.
    public ImmutableDictionary<string, (string Name, DateTimeOffset At)> RunningHooks { get; private set; } = ImmutableDictionary<string, (string, DateTimeOffset)>.Empty;
    public string FastModeState { get; private set; } = "off";
    public string? FastModeReason { get; private set; }
    public JsonElement? Context { get; private set; }          // raw get_context_usage
    public JsonElement? McpStatus { get; private set; }       // raw mcp_status
    public ImmutableDictionary<string, string> PluginStatus { get; private set; } = ImmutableDictionary<string, string>.Empty;   // plugin → its ui_status line
    public ImmutableList<BgTask> Tasks { get; private set; } = [];   // background shells and Monitors, in start order
    public RateLimitEvt? Limits { get; private set; }
    public Usage? Usage { get; private set; }                  // get_usage past the two windows; Week only after RefreshUsage(true)
    public long CacheRead { get; private set; }                // the last API call's prompt read from the cache, and written to it
    public long CacheWrite { get; private set; }
    public RetryEvt? Retry { get; private set; }               // the API call being retried; cleared by the next sign of progress
    public DateTimeOffset? CompactingSince { get; private set; }   // status "compacting" (manual or automatic), until the next status
    public ImmutableList<Artifact> Artifacts { get; private set; } = [];   // the session's published artifacts (session_metadata)
    public int Pushes { get; private set; }                    // PushNotification calls so far; MainLayout notifies on a new one
    public string? LastPush { get; private set; }              // and the last one's message
    public const string PushTool = "PushNotification";
    public DateTimeOffset LimitsAt { get; private set; }
    public bool HasProcess => proc is not null;
    public string Draft { get; set; } = "";          // the Composer's unsent text: survives navigation, reloads and reconnects
    public string? Suggestion { get; private set; }   // the CLI's predicted next prompt (prompt_suggestion), until something is sent
    public IReadOnlyList<UserImage> DraftImages { get; set; } = [];   // and its attached images, likewise
    public MemoryEdit Memory { get; } = new();       // the memory panel's open file and unsaved edit, same reason
    public List<TodoEntry> Todos => TodoList.From(Items);   // recomputed per read: one pass over Items, cheaper than keeping a cache in sync
    public string? ForkOf { get; init; }   // forked from this session id: --resume ForkOf --fork-session until it has its own transcript
    public string? ForkAt { get; init; }   // --resume-session-at: the message uuid the fork stops after (null = the whole conversation)
    // The CLI forks the persisted transcript: none before the first message, and only part of a turn still running.
    // A fork not sent to yet has items but no transcript: SessionManager.Fork then forks its source instead.
    public bool CanFork => Items.Count > 0 && Status is not (SessionStatus.Running or SessionStatus.Waiting);

    public event Action? Changed;

    // ---------- commands ----------

    // Returns the message's uuid (SendNow presses a queued one by it).
    public async Task<string> Send(string text, IReadOnlyList<UserImage>? images = null)
    {
        var p = EnsureProcess();
        var uuid = Guid.NewGuid().ToString();
        Suggestion = null;
        // During a turn the CLI queues the message: it folds it into the turn at the next tool result, or runs it as the
        // next turn (also after an interrupt). It joins the thread on command_lifecycle `started` (--probe-cli queue).
        // A queued message never arms ultracode (the flag would land on the running turn; the toggle stays armed for the next
        // send). It runs under whatever flag the CLI holds when it starts: see the `started` case.
        bool queued;
        lock (gate)
        {
            queued = Status is SessionStatus.Running or SessionStatus.Waiting;
            if (queued) Queued = Queued.Add(new(uuid, text, DateTimeOffset.Now));
            sent = sent.Add(uuid);
        }
        if (!queued)
        {
            if (Ultracode) await p.Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["ultracode"] = true } });
            lock (gate) { turnUltra = Ultracode; BeginTurn(text, images, uuid); }
        }
        Notify();
        try { await p.SendUser(text, images, uuid); }
        catch when (queued)
        {
            // Never written: drop the chip, the Composer puts the text back in the box (never lose a prompt, no duplicate).
            lock (gate) Queued = Queued.RemoveAll(m => m.Uuid == uuid);
            Notify();
            throw;
        }
        return uuid;
    }

    // The CLI's "send now": interrupt {send_now, message_uuid} stops the turn and starts that queued message at once
    // (answers send_now:"stopped"; the message then starts on command_lifecycle, verified on claude 2.1.296).
    public Task SendNow(string uuid) => Request("interrupt", new() { ["send_now"] = true, ["message_uuid"] = uuid });

    // "Rewind to here": only between turns with no background agent still writing files, and only to a message this very
    // process received (its uuid dies with it).
    public bool CanRewind(UserItem u) => u.Uuid is { } id && sent.Contains(id) && proc is not null && Status == SessionStatus.Idle
                                         && !Items.Any(i => i is ToolItem { Background: true, State: ToolState.Running });

    public async Task<RewindPreview> PreviewRewind(UserItem u) =>
        RewindPreview.Parse(await Request("rewind_files", new() { ["user_message_id"] = u.Uuid, ["dry_run"] = true }));

    // The conversation first: rewind_conversation only takes the latest message ("stale target" otherwise), so it goes
    // back one message at a time from the newest to the target; a refusal leaves the files untouched and the thread cut
    // where the CLI's is. Then the files (rewind_files still answers after rewind_conversation). Both verified by
    // --probe-cli rewind-files / rewind-conversation. The message's text goes back to the Composer.
    public async Task Rewind(UserItem u, bool files)
    {
        if (!CanRewind(u)) throw new ClaudeRequestException(Strings.Get("Rewind.Unavailable"));
        string? cut = null, prefill = null;
        var steps = RewindSteps(u.Uuid!);
        try
        {
            foreach (var id in steps)
            {
                var r = await Request("rewind_conversation", new() { ["target_message_uuid"] = id });
                if (Events.Prop(r, "rewound") is not { ValueKind: JsonValueKind.True })
                    throw new ClaudeRequestException(Strings.Get("Rewind.Refused", Events.Str(r, "error") ?? Events.Str(r, "reason") ?? r.GetRawText()));
                (cut, prefill) = (id, Events.Str(r, "prefillText"));
            }
        }
        finally
        {
            if (cut is not null)
            {
                lock (gate) CutBack(cut, prefill, steps);
                Notify();
            }
        }
        if (!files) return;
        try { await Request("rewind_files", new() { ["user_message_id"] = u.Uuid }); }
        catch (ClaudeRequestException ex) { throw new ClaudeRequestException(Strings.Get("Rewind.FilesFailed", ex.Message)); }
    }

    // Cuts the thread at `uuid` and puts that message back in the Composer, ahead of anything not sent yet. The text is
    // the UI's own (what was typed): prefillText is the CLI's stored form, internal XML for a slash command
    // (<command-name>/compact</command-name>…), so it only serves, normalised, when the message is not in the thread.
    // `rewound`: a message sent meanwhile (the automatic restore runs after the result, the Composer is open) stays.
    internal void CutBack(string uuid, string? prefill, IReadOnlyCollection<string>? rewound = null)
    {
        Suggestion = null;   // predicted after the turn that leaves
        var item = Items.OfType<UserItem>().FirstOrDefault(x => x.Uuid == uuid);
        if ((item?.Text ?? (prefill is null ? null : UserText(prefill))) is { } back)
            Draft = string.IsNullOrWhiteSpace(Draft) ? back : back + "\n\n" + Draft;
        if (item?.Images is { Count: > 0 } im && DraftImages.Count == 0) DraftImages = [.. im.Where(i => i.Url is not null)];
        Truncate(uuid, rewound);
    }

    // Esc before Claude answered: as in the terminal, the prompt goes back to the Composer and leaves the conversation
    // (rewind_conversation to it answers rewound after an interrupt, verified on claude 2.1.296). Not when a queued
    // message runs next, nor once the turn produced anything.
    internal UserItem? Unanswered() =>
        LastResultSubtype == Interrupted && Queued.IsEmpty && Items.FindLastIndex(i => i is UserItem) is >= 0 and var at
        && !Items.Skip(at + 1).Any(i => i is TextItem or ToolItem or ThoughtItem or ApiErrorItem or CompactItem or FallbackItem)
        && Items[at] is UserItem { Uuid: not null } u ? u : null;


    // The rewind_conversation targets for a rewind to `uuid`: it and every message sent after it, newest first.
    internal IReadOnlyList<string> RewindSteps(string uuid) =>
        [.. Items.OfType<UserItem>().SkipWhile(x => x.Uuid != uuid).Select(x => x.Uuid).OfType<string>().Reverse()];

    // cancel_async_message drops a message the CLI has not started: {cancelled:true}, then a `cancelled` frame removes the
    // chip (verified by --probe-cli queue). A cancel that loses the race to `started` is not probed: the chip leaves on that frame.
    public Task Cancel(QueuedMessage q) => Request("cancel_async_message", new() { ["message_uuid"] = q.Uuid });

    // mode: set_permission_mode before an allow (ExitPlanMode's approvals). message: a deny's text for the model ("keep planning").
    // input: what to allow the tool with instead of its own input (AskUserQuestion's answers, see AskUser).
    // destination: where an AllowSession's rules are kept (session, localSettings, projectSettings, userSettings).
    public async Task Answer(PendingPermission p, Decision d, string? mode = null, string? message = null, JsonElement? input = null,
                             string destination = "session")
    {
        // A bare allow makes AskUserQuestion return "The user did not answer the questions." (see AskUser).
        if (p.Tool == AskUser.Tool && d != Decision.Deny && input is null) throw new InvalidOperationException(Strings.Get("Ask.NeedsAnswers"));
        if (p.Tool == Elicit.Tool && d != Decision.Deny && input is null && !Elicit.IsUrl(p.Input)) throw new InvalidOperationException(Strings.Get("Ask.NeedsAnswers"));
        var c = proc ?? throw new InvalidOperationException(Strings.Get("Session.NoProcess"));
        // The request stays in Pending (its card stays up) until the CLI has its reply; a second answer meanwhile is a no-op.
        lock (gate) if (!answering.Add(p.RequestId)) return;
        try
        {
            // An elicitation: a deny declines, an allow accepts with the form's content (none in URL mode).
            if (p.Tool == Elicit.Tool) await c.Answer(p.RequestId, Elicit.Reply(d == Decision.Deny ? "decline" : "accept", input));
            else if (d == Decision.Deny) await c.Respond(p.RequestId, false, p.Input, message: message);
            else
            {
                JsonNode? updated = null;
                if (d == Decision.AllowSession && p.Suggestions is { ValueKind: JsonValueKind.Array } sg)
                {
                    var setMode = sg.EnumerateArray().FirstOrDefault(s => Events.Str(s, "type") == "setMode");
                    if (Events.Str(setMode, "mode") is { } m) mode = m;
                    else updated = Destined(sg, destination);   // session: verified by --probe-cli permission-session
                }
                // Answers {"mode":…}; the CLI then emits system/status with it once the allow lands (--probe-cli plan-*).
                if (mode is not null)
                {
                    Mode = Events.Str(await c.Request("set_permission_mode", new() { ["mode"] = mode }), "mode") ?? mode;
                    Notify();
                }
                await c.Respond(p.RequestId, true, input ?? p.Input, updated);
            }
            lock (gate) Resolve(p, d);
            Notify();
        }
        finally { lock (gate) answering.Remove(p.RequestId); }
    }

    // The CLI suggests rules for a settings file (localSettings: .claude/settings.local.json, kept for good). The card lets
    // the user pick where they go, session by default (no file keeps it): every suggestion is sent back with that destination.
    internal static JsonNode Destined(JsonElement suggestions, string destination)
    {
        var a = JsonNode.Parse(suggestions.GetRawText())!.AsArray();
        foreach (var s in a.OfType<JsonObject>()) s["destination"] = destination;
        return a;
    }

    // The subagent that asked (can_use_tool.agent_id = its task id), else the agent whose child the tool is.
    public ToolItem? AskingAgent(PendingPermission p) =>
        p.Info?.AgentId is not { } a ? null
        : Items.LastOrDefault(i => i is ToolItem { TaskId: var id } && id == a) as ToolItem
          ?? (Tool(p.ToolUseId)?.ParentToolUseId is { } parent ? Tool(parent) : null);

    // Ctrl B: the running Bash command or subagent moves to the background (the turn goes on without waiting for it).
    public Task Background(ToolItem t) => proc?.Request("background_tasks", new() { ["tool_use_id"] = t.Id }) ?? Task.CompletedTask;

    public async Task Interrupt()
    {
        if (proc is { } p) await p.Interrupt();
    }

    // Relaunch after a crash or open a past session: --resume when a transcript exists, else a fresh start with the same id.
    public async Task Restart()
    {
        if (proc is { } old) { proc = null; await old.DisposeAsync(); }   // proc is null first, so OnExit ignores old
        lock (gate) PluginStatus = PluginStatus.Clear();
        EnsureProcess(crashedMidTurn);
    }

    // Before the first send of a resumed session there is no process yet: the mode goes into --permission-mode.
    public async Task SetMode(string m)
    {
        if (proc is { } p) await p.Request("set_permission_mode", new() { ["mode"] = m });
        Mode = m;
        Notify();
    }

    public async Task SetModel(string m)
    {
        await Request("set_model", new() { ["model"] = m });
        Model = m;
        Notify();
    }

    public async Task SetEffort(string e)
    {
        await Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["effortLevel"] = e } });
        Effort = e;
        Notify();
    }

    // As the terminal's thinking toggle: 0 turns it off, null back to the session default (claude 2.1.296).
    public async Task SetThinking(bool on)
    {
        await Request("set_max_thinking_tokens", new() { ["max_thinking_tokens"] = on ? null : 0 });
        Thinking = on;
        Notify();
    }

    // advisorModel in the flag layer, null to remove it (/advisor off). The CLI drops an advisor weaker than the model:
    // applied.advisor says what stuck (verified on 2.1.296: "opus" → claude-opus-5-5, null → null).
    public async Task SetAdvisor(string? model)
    {
        await Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["advisorModel"] = model } });
        Advisor = Events.Prop(await Request("get_settings"), "applied") is { } a ? Events.Str(a, "advisor") : null;
        Notify();
    }

    // update_settings localSettings takes outputStyle (its allowlist on 2.1.296): written where /config writes it, the
    // project's .claude/settings.local.json, and live-applied; the next init reports it (verified by a probe).
    public async Task SetOutputStyle(string style)
    {
        await Request("update_settings", new() { ["source"] = "localSettings", ["settings"] = new JsonObject { ["outputStyle"] = style } });
        OutputStyle = style;
        Notify();
    }

    // get_memory_dialog clears the CLI's memory-file cache first, as /memory does: asked when the panel shows.
    public async Task RefreshMemory()
    {
        if (proc is not { } p) return;
        MemoryDialog = await p.Request("get_memory_dialog");
        Notify();
    }

    // autoMemoryEnabled in the user settings, through the CLI's writer (it also keeps an account-memory key in step).
    public async Task SetAutoMemory(bool on)
    {
        if (await CliEdit.Run(Cwd, "edit-memory-settings", new JsonObject { ["autoMemoryEnabled"] = on }) is { } err) throw new ClaudeRequestException(err);
        await RefreshMemory();
    }

    // The breakdown (behaviors) scans every local transcript of the week: asked only when shown.
    public async Task RefreshUsage(bool behaviors)
    {
        if (proc is not { } p) return;
        Usage = Usage.Parse(await p.Request("get_usage", behaviors ? null : new() { ["skip_behaviors"] = true }, 60));
        Notify();
    }

    // The model goes into the user settings file (the CLI's update_settings takes effortLevel only); the effort through
    // update_settings, saved for the session's model as /effort does ("max" is session-only: not written).
    public async Task SetDefault(string model, string? effort)
    {
        UserSettings.SetModel(model);
        if (effort is { } e and not "max")
            await Request("update_settings", new() { ["source"] = "userSettings", ["settings"] = new JsonObject { ["effortLevel"] = e } });
    }

    public Task SetFast(bool on) => Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["fastMode"] = on } });

    public Task SetUltracode(bool on)
    {
        Ultracode = on;
        Notify();
        return Task.CompletedTask;
    }

    // A live process gets apply_flag_settings permissions.additionalDirectories with the whole list (a flag write for
    // another key keeps it); every (re)launch passes --add-dir, which --resume alone does not restore. Both verified by
    // --probe-cli add-dir-live / add-dir-resume. "/add-dir" as user text is refused in stream-json.
    public async Task AddDir(string dir)
    {
        ClaudeSession? p;
        ImmutableList<string> all;
        // Listed under the gate before anything is sent: a process EnsureProcess starts after this gets it as --add-dir,
        // the one running now gets it below. Taken back if that request fails.
        lock (gate)
        {
            if (ExtraDir(dir, Cwd, AddDirs) is not { } d) return;
            AddDirs = all = AddDirs.Add(d);
            p = proc;
        }
        try
        {
            if (p is not null)
                await p.Request("apply_flag_settings", new() { ["settings"] = new JsonObject
                {
                    ["permissions"] = new JsonObject { ["additionalDirectories"] = new JsonArray([.. all.Select(x => (JsonNode?)x)]) },
                } });
        }
        catch
        {
            lock (gate) AddDirs = AddDirs.Remove(all[^1]);
            throw;
        }
        TranscriptStore.RecordDirs(Id, all);
        Notify();
    }

    // Case-sensitive where the file system usually is.
    static readonly StringComparer PathCmp = OperatingSystem.IsLinux() ? StringComparer.Ordinal : StringComparer.OrdinalIgnoreCase;

    // The folder as --add-dir gets it (trimmed, unquoted, no trailing separator); null when empty, the working folder or already listed.
    internal static string? ExtraDir(string raw, string cwd, IEnumerable<string> dirs)
    {
        var d = raw.Trim().Trim('"').Trim();
        if (d.Length == 0) return null;
        d = Path.TrimEndingDirectorySeparator(d);
        return PathCmp.Equals(d, Path.TrimEndingDirectorySeparator(cwd)) || dirs.Contains(d, PathCmp) ? null : d;
    }

    public async Task RefreshContext() { Context = await Request("get_context_usage"); Notify(); }
    public async Task RefreshMcp() { McpStatus = await Request("mcp_status"); Notify(); }

    // reload_plugins (/reload-plugins) reads the plugins from disk again and answers the plugins, agents and MCP servers
    // as they now are; the commands follow in its commands_changed push. reload_skills (/reload-skills) gives the skills,
    // which init.skills lists and reload_plugins does not. Counted from the answers: before the first message there is no
    // init to update. Errors: the load failures (details: `claude plugin list`).
    public async Task<(int Plugins, int Skills, int Agents, int Errors)> ReloadPlugins()
    {
        var r = await Request("reload_plugins");
        var k = await Request("reload_skills");
        lock (gate) Reloaded(r, k);
        Notify();
        return (Events.Names(r, "plugins").Length, Events.Names(k, "skills").Length, Events.Names(r, "agents").Length, (int)Events.Long(r, "error_count"));
    }

    internal void Reloaded(JsonElement r, JsonElement skills)
    {
        if (Init is { } i) Init = i with { Plugins = Events.Names(r, "plugins"), Agents = Events.Names(r, "agents"), Skills = Events.Names(skills, "skills") };
        if (InitializeInfo is { } info && Events.Prop(r, "agents") is { ValueKind: JsonValueKind.Array } a) InitializeInfo = With(info, "agents", a);
        if (Events.Prop(r, "mcpServers") is { ValueKind: JsonValueKind.Array } m) McpStatus = With(JsonDocument.Parse("{}").RootElement, "mcpServers", m);
    }

    // o with key set to v: InitializeInfo stays an immutable element, replaced whole.
    static JsonElement With(JsonElement o, string key, JsonElement v)
    {
        var n = JsonNode.Parse(o.GetRawText())!.AsObject();
        n[key] = JsonNode.Parse(v.GetRawText());
        return JsonSerializer.SerializeToElement(n);
    }

    // "Sign in again" after an MCP call refused for its credential: mcp_clear_auth drops the stored token (the server
    // turns needs-auth), then mcp_authenticate hands out a fresh sign-in page. Both verified by --probe-cli mcp-auth.
    public async Task<McpAuthStart> SignInAgain(string server)
    {
        await Request("mcp_clear_auth", new() { ["serverName"] = server });
        return McpAuthStart.Parse(await Request("mcp_authenticate", new() { ["serverName"] = server }));
    }

    // /cd's headless twin, set_cwd (claude 2.1.296): refused while a turn runs. A folder never trusted answers needs_trust
    // with its canonical path: re-sent with trust_accepted once the user said yes, trusted pinning the exact folder shown.
    // Returns that folder still to trust, else null; a refusal throws with the CLI's message.
    public async Task<string?> SetCwd(string path, string? trusted = null)
    {
        var f = new JsonObject { ["path"] = path };
        if (trusted is not null) { f["trust_accepted"] = true; f["trusted_directory"] = trusted; }
        var r = await Request("set_cwd", f);
        switch (Events.Str(r, "status"))
        {
            case "needs_trust": return Events.Str(r, "directory") ?? path;
            case "ok":
                if (Events.Str(r, "cwd") is { Length: > 0 } c && c != Cwd) { Cwd = c; Worktree = null; }
                _ = RefreshGit();
                return null;
            default: throw new ClaudeRequestException(Events.Str(r, "message") ?? Events.Str(r, "reason") ?? r.GetRawText());
        }
    }

    // A user rename, as /rename: rename_session from the host; session_title_changed then carries the name the CLI kept
    // (cleaned, made unique). Verified on claude 2.1.296.
    public Task Rename(string title) => Request("rename_session", new() { ["title"] = title.Trim(), ["source"] = "host" });

    // export_conversation: the plain text and file name the terminal's /export writes (verified on claude 2.1.296). Saved
    // in Downloads, beside a file of that name rather than over it; the path is returned for "Show in folder".
    public async Task<string> Export()
    {
        var r = await Request("export_conversation");
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var dir = Path.Combine(home, "Downloads");
        var path = FreeFile(Directory.Exists(dir) ? dir : home, Events.Str(r, "default_filename"));
        await File.WriteAllTextAsync(path, Events.Str(r, "text") ?? "");
        return path;
    }

    // A bare file name only (never a path out of dir), numbered -2, -3… while taken.
    internal static string FreeFile(string dir, string? name)
    {
        name = Path.GetFileName(name ?? "");
        if (name.Length == 0 || name.IndexOfAny(Path.GetInvalidFileNameChars()) >= 0) name = "conversation.txt";
        var (stem, ext) = (Path.GetFileNameWithoutExtension(name), Path.GetExtension(name));
        var path = Path.Combine(dir, name);
        for (var i = 2; File.Exists(path); i++) path = Path.Combine(dir, $"{stem}-{i}{ext}");
        return path;
    }

    // /status is not a stream-json command: get_status gives its rows (verified on claude 2.1.296).
    public async Task RefreshStatus()
    {
        if (proc is not { } p) return;
        StatusRows = StatusSection.Parse(await p.Request("get_status", null, 30));
        Notify();
    }

    // Remote Control (/remote-control): remote_control {enabled} starts the claude.ai/code bridge and answers its session_url;
    // {enabled:false} stops it. Both verified on claude 2.1.296. --remote-control itself is for interactive sessions only.
    public async Task SetRemote(bool on)
    {
        var r = await Request("remote_control", new() { ["enabled"] = on });
        RemoteUrl = on && Events.Str(r, "session_url") is { } u && Uri.TryCreate(u, UriKind.Absolute, out var uri) && uri.Scheme == "https" ? u : null;
        Notify();
    }

    // Sign in to Claude (/login): claude_authenticate answers the sign-in pages (the CLI itself receives automaticUrl's
    // callback), claude_oauth_wait_for_completion answers once the browser is done, with the account (claude 2.1.296).
    public async Task<string?> StartSignIn() =>
        Events.Str(await Request("claude_authenticate", new() { ["loginWithClaudeAi"] = true }), "automaticUrl") is { } u
        && Uri.TryCreate(u, UriKind.Absolute, out var uri) && uri.Scheme == "https" ? u : null;

    public async Task FinishSignIn()
    {
        var r = await EnsureProcess().Request("claude_oauth_wait_for_completion", null, 600);
        if (InitializeInfo is { } info && Events.Prop(r, "account") is { ValueKind: JsonValueKind.Object } a) InitializeInfo = With(info, "account", a);
        await RefreshStatus();
    }

    // EnterWorktree / ExitWorktree move the session mid-turn; their results say where (the next init confirms it).
    internal static string? WorktreeMove(string tool, JsonElement? result) => result is not { ValueKind: JsonValueKind.Object } r ? null : tool switch
    {
        "EnterWorktree" => Events.Str(r, "worktreePath"),
        "ExitWorktree" => Events.Str(r, "restoredCwd") ?? Events.Str(r, "originalCwd"),
        _ => null,
    };

    public Task<JsonElement> Request(string subtype, JsonObject? f = null) => EnsureProcess().Request(subtype, f);

    // Background tasks live in this process only: without one there is nothing to read or stop, and nothing to spawn.
    // get_task_output = the last 8 KiB the command wrote; stop_task kills it, then task_updated reports "killed".
    public async Task<string?> TaskOutput(string taskId) =>
        proc is { } p ? Events.Str(await p.Request("get_task_output", new() { ["task_id"] = taskId }), "output") ?? "" : null;
    public Task StopTask(string taskId) => proc?.Request("stop_task", new() { ["task_id"] = taskId }) ?? Task.CompletedTask;
    // UI only: the CLI keeps no list to clear. A task that ended stays ended, so nothing can bring it back.
    public void ClearEndedTasks()
    {
        lock (gate) Tasks = Tasks.RemoveAll(t => !t.Running);
        Notify();   // bumps Version: the Inspector must re-render to drop the panel once no task is left
    }

    // ---------- process ----------

    // Locked check-then-spawn: two circuits (or Boot and the reader thread) would otherwise start two claude with one --session-id.
    // continueTurn (Restart after a crash mid-turn): the --resume CLI re-runs the cut-off turn by itself.
    internal ClaudeSession EnsureProcess(bool continueTurn = false)
    {
        ClaudeSession s = null!;
        lock (gate)
        {
            if (proc is { } p) return p;
            // Ended by SessionManager.Stop: a late Send (a stale tab, Ctrl Enter during the dispose) must not start an orphan claude.
            if (stopped) throw new InvalidOperationException(Strings.Get("Session.ProcessStopped"));
            var resume = resumable || TranscriptStore.Find(Id) is not null;
            var args = Args(resume);
            try
            {
                // --resume restores the last persisted cost-state: total_cost_usd counts on from it (a fork's from its source's).
                costBase = CostUsd - (resume ? TranscriptStore.PersistedCost(Id) : ForkOf is { } src ? TranscriptStore.PersistedCost(src) : 0);
                resumeTurn = continueTurn && resume;
                crashedMidTurn = false;
                s = new ClaudeSession(Cwd, args, OnEvent, (code, text) => OnExit(s, code, text),
                    l => { if (Status == SessionStatus.Starting) Console.Error.WriteLine($"[{Id}] boot {l}"); }, resumeTurn);
            }
            catch (Exception ex)
            {
                OnExit(null, -1, Strings.Get("Session.StartFailed", ex.Message));
                throw;
            }
            proc = s;
            sent = [];
            ExitCode = null; ExitText = null;
            Status = SessionStatus.Starting;
        }
        Notify();
        _ = Boot(s);
        return s;
    }

    // The exact list EnsureProcess launches claude with (also reused by --boot-probe).
    internal List<string> Args(bool resume)
    {
        // --prompt-suggestions: a prompt_suggestion after each result, as the terminal's ghost text.
        var args = new List<string> { "--include-partial-messages", "--forward-subagent-text", TodoList.AllowedToolsArg, "--include-hook-events", "--prompt-suggestions", "true" };
        // No mode chosen (""): the CLI applies permissions.defaultMode, as in the terminal; initialize says which.
        if (Mode.Length > 0) args.InsertRange(0, ["--permission-mode", Mode == "default" ? "manual" : Mode]);
        if (resume) args.AddRange(["--resume", Id]);
        else if (ForkOf is { } src)
        {
            // Verified by --probe-cli fork: init.session_id is Id, the source transcript stays byte-identical, --name is the title.
            args.AddRange(["--resume", src, "--fork-session", "--session-id", Id, "--name", Name]);
            if (ForkAt is { } at) args.AddRange(["--resume-session-at", at]);
        }
        else
        {
            args.AddRange(["--session-id", Id, "--name", Name]);
            if (Worktree is { } w && Init is null) args.AddRange(["-w", w]);
        }
        if (Model is { } m) args.AddRange(["--model", m]);
        if (Effort is { } e) args.AddRange(["--effort", e]);
        if (FallbackModel is { } fb) args.AddRange(["--fallback-model", fb]);
        foreach (var d in AddDirs) args.AddRange(["--add-dir", d]);   // one flag each: the form --probe-cli add-dir-resume checks
        if (Restricted) args.Add("--restricted");
        if (SafeMode) args.Add("--safe-mode");
        // Without it stream-json sends every thinking block empty (verified on 2.1.296); read at each launch.
        if (AppFlag.Thinking.On) args.AddRange(["--thinking-display", "summarized"]);
        return args;
    }

    // system/init only arrives after the first message: fetch models, commands, effort and quota up front.
    async Task Boot(ClaudeSession p)
    {
        try
        {
            // A cold start (new folder, -w, hooks, MCP) can keep the CLI busy past a minute. Wait ONCE on the same
            // request_id: re-sending under a new id dropped a late answer to the first one.
            const int initSeconds = 180;
            var init = p.Request("initialize", null, initSeconds);
            // Thinking off survives a relaunch as a live 0, sent before any prompt: --thinking=disabled would anchor the CLI's
            // reset, so the toggle's null could never turn it back on (r0r.resetAnchor, claude 2.1.296).
            var off = Thinking ? null : p.Request("set_max_thinking_tokens", new() { ["max_thinking_tokens"] = 0 }, initSeconds);
            if (await Task.WhenAny(init, Task.Delay(60_000)) != init) Console.Error.WriteLine($"[{Id}] initialize: still waiting after 60 s");
            JsonElement info;
            try { info = await init; }
            catch (TimeoutException)
            {
                // A CLI that never answered initialize will not run a turn either: rather than a ready-looking Idle session,
                // stop it and show the reason in the crash banner, whose Restart button relaunches it.
                Console.Error.WriteLine($"[{Id}] initialize: no answer after {initSeconds} s, stopping claude");
                if (OnExit(p, -1, Strings.Get("Session.InitTimeout", initSeconds))) await p.DisposeAsync();   // false: p already exited or was replaced
                return;
            }
            InitializeInfo = info;
            OutputStyle ??= Events.Str(info, "output_style");
            if (off is not null) await off;
            if (Mode.Length == 0) Mode = Events.Str(info, "current_permission_mode") ?? "default";
            Console.WriteLine($"[{Id}] initialize: {Count(info, "models")} models, {Count(info, "commands")} commands");
            var settings = await p.Request("get_settings");
            if (Events.Prop(settings, "applied") is { } a)
            {
                Effort = Events.Str(a, "effort") ?? Effort;
                Model ??= Events.Str(a, "model");
                Advisor = Events.Str(a, "advisor");
            }
            statusCommand = StatusCommand.Of(settings);
            // alwaysThinkingEnabled: false starts the session with thinking off, as the terminal's toggle shows it.
            if (Events.Prop(settings, "effective") is { } eff && Events.Prop(eff, "alwaysThinkingEnabled") is { ValueKind: JsonValueKind.False }) Thinking = false;
            var usage = await p.Request("get_usage", new() { ["skip_behaviors"] = true });
            Usage = Usage.Parse(usage);
            if (Events.Prop(usage, "rate_limits") is { } rl
                && Events.Prop(rl, "five_hour") is { } h5 && Events.Prop(rl, "seven_day") is { } d7 && Limits is null)
            {
                Limits = new(Pct(h5), Reset(h5), Pct(d7), Reset(d7));   // get_usage is 0..100, normalised to 0..1
                LimitsAt = DateTimeOffset.Now;
            }
        }
        catch (Exception ex) { Console.Error.WriteLine($"[{Id}] startup: {ex.Message}"); }
        lock (gate) if (Status == SessionStatus.Starting) Status = SessionStatus.Idle;
        Notify();
        _ = RunStatusLine();

        static int Count(JsonElement e, string n) => Events.Prop(e, n) is { ValueKind: JsonValueKind.Array } x ? x.GetArrayLength() : 0;
        static double Pct(JsonElement w) => Events.Prop(w, "utilization") is { ValueKind: JsonValueKind.Number } u ? u.GetDouble() / 100 : 0;
        static DateTimeOffset Reset(JsonElement w) => DateTimeOffset.TryParse(Events.Str(w, "resets_at"), out var t) ? t : default;
    }

    // One run at a time; a failing or silent command leaves the last line rather than flicker.
    async Task RunStatusLine()
    {
        if (statusCommand is not { } cmd || Interlocked.Exchange(ref statusRunning, 1) == 1) return;
        try
        {
            if (await StatusCommand.Run(cmd, Cwd, StatusCommand.Payload(this)) is { } text) { StatusText = text; Notify(); }
        }
        catch (Exception ex) { Console.Error.WriteLine($"[{Id}] statusLine: {ex.Message}"); }
        finally { Interlocked.Exchange(ref statusRunning, 0); }
    }

    // One notification per batch, coalesced to 50 ms; a permission or a result is shown at once.
    internal Task OnEvent(JsonElement raw)
    {
        bool any = false, urgent = false;
        foreach (var e in Events.ParseAll(raw))
        {
            var cwd = Cwd;
            Apply(e);
            any = true;
            if (e is PermissionEvt or ResultEvt) urgent = true;
            else if (e is not TextDeltaEvt) Interlocked.Increment(ref version);
            if (e is ResultEvt { TotalCostUsd: > 0 }) TranscriptStore.RecordCost(Id, CostUsd);
            if (e is ResultEvt) _ = RunStatusLine();
            if (e is InitEvt && Branch is null || cwd != Cwd) _ = RefreshGit();   // cwd: init, or a worktree move mid-turn
            // The server confirmed a URL-mode elicitation: it is done, accept it for the user (the CLI's "I'm done").
            if (e is ElicitDoneEvt ed && Pending.FirstOrDefault(p => Elicit.Matches(p, ed.Server, ed.ElicitationId)) is { } ep)
                _ = Answer(ep, Decision.Allow).ContinueWith(t => Console.Error.WriteLine($"[{Id}] elicitation accept: {t.Exception?.InnerException?.Message}"), TaskContinuationOptions.OnlyOnFaulted);
            if (e is ResultEvt && Unanswered() is { } back && CanRewind(back))
                _ = Rewind(back, false).ContinueWith(t => Console.Error.WriteLine($"[{Id}] restore prompt: {t.Exception?.InnerException?.Message}"), TaskContinuationOptions.OnlyOnFaulted);
            if (e is ResultEvt && turnUltra)
            {
                turnUltra = false;
                Ultracode = false;
                _ = Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["ultracode"] = false } })
                    .ContinueWith(t => Console.Error.WriteLine($"[{Id}] ultracode off : {t.Exception?.InnerException?.Message}"), TaskContinuationOptions.OnlyOnFaulted);
            }
        }
        if (urgent) Notify(); else if (any) Throttled();
        return Task.CompletedTask;
    }

    // false when s is no longer this session's process (already exited or replaced): nothing changed.
    bool OnExit(ClaudeSession? s, int code, string text)
    {
        lock (gate)
        {
            if (s is not null && proc != s) return false;
            proc = null;
            crashedMidTurn = s is not null && Status is SessionStatus.Running or SessionStatus.Waiting;
            Status = SessionStatus.Crashed;
            RemoteUrl = null;   // the bridge lived in that process
            ExitCode = code; ExitText = text; StartFailed = s is null;
            Pending = [];
            Queued = []; draining = false;
            TurnStartedAt = null;
            CompactingSince = null;
            RunningHooks = RunningHooks.Clear();
            ClearStream();
            EndTools(DateTimeOffset.Now, false);
            PluginStatus = PluginStatus.Clear();   // the next process never sends text:null for the old plugins' lines
        }
        Notify();
        return true;
    }

    internal async Task RefreshGit()
    {
        Branch = (await Git(Cwd, "branch", "--show-current"))?.Trim() is { Length: > 0 } b ? b : null;
        if ((await Git(Cwd, "rev-parse", "--path-format=absolute", "--git-common-dir"))?.Trim() is { Length: > 0 } common)
            Repo = Path.GetFileName(Path.GetDirectoryName(common.TrimEnd('/', '\\')));
        Notify();
    }

    // The main checkout's root (the git common dir's parent), where the CLI keeps localSettings; null outside a repo.
    public static async Task<string?> GitRoot(string dir) =>
        (await Git(dir, "rev-parse", "--path-format=absolute", "--git-common-dir"))?.Trim().TrimEnd('/', '\\') is { Length: > 0 } c
        && Path.GetFileName(c) == ".git" ? Path.GetDirectoryName(c) : null;

    internal static async Task<string?> Git(string dir, params string[] args)
    {
        try
        {
            // stderr unread would fill its pipe on a chatty diff (one CRLF warning per file) and block git: drain it alongside stdout.
            var psi = new ProcessStartInfo("git") { RedirectStandardOutput = true, RedirectStandardError = true };
            psi.ArgumentList.Add("-C"); psi.ArgumentList.Add(dir);
            foreach (var a in args) psi.ArgumentList.Add(a);
            using var p = Process.Start(psi)!;
            var err = p.StandardError.ReadToEndAsync();
            var output = await p.StandardOutput.ReadToEndAsync();
            await err;
            await p.WaitForExitAsync();
            return p.ExitCode == 0 ? output : null;
        }
        catch (Exception) { return null; }
    }

    public async ValueTask DisposeAsync()
    {
        ClaudeSession? p;
        lock (gate) { stopped = true; p = proc; proc = null; }
        if (p is not null) await p.DisposeAsync();
        lock (gate)
        {
            if (Status != SessionStatus.Crashed) Status = SessionStatus.Exited;
            Pending = [];
            Queued = []; draining = false;
            TurnStartedAt = null;
            EndTools(DateTimeOffset.Now, false);
        }
        Notify();
    }

    // ---------- notifications: at most one per 50 ms while events stream ----------

    // Bumped by every change except streamed text: views that do not show StreamingText skip renders where only it moved.
    public int Version => Volatile.Read(ref version);

    void Notify()
    {
        Interlocked.Increment(ref version);
        Raise();
    }

    void Raise()
    {
        Interlocked.Exchange(ref lastNotify, Environment.TickCount64);
        Changed?.Invoke();
    }

    void Throttled()
    {
        var wait = 50 - (Environment.TickCount64 - Interlocked.Read(ref lastNotify));
        if (wait <= 0) { Raise(); return; }
        if (Interlocked.Exchange(ref notifyQueued, 1) == 1) return;
        _ = Task.Delay((int)wait).ContinueWith(_ => { Interlocked.Exchange(ref notifyQueued, 0); Raise(); });
    }

    // ---------- reducer ----------

    // The message with this uuid and everything after it (its tools, answers, later turns) leave the thread; with
    // `rewound`, only up to the first later message not in it.
    internal void Truncate(string uuid, IReadOnlyCollection<string>? rewound = null)
    {
        var i = Items.FindIndex(x => x is UserItem u && u.Uuid == uuid);
        if (i < 0) return;
        var end = rewound is null ? -1 : Items.FindIndex(i + 1, x => x is UserItem u && !rewound.Contains(u.Uuid ?? ""));
        Items = Items.RemoveRange(i, (end < 0 ? Items.Count : end) - i);
    }

    internal void BeginTurn(string text, IReadOnlyList<UserImage>? images = null, string? uuid = null)
    {
        Items = Items.Add(new UserItem(text, DateTimeOffset.Now, Ultracode, images, uuid));
        resumeTurn = false;
        Status = SessionStatus.Running;
        TurnStartedAt = DateTimeOffset.Now;
        ClearStream();
        ThinkingTokens = 0;
    }

    void ResumedTurn(DateTimeOffset now)
    {
        if (!resumeTurn || Status is not (SessionStatus.Idle or SessionStatus.Starting)) return;
        resumeTurn = false;
        Items = Items.Add(new ResumedItem());
        Status = SessionStatus.Running;
        TurnStartedAt = now;
        ClearStream();
        ThinkingTokens = 0;
    }

    // Runs after the reply was sent, so the CLI may already have moved the tool on (result, end of turn): only a Waiting tool changes.
    internal void Resolve(PendingPermission p, Decision d)
    {
        if (!Pending.Contains(p)) return;
        Pending = Pending.Remove(p);
        if (Tool(p.ToolUseId) is { State: ToolState.Waiting } t) Set(t, t with { State = d == Decision.Deny ? ToolState.Denied : ToolState.Running });
        if (Pending.IsEmpty && Status == SessionStatus.Waiting) Status = SessionStatus.Running;
    }

    // Tools still open when the turn or the process ends: they never got a result. The process's end also ends its
    // background shells (ProcessJob kills its process group; --probe-cli exit-ends-tasks), which no task_updated will report.
    internal void EndTools(DateTimeOffset now, bool keepBackground = true)
    {
        Items = Items.ConvertAll(i => i is ToolItem { State: ToolState.Running or ToolState.Waiting } t && !(keepBackground && t.Background)
            ? t with { State = ToolState.Error, EndedAt = now } : i);
        if (!keepBackground) Tasks = Tasks.ConvertAll(t => t.Running ? t with { Status = "killed", EndedAt = now } : t);
    }

    public ToolItem? Tool(string? id) => id is null ? null : Items.LastOrDefault(i => i is ToolItem t && t.Id == id) as ToolItem;

    // Subagent items by parent tool id, in order; rebuilt once per Items snapshot (ids and parents never change in place).
    sealed record ChildIndex(ImmutableList<Item> Of, ILookup<string, Item> By);
    ChildIndex? children;
    public ILookup<string, Item> Children
    {
        get
        {
            var items = Items;
            if (children is { } c && c.Of == items) return c.By;
            var by = items.Select(i => (P: i switch { ToolItem t => t.ParentToolUseId, TextItem x => x.ParentToolUseId, _ => null }, I: i))
                          .Where(x => x.P is not null).ToLookup(x => x.P!, x => x.I);
            children = new(items, by);
            return by;
        }
    }

    // Replaces one tool row (by reference: two rows never share an instance). Callers hold the gate, or own the session (replay).
    internal void Set(ToolItem old, ToolItem now) => Items = Items.Replace(old, now, ReferenceEqualityComparer.Instance);

    internal void Apply(ClaudeEvent e)
    {
        lock (gate) Reduce(e);
    }

    void Reduce(ClaudeEvent e)
    {
        var now = DateTimeOffset.Now;
        LastEventAt = now;
        if (e is not (RetryEvt or RateLimitEvt or HookEvt or HookStartedEvt)) Retry = null;
        if (e is HookEvt { Id: { } hid }) RunningHooks = RunningHooks.Remove(hid);
        switch (e)
        {
            case HookStartedEvt hs:
                RunningHooks = RunningHooks.SetItem(hs.Id, (hs.Name, now));
                break;

            case RetryEvt rt:
                Retry = rt;
                break;

            // The CLI withdrew the request (e.g. a dangerous rm left unanswered): the card goes, the tool's result follows.
            case CancelRequestEvt c when Pending.FirstOrDefault(x => x.RequestId == c.RequestId) is { } cp:
                Resolve(cp, Decision.Allow);
                break;

            case InitEvt i:
                Init = i;
                if (i.Cwd.Length > 0) Cwd = i.Cwd;
                if (i.Model.Length > 0) Model = i.Model;
                if (i.PermissionMode.Length > 0) Mode = i.PermissionMode;
                FastModeState = i.FastModeState;
                FastModeReason = i.FastModeReason;
                if (i.OutputStyle is { } os) OutputStyle = os;
                break;

            case StatusEvt se:
                if (se.PermissionMode is { } pm) Mode = pm;
                // A compaction ends on the next status: {status:null, compact_result} or "requesting" (--probe-cli compact).
                CompactingSince = se.Status == "compacting" ? CompactingSince ?? now : null;
                if (se.Status == "requesting") ResumedTurn(now);
                break;

            // CLAUDE_CODE_RESUME_INTERRUPTED_TURN: the relaunched CLI re-runs the cut-off turn on its own, with no user frame:
            // command_lifecycle started for the old prompt (before initialize even answers), then status "requesting"
            // (--probe-cli resume-turn, claude 2.1.296).
            case QueueEvt { State: "started" } when resumeTurn && Queued.IsEmpty:
                ResumedTurn(now);
                break;

            case CompactEvt c:
                Items = Items.Add(new CompactItem(c.Trigger, c.PreTokens, c.PostTokens, c.DurationMs));
                break;

            // The summary follows its boundary (SessionStart:compact hooks may come between, on the stream).
            case SummaryEvt sm when Items.FindLastIndex(i => i is CompactItem) is >= 0 and var ci && Items[ci] is CompactItem { Summary: null } cc:
                Items = Items.SetItem(ci, cc with { Summary = sm.Text });
                break;

            case ThoughtEvt { ParentToolUseId: null } th:
                Items = Items.Add(new ThoughtItem(th.Text));
                break;

            // A refusal fallback retracts the refused partial (its uuids) before the retried answer comes.
            case FallbackEvt fb:
                if (fb.Retracted.Count > 0)
                {
                    Items = Items.RemoveAll(i => i is TextItem { Uuid: { } tu } && fb.Retracted.Contains(tu));
                    ClearStream();
                }
                if (fb.Model is { } fm) Model = fm;
                Items = Items.Add(new FallbackItem(fb.Text, fb.Retry));
                break;

            case ArtifactsEvt ar:
                Artifacts = [.. ar.Artifacts];
                break;

            case ThinkingEvt t:
                ThinkingTokens = t.EstimatedTokens;
                break;

            case TextDeltaEvt { ParentToolUseId: null } d:
                stream.Append(d.Text);
                streamText = null;
                break;

            case AssistantTextEvt { Synthetic: true, ParentToolUseId: null } a when ApiErrors.Parse(a.Text) is { } err:
                ClearStream();
                Items = Items.Add(new ApiErrorItem(err));
                break;

            case AssistantTextEvt a:
                if (a.ParentToolUseId is null) ClearStream();
                Items = Items.Add(new TextItem(a.Text, a.ParentToolUseId, a.Uuid, a.At ?? now));
                break;

            case ToolUseEvt u:
                if (u.ParentToolUseId is null) ClearStream();
                Items = Items.Add(new ToolItem(u.Id, u.Name, u.Input, u.ParentToolUseId) { State = ToolState.Running, StartedAt = now });
                ToolCount++;
                if (u.Name == PushTool) { Pushes++; LastPush = Events.Str(u.Input, "message"); }
                break;

            case ToolResultEvt { Structured: { } rs } r when Tool(r.ToolUseId) is { State: ToolState.Running } t && (ToolKinds.IsAgent(t.Name) || t.Name == WorkflowRuns.Tool)
                                                              && Events.Str(rs, "status") == "async_launched":
                // launch receipt, not the outcome: stays Running until the task-notification
                Set(t, t with { Background = true, TaskId = Events.Str(rs, "agentId") ?? Events.Str(rs, "taskId") ?? t.TaskId, Structured = rs });
                break;

            case ToolResultEvt r when Tool(r.ToolUseId) is { } t:
                Set(t, t with
                {
                    State = r.NotRun ? ToolState.NotRun : r.IsError ? (t.State == ToolState.Denied ? ToolState.Denied : ToolState.Error) : ToolState.Done,
                    ResultText = r.Text,
                    Structured = r.Structured,
                    EndedAt = now,
                    Tokens = r.Structured is { } st && Events.Prop(st, "totalTokens") is { ValueKind: JsonValueKind.Number } tok ? tok.GetInt64() : t.Tokens,
                });
                // A subagent's EnterWorktree moves only that agent: top-level calls move the session.
                if (!r.IsError && t.ParentToolUseId is null && WorktreeMove(t.Name, r.Structured) is { Length: > 0 } moved)
                {
                    Cwd = moved;
                    if (t.Name == "ExitWorktree") Worktree = null;
                }
                break;

            // A transcript writes the /compact line after the boundary and summary it produced: it goes back above its card.
            case UserTextEvt u when UserText(u.Text) is { } text && text.StartsWith("/compact") && Items.LastOrDefault() is CompactItem:
                Items = Items.Insert(Items.Count - 1, new UserItem(text, u.At ?? now, false, u.Images));
                break;

            case UserTextEvt u when UserText(u.Text) is { } text:
                Items = Items.Add(new UserItem(text, u.At ?? now, false, u.Images));
                break;

            case PermissionEvt p:
                Pending = Pending.Add(new(p.RequestId, p.Tool, p.Input, p.Description, p.ToolUseId, p.Suggestions, now, p.Info));
                if (Tool(p.ToolUseId) is { } wt) Set(wt, wt with { State = ToolState.Waiting });
                Status = SessionStatus.Waiting;
                break;

            case ResultEvt r:
                if (r.TotalCostUsd > 0)   // cumulative per process: assign, never add (slash commands report 0); a relaunch restarts at 0
                {
                    LastTurnCostUsd = costBase + r.TotalCostUsd - CostUsd;
                    CostUsd = costBase + r.TotalCostUsd;
                }
                else LastTurnCostUsd = 0;
                LastTurn = TimeSpan.FromMilliseconds(r.DurationMs);
                LastResultSubtype = r.TerminalReason?.StartsWith("aborted") == true ? Interrupted : r.Subtype;
                LastResultAt = now;
                if (r.ContextTokens > 0) (ContextTokens, CacheRead, CacheWrite) = (r.ContextTokens, r.CacheRead, r.CacheWrite);
                if (r.ContextWindow is { } w) ContextWindow = w;
                if (r.FastModeState is { } fs) { FastModeState = fs; FastModeReason = r.FastModeReason; }
                // A call refused by a deny rule or the auto-mode classifier never asked: only its error result and this list say so.
                foreach (var id in r.Denied ?? [])
                    if (Tool(id) is { State: ToolState.Error } dt) Set(dt, dt with { State = ToolState.Denied });
                EndTools(now);
                Pending = [];
                ClearStream();
                ThinkingTokens = 0;
                CompactingSince = null;
                RunningHooks = RunningHooks.Clear();   // one that never answered (an interrupted turn) must not hold the line
                if (!Queued.IsEmpty)   // a queued message runs next, even after an interrupt
                {
                    draining = true;
                    ultraCarry = turnUltra;   // read before OnEvent resets it
                    TurnStartedAt = now;
                    if (Status is SessionStatus.Waiting or SessionStatus.Starting) Status = SessionStatus.Running;   // Esc while a permission waits
                    break;
                }
                TurnStartedAt = null;
                if (Status is SessionStatus.Running or SessionStatus.Waiting or SessionStatus.Starting) Status = SessionStatus.Idle;
                break;

            case RateLimitEvt l:
                // unifiedWindows carries each window only once known: keep the other one, and show nothing before a first window.
                if (Limits is not { } k)
                {
                    if (l.FiveHourReset == default && l.SevenDayReset == default) break;
                    k = l;
                }
                Limits = l with
                {
                    FiveHour = l.FiveHourReset == default ? k.FiveHour : l.FiveHour, FiveHourReset = l.FiveHourReset == default ? k.FiveHourReset : l.FiveHourReset,
                    SevenDay = l.SevenDayReset == default ? k.SevenDay : l.SevenDay, SevenDayReset = l.SevenDayReset == default ? k.SevenDayReset : l.SevenDayReset,
                };
                LimitsAt = now;
                break;

            case TaskStartedEvt ts:
                if (Tool(ts.ToolUseId) is { } launcher) Set(launcher, launcher with { TaskId = ts.TaskId });
                if (ts is { TaskType: "local_bash", Backgrounded: true } && !Tasks.Any(x => x.Id == ts.TaskId))
                    Tasks = Tasks.Add(new(ts.TaskId, ts.ToolUseId, ts.Description, now));
                break;

            // Moved to the background mid-run (background_tasks, verified by a probe on claude 2.1.296): a shell joins the
            // background tasks, an agent ends on its task-notification instead of its tool result.
            case TaskUpdatedEvt { Backgrounded: true } tb when Items.LastOrDefault(i => i is ToolItem x && x.TaskId == tb.TaskId) is ToolItem bl:
                Set(bl, bl with { Background = true });
                if (ToolKinds.IsShell(bl.Name) && !Tasks.Any(x => x.Id == tb.TaskId))
                    Tasks = Tasks.Add(new(tb.TaskId, bl.Id, Events.Str(bl.Input, "description") ?? ToolKinds.Target(bl.Name, bl.Input, Cwd), bl.StartedAt));
                break;

            case TaskUpdatedEvt { Status: { } status } tu when Tasks.FirstOrDefault(x => x.Id == tu.TaskId) is { } bt:
                var ended = bt with { Status = status };
                Tasks = Tasks.Replace(bt, ended with { EndedAt = ended.Running ? null : tu.EndedAt ?? now });
                break;

            case TaskProgressEvt tp when Items.LastOrDefault(i => i is ToolItem t && t.TaskId == tp.TaskId) is ToolItem t:   // IList: scans from the end
                Set(t, t with { Tokens = tp.TotalTokens, SubToolUses = tp.ToolUses, Progress = tp.Workflow ?? t.Progress });   // some carry no snapshot
                break;

            case TaskDoneEvt td when (Tool(td.ToolUseId) ?? Items.LastOrDefault(i => i is ToolItem x && td.TaskId.Length > 0 && x.TaskId == td.TaskId) as ToolItem) is { } t
                                     && (t.State == ToolState.Running || t.Background):   // a resumed background agent notifies again
                Set(t, t with
                {
                    State = td.Status == "completed" ? ToolState.Done : ToolState.Error,
                    TaskStatus = td.Status,
                    EndedAt = td.DurationMs > 0 ? t.StartedAt.AddMilliseconds(td.DurationMs) : now,
                    ResultText = td.Result ?? t.ResultText,
                    Tokens = td.Tokens > 0 ? td.Tokens : t.Tokens,
                    SubToolUses = td.ToolUses > 0 ? td.ToolUses : t.SubToolUses,
                });
                break;

            // The uuid is ours (Send). A message sent while idle is already in Items: only queued ones are moved here.
            case QueueEvt { State: not "queued" } q when Queued.FirstOrDefault(m => m.Uuid == q.Uuid) is { } m:
                Queued = Queued.Remove(m);
                if (q.State == "started")
                {
                    // Folded into an ultracode turn, or started right after one: the CLI starts it as it emits that result,
                    // before OnEvent's ultracode:false can land, so it very likely runs with ultracode still on (not probed).
                    // Labelled ultracode so the stop button says what may be running.
                    Items = Items.Add(new UserItem(m.Text, now, turnUltra || draining && ultraCarry, Uuid: m.Uuid));
                    draining = false;
                    if (Status == SessionStatus.Idle) { Status = SessionStatus.Running; TurnStartedAt = now; }
                }
                else if (draining && Queued.IsEmpty)   // cancelled / discarded / refused before it could start
                {
                    draining = false;
                    TurnStartedAt = null;
                    if (Status == SessionStatus.Running) Status = SessionStatus.Idle;
                }
                break;

            case HookEvt h when Shown(h):
                Items = Items.Add(new HookItem(h));
                break;

            case TitleEvt { Title.Length: > 0 } ti:
                Name = ti.Title;
                break;

            case PrEvt pr:
                PrUrl = pr.Url;
                break;

            case ResetEvt:
                Items = Items.Add(new ResetItem());
                Suggestion = null;
                break;

            case NoticeEvt n:
                Items = Items.Add(new NoticeItem(n));
                break;

            case PluginStatusEvt ps:
                PluginStatus = ps.Text is { } line ? PluginStatus.SetItem(ps.Plugin, line) : PluginStatus.Remove(ps.Plugin);
                break;

            case SuggestionEvt sg:
                Suggestion = sg.Text;
                break;

            case CommandsEvt c when InitializeInfo is { } info:
                InitializeInfo = With(info, "commands", c.Commands);
                break;
        }
    }

    // The history a fork shows: up to and including the text block `at` (what --resume-session-at keeps), else all of it.
    internal static ImmutableList<Item> Upto(IReadOnlyList<Item> items, string? at)
    {
        var i = at is null ? -1 : items.ToList().FindIndex(x => x is TextItem t && t.Uuid == at);
        return [.. i < 0 ? items : items.Take(i + 1)];
    }

    // maxEffortLevel from get_settings.effective: modelSettings.<model> replaces the top-level one ("max" exempts the model),
    // keyed by canonical name, which also matches the dated and [1m] spellings. Levels above it the CLI clamps: not offered.
    internal static string? EffortCap(JsonElement effective, string? model)
    {
        var cap = Events.Str(effective, "maxEffortLevel");
        if (model is { } m && Events.Prop(effective, "modelSettings") is { ValueKind: JsonValueKind.Object } ms)
            foreach (var x in ms.EnumerateObject())
                if (m.StartsWith(x.Name, StringComparison.OrdinalIgnoreCase) && Events.Str(x.Value, "maxEffortLevel") is { } own) cap = own;
        return cap == "max" ? null : cap;
    }

    // The keys of the CLI's reply to a bare "/config" (2.1.296): "Usage: /config key=value…" then "  key=a|b|c" or
    // "  key=<value>" (free text: no choices). null when the text is not that reply.
    internal static IReadOnlyList<(string Key, string[] Choices)>? ConfigKeys(string text)
    {
        if (!text.StartsWith(ConfigUsage)) return null;
        return [.. text.Split('\n').Skip(1).Select(l => l.Trim()).Where(l => l.IndexOf('=') > 0)
            .Select(l => (l[..l.IndexOf('=')], l[(l.IndexOf('=') + 1)..] is var v && v.StartsWith('<') ? [] : v.Split('|')))];
    }
    internal const string ConfigUsage = "Usage: /config key=value";

    internal static IReadOnlyList<string> Capped(IReadOnlyList<string> levels, string? cap) =>
        cap is not null && levels.ToList().IndexOf(cap) is >= 0 and var i ? [.. levels.Take(i + 1)] : levels;

    // A silent success is the common case (and most of the stream's hook events): nothing to show. Context a hook injects
    // into the prompt is for the model, not to read: a successful SessionStart (plugins: ~11 KB per start) or SubagentStart
    // (~5 KB per Agent call, measured on 2.1.296), and any output that is only hookSpecificOutput.additionalContext.
    internal static bool Shown(HookEvt h) =>
        h.Outcome != "success"
        || h.Output.Length > 0 && !h.Name.StartsWith("SessionStart") && !h.Name.StartsWith("SubagentStart") && !Events.ContextOnly(h.Output);

    // The Args patterns below are for summaries off, whatever this machine's toggle says.
    internal static void Check()
    {
        var flag = AppFlag.Thinking.Path;
        AppFlag.Thinking.Path = Path.Combine(Path.GetTempPath(), "ccui-check-" + Guid.NewGuid().ToString("N"));
        try { CheckReducer(); }
        finally { if (File.Exists(AppFlag.Thinking.Path)) File.Delete(AppFlag.Thinking.Path); AppFlag.Thinking.Path = flag; }
    }

    // send → tool_use → permission → deny → result, replayed on the reducer only.
    static void CheckReducer()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "LiveSession: " + what);
        var input = JsonDocument.Parse("""{"file_path":"C:\\w\\b.txt","content":"x"}""").RootElement.Clone();
        var fresh = new LiveSession("id", "n", @"C:\w", "default").Args(false);
        Ok(fresh is ["--permission-mode", "manual", _, _, TodoList.AllowedToolsArg, "--include-hook-events", "--prompt-suggestions", "true", "--session-id", "id", "--name", "n"], "fresh args");
        var resumed = new LiveSession("o", "o", @"C:\w", "plan", model: "haiku").Args(true);
        Ok(resumed is ["--permission-mode", "plan", _, _, _, _, _, _, "--resume", "o", "--model", "haiku"], "resume args");
        var fork = new LiveSession("f", "x (fork)", @"C:\w", "default") { ForkOf = "o", ForkAt = "u1" };
        Ok(fork.Args(false) is ["--permission-mode", "manual", _, _, TodoList.AllowedToolsArg, "--include-hook-events", _, _, "--resume", "o", "--fork-session", "--session-id", "f", "--name", "x (fork)", "--resume-session-at", "u1"], "fork args");
        Ok(fork.Args(true) is [_, _, _, _, _, _, _, _, "--resume", "f"], "fork resumes its own transcript once written");
        Ok(new LiveSession("c", "c", @"C:\w", "").Args(false) is [_, _, TodoList.AllowedToolsArg, "--include-hook-events", _, _, "--session-id", "c", ..], "no mode chosen: no --permission-mode");
        var sg = JsonDocument.Parse("""[{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"npm test:*"}],"behavior":"allow","destination":"localSettings"},{"type":"addDirectories","directories":["/x"],"destination":"localSettings"}]""").RootElement;
        Ok(Destined(sg, "session").ToJsonString() is var so && so.Contains("\"destination\":\"session\"") && !so.Contains("localSettings") && so.Contains("npm test:*"), "whole-session allow stays in the session");
        Ok(Destined(sg, "userSettings").ToJsonString() is var su && su.Split("\"destination\":\"userSettings\"").Length == 3, "always-allow goes where the user picked");
        Item[] h = [new UserItem("a", default, false), new TextItem("b", null, "u1"), new UserItem("c", default, false), new TextItem("d", null, "u2")];
        Ok(Upto(h, "u1") is [UserItem, TextItem { Uuid: "u1" }] && Upto(h, null).Count == 4, "fork history cut");
        var dirs = new LiveSession("d", "d", "/w", "default", resumable: true, addDirs: ["/a", "/b c"]).Args(true);
        Ok(dirs is [.., "--resume", "d", "--add-dir", "/a", "--add-dir", "/b c"], "resume keeps --add-dir");
        Ok(ExtraDir(" \"/x/y/\" ", "/w", []) == "/x/y" && ExtraDir("/w/", "/w", []) is null && (ExtraDir("/A", "/w", ["/a"]) is null) == !OperatingSystem.IsLinux()
           && ExtraDir("  ", "/w", []) is null && ExtraDir("/", "/w", []) == "/", "extra dir");
        Ok(new LiveSession("i", "i", "/r/.claude/worktrees/x", "auto").Isolated && new LiveSession("i", "i", "/r", "auto", worktree: "x").Isolated
           && !new LiveSession("i", "i", "/r", "auto").Isolated
           && !new LiveSession("i", "i", "/r/.claude/worktrees/x", "auto", addDirs: ["/o"]).Isolated
           && !new LiveSession("i", "i", "/r", "auto", worktree: "x", addDirs: ["/o"]).Isolated, "extra folders are never isolated");
        Ok(ClaudeSession.TraceLine(1234, "stderr x") == "+1234 ms stderr x", "trace line format");
        AppFlag.Thinking.On = true;
        Ok(new LiveSession("t", "t", "/w", "default").Args(false) is [.., "--name", "t", "--thinking-display", "summarized"], "thinking summaries asked when on");
        AppFlag.Thinking.On = false;
        var fb = new LiveSession("fb", "fb", "/w", "default", model: "opus") { FallbackModel = "sonnet" };
        fb.Thinking = false;
        Ok(fb.Args(false) is [.., "--model", "opus", "--fallback-model", "sonnet"], "fallback model, thinking off not anchored at launch");
        // claude 2.1.296's answer to a bare /config (trimmed).
        Ok(ConfigKeys("Usage: /config key=value [key=value ...]\n  autoCompact=true|false\n  language=<value>\n  switchModelsOnFlag=Switch automatically|Ask each time")
           is [("autoCompact", ["true", "false"]), ("language", []), ("switchModelsOnFlag", ["Switch automatically", "Ask each time"])]
           && ConfigKeys("Set Language to French") is null, "/config usage keys");
        Ok(Capped(["low", "medium", "high", "xhigh", "max"], "high") is ["low", "medium", "high"] && Capped(["low", "high"], null).Count == 2 && Capped(["low", "high"], "auto").Count == 2, "effort cap");
        var effS = JsonDocument.Parse("""{"maxEffortLevel":"high","modelSettings":{"claude-fable-5-1":{"maxEffortLevel":"max"},"claude-opus-5-5":{"maxEffortLevel":"medium"}}}""").RootElement;
        Ok(EffortCap(effS, "claude-haiku-5-5") == "high" && EffortCap(effS, "claude-fable-5-1") is null && EffortCap(effS, "claude-opus-5-5[1m]") == "medium"
           && EffortCap(JsonDocument.Parse("{}").RootElement, "x") is null, "maxEffortLevel, per model, max exempts");
        var rlm = new LiveSession("rl", "rl", "/w", "default");
        rlm.Apply(new RateLimitEvt(0, default, 0, default, "rejected", "five_hour"));
        Ok(rlm.Limits is null, "no quota before a first window");
        rlm.Apply(new RateLimitEvt(0.05, DateTimeOffset.FromUnixTimeSeconds(1), 0.91, DateTimeOffset.FromUnixTimeSeconds(2)));
        rlm.Apply(new RateLimitEvt(0, default, 0, default, "rejected", "seven_day"));
        Ok(rlm.Limits is { Status: "rejected", SevenDay: 0.91, FiveHour: 0.05 }, "rate limit status keeps the known windows");
        rlm.Apply(new RateLimitEvt(0.4, DateTimeOffset.FromUnixTimeSeconds(3), 0, default));
        Ok(rlm.Limits is { FiveHour: 0.4, SevenDay: 0.91, SevenDayReset: var r7 } && r7.ToUnixTimeSeconds() == 2, "a lone five_hour window keeps seven_day");
        rlm.Apply(new ResultEvt("success", false, 0, 10, 1, 34232, null, null, null, CacheRead: 11204, CacheWrite: 23026));
        Ok(rlm is { ContextTokens: 34232, CacheRead: 11204, CacheWrite: 23026 }, "prompt cache of the last call");

        // /compact as probed on 2.1.296: compacting, hooks, status null, boundary, summary; then an auto compaction mid-turn.
        var cp = new LiveSession("cp", "cp", "/w", "default");
        cp.BeginTurn("/compact");
        cp.Apply(new StatusEvt("compacting", null));
        var since = cp.CompactingSince;
        cp.Apply(new HookEvt("SessionStart:compact", "success", 0, "ctx"));
        cp.Apply(new StatusEvt("compacting", null));
        Ok(since is not null && cp.CompactingSince == since, "compacting since its first status");
        cp.Apply(new StatusEvt(null, null));
        Ok(cp.CompactingSince is null, "status null ends compacting");
        cp.Apply(new CompactEvt("manual", 37317, 8995, 3207));
        cp.Apply(new HookEvt("SessionStart:compact", "error", 1, "boom"));
        cp.Apply(new SummaryEvt("This session is being continued… Summary: hi"));
        Ok(cp.Items is [UserItem, CompactItem { Trigger: "manual", PreTokens: 37317, Summary: "This session is being continued… Summary: hi" }, HookItem], "compact card gets its summary");
        cp.Apply(new SummaryEvt("again"));
        Ok(cp.Items[1] is CompactItem { Summary: "This session is being continued… Summary: hi" }, "a summary never replaces another");
        cp.Apply(new StatusEvt("compacting", null));
        cp.Apply(new ResultEvt("success", false, 0, 10, 0, 0, null, null, null));
        Ok(cp.CompactingSince is null && ThreadBlocks.Of(cp.Items) is [UserItem, CompactItem, ThreadBlocks.Hooks], "result ends compacting; the card is a block");

        // Thinking summary, fallback notice (retracts the refused partial, swaps the model), artifacts, push, not run.
        var fx = new LiveSession("fx", "fx", "/w", "default", model: "claude-fable-5");
        fx.BeginTurn("q", uuid: "u7");
        fx.Apply(new ThoughtEvt("Checking.", null));
        fx.Apply(new ThoughtEvt("sub", "toolu_A"));
        fx.Apply(new AssistantTextEvt("m", "partial", null, false, "a1"));
        fx.Apply(new FallbackEvt("Fable 5 declined; retried with Opus 5.5", "u7", ["a1"], "claude-opus-5-5"));
        Ok(fx.Items is [UserItem, ThoughtItem { Text: "Checking." }, FallbackItem { Retry: "u7" }] && fx.Model == "claude-opus-5-5", "fallback retracts and swaps the model");
        fx.Apply(new ArtifactsEvt([new("https://claude.ai/a/1", "Report")]));
        Ok(fx.Artifacts is [{ Title: "Report" }], "artifacts listed");
        fx.Apply(new ArtifactsEvt([]));
        Ok(fx.Artifacts.IsEmpty, "artifacts cleared");
        fx.Apply(new ToolUseEvt("toolu_P", PushTool, JsonDocument.Parse("""{"message":"Build ready","status":"proactive"}""").RootElement.Clone(), null));
        Ok(fx.Pushes == 1 && fx.LastPush == "Build ready", "push notification counted");
        fx.Apply(new ToolUseEvt("toolu_N", "Bash", input, null));
        fx.Apply(new ToolResultEvt("toolu_N", "held back", true, null, NotRun: true));
        fx.Apply(new ResultEvt("success", false, 0, 10, 1, 0, null, null, null, Denied: ["toolu_N"]));
        Ok(fx.Tool("toolu_N") is { State: ToolState.NotRun }, "held back by auto mode: not run, not denied");
        fx.Apply(new AssistantTextEvt("m", "x", null, false, "u", DateTimeOffset.UnixEpoch));
        Ok(fx.Items[^1] is TextItem { At: { } fxa } && fxa == DateTimeOffset.UnixEpoch, "text keeps the transcript time");

        var s = new LiveSession("id", "essai", @"C:\w", "default");

        const string apiErr = "API Error: Output blocked by content filtering policy";
        var ae = new LiveSession("ae", "ae", @"C:\w", "default");
        ae.Apply(new AssistantTextEvt("m", apiErr, null, true));
        ae.Apply(new AssistantTextEvt("m", apiErr, null, false));
        ae.Apply(new AssistantTextEvt("m", "Total cost: $0.01", null, true));
        Ok(ae.Items is [ApiErrorItem { Error.Raw: apiErr }, TextItem, TextItem], "api error item only for synthetic API Error text");
        ae.Apply(new AssistantTextEvt("m", "x", null, false, "u9"));
        Ok(ae.Items[^1] is TextItem { Uuid: "u9" }, "text item keeps the message uuid");

        var hk = new LiveSession("hk", "hk", @"C:\w", "default");
        hk.Apply(new HookEvt("Stop", "success", 0, ""));
        hk.Apply(new HookEvt("UserPromptSubmit", "success", 0, "context"));
        hk.Apply(new HookEvt("SessionStart:startup", "success", 0, "plugin prompt"));
        hk.Apply(new HookEvt("PreToolUse:Write", "error", 2, "nope"));
        hk.Apply(new HookEvt("SessionStart:resume", "error", 1, "boom"));
        Ok(hk.Items is [HookItem { Hook.Output: "context" }, HookItem { Hook.ExitCode: 2 }, HookItem { Hook.ExitCode: 1 }],
            "silent hooks and successful SessionStart hidden, output and errors kept");
        // claude 2.1.296: hook_started per hook, answered by hook_id; one left unanswered ends with the turn.
        hk.Apply(new HookStartedEvt("h1", "SessionStart:startup"));
        hk.Apply(new HookStartedEvt("h2", "SessionStart:startup"));
        hk.Apply(new HookEvt("SessionStart:startup", "success", 0, "", "h1"));
        Ok(hk.RunningHooks.Count == 1 && hk.RunningHooks.ContainsKey("h2") && hk.Items.Count == 3, "running hooks: started until answered");
        hk.Apply(new ResultEvt("success", false, 0, 1, 1, 0, null, null, null));
        Ok(hk.RunningHooks.IsEmpty, "a result ends the hooks still listed");
        hk.Apply(new InitEvt("s", "", "", "", [], [], [], [], [], [], "off", null, OutputStyle: "Concise"));
        Ok(hk.OutputStyle == "Concise", "init output_style");

        // claude 2.1.296, two Agent calls in a row: each streams a successful SubagentStart (plugin context, plain or JSON)
        // between its tool_use and its result, then a silent PostToolBatch. None of it is shown, the agents share one Workflow.
        var sa = new LiveSession("sa", "sa", @"C:\w", "default");
        const string ctx = """{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":"PONYTAIL MODE ACTIVE"}}""";
        foreach (var id in new[] { "a1", "a2" })
        {
            sa.Apply(new ToolUseEvt(id, "Agent", input, null));
            sa.Apply(new HookEvt("SubagentStart:general-purpose", "success", 0, ctx));
            sa.Apply(new HookEvt("SubagentStart:general-purpose", "success", 0, "plain context"));
            sa.Apply(new ToolResultEvt(id, "ok", false, null));
            sa.Apply(new HookEvt("PostToolBatch", "success", 0, ""));
        }
        sa.Apply(new HookEvt("UserPromptSubmit", "success", 0, ctx.Replace("SubagentStart", "UserPromptSubmit")));
        sa.Apply(new HookEvt("SubagentStart:Explore", "error", 1, "boom"));
        Ok(sa.Items is [ToolItem, ToolItem, HookItem { Hook.Name: "SubagentStart:Explore" }]
           && ThreadBlocks.Of(sa.Items) is [ThreadBlocks.Run { Agents: true, Tools.Count: 2 }, ThreadBlocks.Hooks],
            "successful SubagentStart and context-only output hidden, a failing one kept");

        var cid = Guid.NewGuid().ToString(); var zid = Guid.NewGuid().ToString();
        try
        {
            var c = new LiveSession(cid, "c", @"C:\w", "default");
            c.OnEvent(JsonDocument.Parse("""{"type":"result","subtype":"success","is_error":false,"duration_ms":10,"num_turns":1,"total_cost_usd":0.0123}""").RootElement);
            Ok(TranscriptStore.RecordedCost(cid) == 0.0123m && c.CostUsd == 0.0123m, "result recorded in ledger");
            new LiveSession(zid, "z", @"C:\w", "default").OnEvent(JsonDocument.Parse("""{"type":"result","subtype":"success","is_error":false,"duration_ms":10,"num_turns":1,"total_cost_usd":0}""").RootElement);
            Ok(TranscriptStore.RecordedCost(zid) is null, "slash result not recorded");
        }
        finally { foreach (var i in new[] { cid, zid }) TranscriptStore.DeleteCost(i); }

        s.BeginTurn("crée b.txt");
        Ok(s.Status == SessionStatus.Running && s.TurnStartedAt is not null && s.Items is [UserItem { Text: "crée b.txt" }], "send");
        s.Apply(new TextDeltaEvt("je ", null));
        s.Apply(new TextDeltaEvt("crée", null));
        Ok(s.StreamingText == "je crée", "streaming text");
        var v = s.Version;
        s.OnEvent(JsonDocument.Parse("""{"type":"stream_event","event":{"type":"content_block_delta","delta":{"type":"text_delta","text":"!"}}}""").RootElement);
        Ok(s.StreamingText == "je crée!" && s.Version == v, "text delta leaves Version unchanged");
        s.Apply(new AssistantTextEvt("m1", "je crée", null, false));
        Ok(s.StreamingText == "" && s.Items[^1] is TextItem { Markdown: "je crée" }, "text block replaces stream");
        s.Apply(new ToolUseEvt("t1", "Write", input, null));
        Ok(s.ToolCount == 1 && s.Items[^1] is ToolItem { State: ToolState.Running }, "tool_use");
        s.Apply(new PermissionEvt("r1", "Write", input, "b.txt", "t1", null));
        Ok(s.Status == SessionStatus.Waiting && s.Pending.Count == 1 && s.Items[^1] is ToolItem { State: ToolState.Waiting }, "permission");
        s.Apply(new CancelRequestEvt("other"));
        Ok(s.Pending.Count == 1, "a cancel for another request changes nothing");
        s.Apply(new CancelRequestEvt("r1"));
        Ok(s.Status == SessionStatus.Running && s.Pending.IsEmpty && s.Items[^1] is ToolItem { State: ToolState.Running }, "request withdrawn by the CLI");
        s.Apply(new RetryEvt(1, 10, 500, 529, "overloaded"));
        Ok(s.Retry is { Attempt: 1 }, "api_retry shown");
        s.Apply(new PermissionEvt("r1", "Write", input, "b.txt", "t1", null));
        Ok(s.Retry is null, "progress clears the retry");
        Ok(s.AskingAgent(s.Pending[0]) is null, "main thread: no asking agent");
        s.Resolve(s.Pending[0], Decision.Deny);
        Ok(s.Status == SessionStatus.Running && s.Pending.IsEmpty && s.Items[^1] is ToolItem { State: ToolState.Denied }, "deny");
        s.Apply(new ToolResultEvt("t1", "The user denied this tool use.", true, null));
        Ok(s.Items[^1] is ToolItem { State: ToolState.Denied, ResultText: "The user denied this tool use." }, "denied stays denied");
        s.Apply(new ToolUseEvt("tr", "Read", input, null));
        s.Apply(new ToolResultEvt("tr", "Permission to read b.txt has been denied.", true, null));
        s.Apply(new ResultEvt("success", false, 0.01m, 2400, 2, 41878, 1000000, "off", "sdk_opt_in_required", Denied: ["tr", "gone"]));
        Ok(s.Tool("tr") is { State: ToolState.Denied }, "a rule's denial marks the tool denied");
        Ok(s.Status == SessionStatus.Idle && s.CostUsd == 0.01m && s.LastTurn == TimeSpan.FromMilliseconds(2400) && s.ContextTokens == 41878, "result");

        s.BeginTurn("encore");
        s.Apply(new PermissionEvt("r2", "Edit", input, null, "nope", null));
        var r2 = s.Pending[0];
        s.Apply(new ResultEvt("error_during_execution", true, 0.03m, 900, 1, 0, null, null, null, "aborted_streaming"));
        Ok(s.CostUsd == 0.03m && s.LastTurnCostUsd == 0.02m, "cost assigned, not added");
        Ok(s.Pending.IsEmpty && s.Status == SessionStatus.Idle && s.LastResultSubtype == Interrupted && s.ContextTokens == 41878, "interrupted turn");
        s.Resolve(r2, Decision.Allow);   // the reply landed after the turn ended
        Ok(s.Status == SessionStatus.Idle && s.Pending.IsEmpty, "late resolve is a no-op");
        s.Apply(new ResultEvt("success", false, 0, 10, 0, 0, null, null, null));
        Ok(s.CostUsd == 0.03m, "slash command result keeps cost");

        // Esc before any answer: the prompt (and its images) go back to the Composer; once something came back, it stays.
        var ir = new LiveSession("ir", "ir", "/w", "default");
        var shot = Images.From(CliProbe.DigitsPng("7"))!;
        ir.BeginTurn("fix it", [shot], "u9");
        ir.Apply(new HookEvt("UserPromptSubmit", "success", 0, ""));
        ir.Apply(new ResultEvt("error_during_execution", true, 0, 10, 0, 0, null, null, null, "aborted_streaming"));
        Ok(ir.Unanswered() is { Uuid: "u9" }, "interrupted before an answer: restorable");
        ir.CutBack("u9", "fix it");
        Ok(ir.Draft == "fix it" && ir.DraftImages is [{ MediaType: "image/png" }] && ir.Items.IsEmpty, "prompt and images back in the Composer");
        ir.BeginTurn("go", null, "u10");
        ir.Apply(new AssistantTextEvt("m", "partial", null, false));
        ir.Apply(new ResultEvt("error_during_execution", true, 0, 10, 0, 0, null, null, null, "aborted_streaming"));
        Ok(ir.Unanswered() is null, "an answered turn stays in the thread");
        ir.Apply(new SuggestionEvt("run the tests"));
        Ok(ir.Suggestion == "run the tests", "prompt suggestion kept");
        ir.Apply(new ResetEvt());
        Ok(ir.Suggestion is null, "prompt suggestion dropped on /clear");
        ir.Apply(new SuggestionEvt("run the tests"));
        ir.CutBack("u10", null);
        Ok(ir.Suggestion is null, "prompt suggestion dropped on rewind");

        // Messages sent mid-turn, replayed as the command_lifecycle frames --probe-cli queue captured.
        void Enqueue(string uuid, string text) { s.Queued = s.Queued.Add(new(uuid, text, DateTimeOffset.Now)); s.Apply(new QueueEvt(uuid, "queued")); }
        ResultEvt Done(string? reason = null) => new("success", false, 0.04m, 10, 1, 0, null, null, null, reason);
        s.BeginTurn("un"); Enqueue("u2", "deux");
        Ok(s.Queued is [{ Text: "deux" }] && s.Items[^1] is UserItem { Text: "un" }, "queued message waits outside the thread");
        s.Apply(Done());
        Ok(s.Status == SessionStatus.Running && s.TurnStartedAt is not null, "text-only turn: queued message runs next");
        s.Apply(new QueueEvt("u1", "completed")); s.Apply(new QueueEvt("u2", "started"));
        Ok(s.Queued.IsEmpty && s.Items[^1] is UserItem { Text: "deux" }, "started: joins the thread");
        s.Apply(Done());
        Ok(s.Status == SessionStatus.Idle && s.TurnStartedAt is null, "queued turn done");

        s.BeginTurn("trois"); Enqueue("u4", "quatre");
        s.Apply(new ToolUseEvt("t4", "Bash", input, null));
        s.Apply(new ToolResultEvt("t4", "one", false, null));
        s.Apply(new QueueEvt("u4", "started"));
        Ok(s.Items[^2] is ToolItem { Id: "t4" } && s.Items[^1] is UserItem { Text: "quatre" }, "folded after the tool result");
        s.Apply(new QueueEvt("u4", "completed")); s.Apply(Done());
        Ok(s.Status == SessionStatus.Idle && s.Queued.IsEmpty, "one result for the folded turn");

        s.BeginTurn("cinq"); Enqueue("u6", "six");
        s.Apply(new QueueEvt("u6", "cancelled"));
        Ok(s.Queued.IsEmpty && s.Status == SessionStatus.Running && s.Items[^1] is UserItem { Text: "cinq" }, "cancelled before start");
        s.Apply(Done());
        Ok(s.Status == SessionStatus.Idle, "cancelled: no extra turn");

        s.BeginTurn("sept"); Enqueue("u8", "huit");
        s.Apply(Done("aborted_streaming"));
        Ok(s.Status == SessionStatus.Running && s.LastResultSubtype == Interrupted && s.Queued.Count == 1, "interrupt keeps the queue");
        s.Apply(new QueueEvt("u8", "started"));
        Ok(s.Items[^1] is UserItem { Text: "huit" }, "queued message runs after the interrupt");
        s.Apply(Done());
        Enqueue("u9", "neuf");   // a result raced the cancel: the CLI's terminal frame must not leave the session busy
        s.Status = SessionStatus.Running; s.Apply(Done()); s.Apply(new QueueEvt("u9", "discarded"));
        Ok(s.Status == SessionStatus.Idle && s.TurnStartedAt is null, "draining ends when the queue empties");

        s.BeginTurn("dix"); Enqueue("u11", "onze");   // Esc while a permission waits: the queued turn runs, not "waiting"
        s.Apply(new PermissionEvt("r10", "Bash", input, null, null, null));
        s.Apply(Done("aborted_streaming"));
        Ok(s.Status == SessionStatus.Running && s.Pending.IsEmpty, "interrupt while waiting: queued turn runs");
        s.Apply(new QueueEvt("u11", "started"));
        Ok(s.Status == SessionStatus.Running && s.Items[^1] is UserItem { Text: "onze", Ultracode: false }, "started after a waiting interrupt");
        s.Apply(Done());

        s.turnUltra = true; s.BeginTurn("douze"); Enqueue("u13", "treize"); Enqueue("u14", "quatorze");
        s.Apply(new ToolUseEvt("t12", "Bash", input, null)); s.Apply(new ToolResultEvt("t12", "ok", false, null));
        s.Apply(new QueueEvt("u13", "started"));
        Ok(s.Items[^1] is UserItem { Text: "treize", Ultracode: true }, "folded into an ultracode turn: labelled ultracode");
        s.Apply(Done()); s.turnUltra = false;   // OnEvent resets it after the reducer
        s.Apply(new QueueEvt("u14", "started"));
        Ok(s.Items[^1] is UserItem { Text: "quatorze", Ultracode: true }, "started right after an ultracode turn: labelled ultracode");
        s.Apply(Done());
        s.BeginTurn("quinze"); Enqueue("u16", "seize"); s.Apply(Done()); s.Apply(new QueueEvt("u16", "started"));
        Ok(s.Items[^1] is UserItem { Ultracode: false }, "after a plain turn: not ultracode");
        s.Apply(Done());

        // Rewind: the CLI 2.1.296 dry_run answers (--probe-cli rewind-files), then the thread cut at the target message.
        var pv = RewindPreview.Parse(JsonDocument.Parse("""{"canRewind":true,"filesChanged":["/w/notes.txt","/w/keep.txt"],"insertions":1,"deletions":2}""").RootElement);
        Ok(pv is { CanRewind: true, Files: ["/w/notes.txt", "/w/keep.txt"], Insertions: 1, Deletions: 2, Error: null }, "rewind preview");
        Ok(RewindPreview.Parse(JsonDocument.Parse("""{"canRewind":false,"error":"No file checkpoint found for this message."}""").RootElement)
            is { CanRewind: false, Files: [], Error: "No file checkpoint found for this message." }, "rewind preview error");
        var rw = new LiveSession("r", "r", @"C:\w", "default");
        rw.BeginTurn("un", uuid: "u1");
        rw.Apply(new AssistantTextEvt("m", "ok", null, false));
        rw.BeginTurn("deux", uuid: "u2");
        rw.Apply(new ToolUseEvt("t9", "Write", input, null));
        rw.Apply(new ResultEvt("success", false, 0.01m, 10, 1, 0, null, null, null));
        rw.BeginTurn("trois", uuid: "u3");
        Ok(rw.RewindSteps("u2") is ["u3", "u2"] && rw.RewindSteps("u1") is ["u3", "u2", "u1"] && rw.RewindSteps("u3") is ["u3"],
            "rewind_conversation steps: newest back to the target");
        rw.Truncate("u2");
        Ok(rw.Items is [UserItem { Uuid: "u1" }, TextItem], "rewind cuts from the target message");
        // A slash command comes back as typed, not as the CLI's prefillText XML, and ahead of what the box already holds.
        const string compactXml = "<command-message>compact</command-message>\n<command-name>/compact</command-name>\n<command-args>keep tests</command-args>";
        var cb = new LiveSession("c", "c", @"C:\w", "default") { Draft = "unsent" };
        cb.BeginTurn("/compact keep tests", uuid: "c1");
        cb.CutBack("c1", compactXml);
        Ok(cb.Draft == "/compact keep tests\n\nunsent" && cb.Items.Count == 0, "rewind draft is the typed text, ahead of the unsent one");
        var cx = new LiveSession("c", "c", @"C:\w", "default");
        cx.CutBack("gone", compactXml);
        Ok(cx.Draft == "/compact keep tests", "rewind draft normalises prefillText when the message is not in the thread");
        // Sent while the automatic restore awaited its reply: the CLI holds it, so it stays.
        var rs = new LiveSession("r", "r", "/w", "default");
        rs.BeginTurn("stop", uuid: "s1"); rs.BeginTurn("new", uuid: "s2"); rs.Apply(new AssistantTextEvt("m", "ok", null, false));
        rs.CutBack("s1", null, ["s1"]);
        Ok(rs.Draft == "stop" && rs.Items is [UserItem { Uuid: "s2" }, TextItem], "rewind keeps a message sent meanwhile");
        Ok(!rw.CanRewind((UserItem)rw.Items[0]), "no rewind without a process that knows the uuid");
        Ok(RewindPreview.Parse(JsonDocument.Parse("""{"canRewind":true,"filesChanged":[null,"","/w/a"]}""").RootElement).Files is ["/w/a"],
            "rewind preview drops empty paths");
        Ok(RewindPreview.Parse(JsonDocument.Parse("""{"canRewind":true,"insertions":3,"deletions":0}""").RootElement).Changes
           && !RewindPreview.Parse(JsonDocument.Parse("""{"canRewind":true,"filesChanged":[]}""").RootElement).Changes
           && !RewindPreview.Parse(JsonDocument.Parse("""{"canRewind":false,"insertions":3}""").RootElement).Changes,
            "rewind preview: changes without the file list still count");

        // Opened from Récentes at 0.0127; --resume restores that cost-state and reports 0.0165 after one turn.
        var o = new LiveSession("o", "o", @"C:\w", "default", resumable: true) { CostUsd = 0.0127m };
        o.costBase = o.CostUsd - 0.0127m;
        o.Apply(new ResultEvt("success", false, 0.0165m, 10, 1, 0, null, null, null));
        Ok(o.CostUsd == 0.0165m && o.LastTurnCostUsd == 0.0038m, "resumed cost not counted twice");
        var ag = new LiveSession("a", "a", @"C:\w", "default");
        var recv = JsonDocument.Parse("""{"isAsync":true,"status":"async_launched","agentId":"aa5"}""").RootElement.Clone();
        ag.BeginTurn("agent");
        ag.Apply(new ToolUseEvt("toolu_A", "Agent", input, null));
        ag.Apply(new ToolResultEvt("toolu_A", "Async agent launched", false, recv));
        Ok(ag.Items[^1] is ToolItem { State: ToolState.Running, EndedAt: null, Background: true, TaskId: "aa5" }, "async agent stays running");
        ag.Apply(new ResultEvt("success", false, 0, 10, 1, 0, null, null, null));
        Ok(ag.Items[^1] is ToolItem { State: ToolState.Running }, "background row survives the turn");
        var info = new PermissionInfo("aa5", null, null, null, null, null, null, false);
        Ok(ag.AskingAgent(new("rq", "Write", input, null, "toolu_X", null, default, info)) is { Id: "toolu_A" }, "background subagent's request names its agent");
        var n = ag.Items.Count;
        ag.Apply(new TaskDoneEvt("aa5", "toolu_A", "completed", "pong", 31599, 1, 5088));
        var at = (ToolItem)ag.Items[^1];
        Ok(ag.Items.Count == n && at is { State: ToolState.Done, ResultText: "pong", Tokens: 31599, SubToolUses: 1 }
           && at.EndedAt - at.StartedAt == TimeSpan.FromMilliseconds(5088), "notification completes the row");
        var ag2 = new LiveSession("b", "b", @"C:\w", "default");
        ag2.Apply(new ToolUseEvt("toolu_B", "Agent", input, null));
        ag2.Apply(new ToolResultEvt("toolu_B", "Async agent launched", false, recv));
        ag2.EndTools(DateTimeOffset.Now, false);
        Ok(ag2.Items[^1] is ToolItem { State: ToolState.Error }, "process exit ends background row");

        // Workflow (claude 2.1.296, --probe-cli workflow): task_started, the async receipt, snapshots, the launching turn's
        // result mid-run, a progress line without a snapshot, then the notification.
        var wf = new LiveSession("w", "w", "/w", "default");
        var snap = JsonDocument.Parse("""[{"type":"workflow_phase","index":1,"title":"Ping"}]""").RootElement.Clone();
        wf.BeginTurn("wf");
        wf.Apply(new ToolUseEvt("toolu_W", WorkflowRuns.Tool, input, null));
        wf.Apply(new TaskStartedEvt("w0wlsjjl5", "toolu_W", "Two agents reply pong", "", "local_workflow"));
        wf.Apply(new ToolResultEvt("toolu_W", "Workflow launched in background. Task ID: w0wlsjjl5", false,
            JsonDocument.Parse("""{"status":"async_launched","taskId":"w0wlsjjl5","taskType":"local_workflow","workflowName":"pong-probe","runId":"wf_8e162ae6-279"}""").RootElement.Clone()));
        wf.Apply(new TaskProgressEvt("w0wlsjjl5", 0, 0, 20, snap));
        wf.Apply(new ResultEvt("success", false, 0, 10, 1, 0, null, null, null));
        wf.Apply(new TaskProgressEvt("w0wlsjjl5", 49076, 0, 3483));
        Ok(wf.Items[^1] is ToolItem { State: ToolState.Running, Background: true, TaskId: "w0wlsjjl5", Tokens: 49076, Progress: not null, ResultText: null }, "workflow runs past its turn, snapshot kept");
        wf.Apply(new TaskDoneEvt("w0wlsjjl5", "toolu_W", "completed", null, 49076, 0, 3493));
        Ok(wf.Items[^1] is ToolItem { State: ToolState.Done, TaskStatus: "completed", Progress: not null }, "workflow notification");

        // ExitPlanMode (2.1.296): "keep planning" denies with the feedback; an approval's mode comes back as system/status.
        var plan = JsonDocument.Parse("""{"plan":"# P\n\n1. a","planFilePath":"/p.md"}""").RootElement.Clone();
        Ok(ClaudeSession.Reply(false, plan, null, "  add tests  ")["message"]?.GetValue<string>() == "add tests"
           && ClaudeSession.Reply(false, plan, null, " ")["message"]?.GetValue<string>() == "The user denied this tool use."
           && ClaudeSession.Reply(true, plan, null, "x")["message"] is null, "deny message = feedback");
        var pm = new LiveSession("p", "p", @"C:\w", "plan");
        pm.BeginTurn("plan it");
        pm.Apply(new ToolUseEvt("toolu_P", "ExitPlanMode", plan, null));
        pm.Apply(new PermissionEvt("rp", "ExitPlanMode", plan, null, "toolu_P", null));
        pm.Resolve(pm.Pending[0], Decision.Allow);
        pm.Apply(new StatusEvt(null, "acceptEdits"));
        Ok(pm.Mode == "acceptEdits" && pm.Status == SessionStatus.Running && pm.Items[^1] is ToolItem { State: ToolState.Running }, "plan approved, mode follows status");
        var bare = new PendingPermission("r", AskUser.Tool, input, null, null, null);
        // An MCP elicitation queues like a permission and pauses the turn; a form one is never accepted without content.
        var el = new LiveSession("el", "el", @"C:\w", "default");
        el.BeginTurn("ask");
        var elForm = JsonDocument.Parse("""{"subtype":"elicitation","mcp_server_name":"probe","message":"Name?","mode":"form","requested_schema":{"type":"object","properties":{"name":{"type":"string"}}}}""").RootElement.Clone();
        el.Apply(new PermissionEvt("e1", Elicit.Tool, elForm, "Name?", null, null));
        Ok(el.Status == SessionStatus.Waiting && el.Pending is [{ Tool: Elicit.Tool, Description: "Name?" }], "elicitation pending");
        Ok(ThrowsMsg(() => el.Answer(el.Pending[0], Decision.Allow)) == Strings.Get("Ask.NeedsAnswers"), "form elicitation never accepted bare");
        el.Apply(new CancelRequestEvt("e1"));
        Ok(el.Pending.IsEmpty && el.Status == SessionStatus.Running, "elicitation withdrawn by the CLI");
        // A plugin's status line is replaced, then cleared; commands_changed replaces initialize's commands; a reload
        // (reload_plugins + reload_skills, 2.1.296 shapes trimmed) refreshes the plugin, agent, skill and MCP lists.
        el.Apply(new PluginStatusEvt("demo", "syncing"));
        el.Apply(new PluginStatusEvt("demo", "synced"));
        Ok(el.PluginStatus is { Count: 1 } && el.PluginStatus["demo"] == "synced", "plugin status replaced");
        el.Apply(new PluginStatusEvt("demo", null));
        Ok(el.PluginStatus.IsEmpty, "plugin status cleared");
        var gone = new LiveSession("g", "g", "/w", "default");
        gone.Apply(new PluginStatusEvt("demo", "syncing"));
        gone.OnExit(null, 1, "x");
        Ok(gone.PluginStatus.IsEmpty, "plugin status dropped when the process ends");
        el.InitializeInfo = JsonDocument.Parse("""{"commands":[{"name":"old"}],"agents":[],"models":[{"value":"haiku"}]}""").RootElement;
        el.Apply(new CommandsEvt(JsonDocument.Parse("""[{"name":"demo:hello","description":"say hi"}]""").RootElement));
        Ok(Events.Prop(el.InitializeInfo.Value, "commands")?[0].GetProperty("name").GetString() == "demo:hello" && Events.Prop(el.InitializeInfo.Value, "models") is not null, "commands_changed");
        el.Apply(new InitEvt("s", "", "", "", [], [], [], ["old"], [], ["gone"], "off", null));
        el.Reloaded(JsonDocument.Parse("""{"commands":[],"agents":[{"name":"claude","description":"Catch-all"}],"plugins":[{"name":"context7","path":"/p","source":"context7@claude-plugins-official"}],"mcpServers":[{"name":"plugin:context7:context7","status":"needs-auth","config":{"type":"http","url":"https://mcp.context7.com/mcp"},"scope":"dynamic","source":"plugin"}],"error_count":1}""").RootElement,
            JsonDocument.Parse("""{"skills":[{"name":"demo:hello","description":"say hi","argumentHint":""}]}""").RootElement);
        Ok(el.Init is { Plugins: ["context7"], Agents: ["claude"], Skills: ["demo:hello"] } && Mcp.Names(el).SequenceEqual(["plugin:context7:context7"])
           && Events.Prop(el.InitializeInfo.Value, "agents")?.GetArrayLength() == 1, "reload refreshes the lists");
        el.Apply(new NoticeEvt("Error compacting conversation", "error"));
        Ok(el.Items[^1] is NoticeItem { Notice.Color: "error" }, "notification joins the thread");
        Ok(ThrowsMsg(() => s.Answer(bare, Decision.Allow)) == Strings.Get("Ask.NeedsAnswers")
           && ThrowsMsg(() => s.Answer(bare, Decision.Deny)) == Strings.Get("Session.NoProcess"), "AskUserQuestion is never allowed bare");
        // Background shell: launch receipt, stop, process exit; the agent path above is untouched by it.
        var bg = new LiveSession("bg", "bg", @"C:\w", "default");
        var cmd = JsonDocument.Parse("""{"command":"sleep 60","run_in_background":true}""").RootElement.Clone();
        bg.BeginTurn("bg");
        bg.Apply(new ToolUseEvt("toolu_S", "Bash", cmd, null));
        bg.Apply(new TaskStartedEvt("b1", "toolu_S", "ticks", "", "local_bash", true));
        bg.Apply(new TaskStartedEvt("a1", "toolu_X", "agent", "general-purpose", "local_agent", true));
        bg.Apply(new ToolResultEvt("toolu_S", "Command running in background with ID: b1.", false, null));
        bg.Apply(new ResultEvt("success", false, 0, 10, 1, 0, null, null, null));
        Ok(bg.Tasks is [{ Id: "b1", ToolUseId: "toolu_S", Running: true, EndedAt: null }] && bg.Items[^1] is ToolItem { State: ToolState.Done, TaskId: "b1" },
            "background shell listed, survives the turn, agent tasks not listed");
        bg.Apply(new TaskUpdatedEvt("b1", null, null));
        Ok(bg.Tasks[0].Running, "status-less patch ignored");
        var stopAt = DateTimeOffset.FromUnixTimeMilliseconds(1791585746931);
        bg.Apply(new TaskUpdatedEvt("b1", "killed", stopAt));
        bg.Apply(new TaskDoneEvt("b1", "toolu_S", "stopped"));
        Ok(bg.Tasks is [{ Status: "killed", Running: false } k] && k.EndedAt == stopAt && bg.Items[^1] is ToolItem { State: ToolState.Done }, "stopped task");
        bg.Apply(new TaskStartedEvt("b2", "toolu_T", "watch", "", "local_bash", true));
        bg.Apply(new TaskStartedEvt("b3", "toolu_U", "fg", "", "local_bash", false));
        bg.EndTools(DateTimeOffset.Now, false);
        Ok(bg.Tasks is [{ Status: "killed" }, { Id: "b2", Status: "killed", EndedAt: not null }], "process exit ends running tasks, foreground shells not listed");
        bg.Apply(new TaskStartedEvt("b4", "toolu_V", "live", "", "local_bash", true));
        var cv = bg.Version;
        bg.ClearEndedTasks();
        Ok(bg.Tasks is [{ Id: "b4", Running: true }] && bg.Version != cv, "clear drops ended tasks only and bumps Version");
        // Foreground, then background_tasks (probed on claude 2.1.296): task_updated {is_backgrounded}, then the tool's result.
        bg.BeginTurn("fg");
        bg.Apply(new ToolUseEvt("toolu_F", "Bash", JsonDocument.Parse("""{"command":"make","description":"Build"}""").RootElement.Clone(), null));
        bg.Apply(new TaskStartedEvt("b5", "toolu_F", "Build", "", "local_bash", false));
        bg.Apply(new TaskUpdatedEvt("b5", null, null, Backgrounded: true));
        bg.Apply(new ToolResultEvt("toolu_F", "Command was manually backgrounded by user with ID: b5.", false, null));
        Ok(bg.Tasks is [_, { Id: "b5", Description: "Build", Running: true }] && bg.Items[^1] is ToolItem { Background: true }, "shell moved to the background joins the tasks");
        Ok(BgTask.Tail("tick 1\r\ntick 2\n\u001b[31mred\u001b[0m\n50%\r100%\n\n", 3) == "tick 2\nred\n100%" && BgTask.Tail("", 5) == "", "output tail");

        // Session lifecycle, on claude 2.1.296 shapes.
        var lockdown = new LiveSession("k", "k", "/w", "default") { Restricted = true, SafeMode = true }.Args(false);
        Ok(lockdown.Contains("--restricted") && lockdown.Contains("--safe-mode") && !new LiveSession("k", "k", "/w", "default").Args(false).Contains("--restricted"), "--restricted / --safe-mode");
        var wm = new LiveSession("w", "w", "/r", "default", worktree: "x");
        wm.BeginTurn("go");
        wm.Apply(new ToolUseEvt("toolu_E", "EnterWorktree", JsonDocument.Parse("""{"name":"y"}""").RootElement.Clone(), null));
        wm.Apply(new ToolResultEvt("toolu_E", "Created worktree at /r/.claude/worktrees/y on branch worktree-y.", false,
            JsonDocument.Parse("""{"worktreePath":"/r/.claude/worktrees/y","worktreeBranch":"worktree-y","message":"Created worktree at /r/.claude/worktrees/y on branch worktree-y."}""").RootElement.Clone()));
        Ok(wm.Cwd == "/r/.claude/worktrees/y", "EnterWorktree moves the session");
        wm.Apply(new ToolUseEvt("toolu_S", "EnterWorktree", JsonDocument.Parse("""{"path":"/r/.claude/worktrees/z"}""").RootElement.Clone(), "toolu_agent"));
        wm.Apply(new ToolResultEvt("toolu_S", "Entered worktree", false, JsonDocument.Parse("""{"worktreePath":"/r/.claude/worktrees/z","message":"Entered worktree"}""").RootElement.Clone()));
        Ok(wm.Cwd == "/r/.claude/worktrees/y", "a subagent's EnterWorktree moves only that agent");
        wm.Apply(new ToolUseEvt("toolu_X", "ExitWorktree", JsonDocument.Parse("""{"action":"keep"}""").RootElement.Clone(), null));
        wm.Apply(new ToolResultEvt("toolu_X", "Exited worktree.", false, JsonDocument.Parse("""{"action":"keep","originalCwd":"/r","worktreePath":"/r/.claude/worktrees/y","restoredCwd":"/r","message":"Exited worktree."}""").RootElement.Clone()));
        Ok(wm.Cwd == "/r" && wm.Worktree is null && !wm.Isolated, "ExitWorktree moves it back, no longer isolated");
        Ok(WorktreeMove("EnterWorktree", null) is null && WorktreeMove("Bash", JsonDocument.Parse("""{"worktreePath":"/x"}""").RootElement) is null, "other tools never move it");
        var rt = new LiveSession("z", "z", "/w", "default") { Items = [new UserItem("count", default, false)] };
        rt.Status = SessionStatus.Starting;
        rt.resumeTurn = true;
        rt.Apply(new QueueEvt("1967353c-03fe-4c2a-8c3b-72e2b6697258", "started"));
        rt.Apply(new StatusEvt("requesting", null));
        Ok(rt.Status == SessionStatus.Running && rt.Items is [UserItem, ResumedItem] && rt.TurnStartedAt is not null, "re-run of the cut-off turn starts a turn, labelled once");
        rt.Apply(new PrEvt("https://github.com/o/r/pull/42", "42"));
        Ok(rt.PrUrl == "https://github.com/o/r/pull/42", "code_change_published links the PR");
        var dl = Path.Combine(Path.GetTempPath(), "ccui-check-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(dl);
        try
        {
            File.WriteAllText(Path.Combine(dl, "a.txt"), "");
            Ok(FreeFile(dl, "a.txt") == Path.Combine(dl, "a-2.txt") && FreeFile(dl, "../../x.txt") == Path.Combine(dl, "x.txt") && FreeFile(dl, null) == Path.Combine(dl, "conversation.txt"),
               "export file: never over a file, never out of the folder");
        }
        finally { Directory.Delete(dl, true); }

        Ok(UserText("<command-message>cost</command-message>\n<command-name>/cost</command-name>\n<command-args></command-args>") == "/cost"
            && UserText("<local-command-stdout>Set model</local-command-stdout>") is null && UserText("<agent-message from=\"a\">\nreport\n</agent-message>") is null && UserText("salut") == "salut", "user text");
    }

    static string? ThrowsMsg(Func<Task> f)
    {
        try { f().GetAwaiter().GetResult(); return null; } catch (InvalidOperationException e) { return e.Message; }
    }

    // Echoed slash commands come back as tags: "<command-name>/x</command-name>…<command-args>a</command-args>" → "/x a"; their output is hidden.
    internal static string? UserText(string t)
    {
        if (t.StartsWith("[Request interrupted by user") || t.StartsWith("<local-command-") || t.StartsWith("<task-notification>")
            || t.StartsWith("<agent-message")) return null;   // a subagent's hand-back: model output, not the user
        if (System.Text.RegularExpressions.Regex.Match(t, "<command-name>(.*?)</command-name>") is not { Success: true } m) return t;
        var args = System.Text.RegularExpressions.Regex.Match(t, "<command-args>(.*?)</command-args>", System.Text.RegularExpressions.RegexOptions.Singleline).Groups[1].Value.Trim();
        return args.Length > 0 ? $"{m.Groups[1].Value} {args}" : m.Groups[1].Value;
    }
}
