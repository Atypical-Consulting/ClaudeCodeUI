using System.Collections.Immutable;
using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

public enum SessionStatus { Starting, Idle, Running, Waiting, Exited, Crashed }
public enum ToolState { Running, Done, Error, Waiting, Denied }
public enum Decision { Allow, AllowSession, Deny }

public abstract record Item;
public record UserItem(string Text, DateTimeOffset At, bool Ultracode, IReadOnlyList<UserImage>? Images = null, string? Uuid = null) : Item;   // Uuid: sent with the message (Send)
public record TextItem(string Markdown, string? ParentToolUseId, string? Uuid = null) : Item;   // Uuid: where "Fork from here" cuts
public record ApiErrorItem(ApiError Error) : Item;
public record ResetItem : Item;                                   // /clear went through: not rendered, it starts a new task list
public record HookItem(HookEvt Hook) : Item;   // a hook that failed, blocked or printed something
// Immutable like every item: an update replaces the instance in Items (LiveSession.Set), so a render never sees half of it.
public sealed record ToolItem(string Id, string Name, JsonElement Input, string? ParentToolUseId) : Item
{
    public ToolState State { get; init; }
    public string? ResultText { get; init; }
    public JsonElement? Structured { get; init; }
    public DateTimeOffset StartedAt { get; init; }
    public DateTimeOffset? EndedAt { get; init; }
    public string? TaskId { get; init; }          // Agent only
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
        var plain = System.Text.RegularExpressions.Regex.Replace(output, @"\x1B(\[[0-?]*[ -/]*[@-~]|\][^\x07\x1B]*(\x07|\x1B\\)|[@-_])", "");
        var all = plain.Split('\n').Select(l => l.TrimEnd('\r')).Select(l => l[(l.LastIndexOf('\r') + 1)..]).ToList();
        while (all.Count > 0 && all[^1].Length == 0) all.RemoveAt(all.Count - 1);
        return string.Join('\n', all.TakeLast(lines));
    }
}
public record PendingPermission(string RequestId, string Tool, JsonElement Input, string? Description, string? ToolUseId,
                                JsonElement? Suggestions, DateTimeOffset At = default);

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
    bool draining;                                             // a result came with messages still queued: the CLI runs the next one
    bool ultraCarry;                                           // ...and that result ended an ultracode turn
    ImmutableHashSet<string> sent = [];                        // UserItem.Uuid the current process knows (rewind targets)
    long lastNotify;
    int notifyQueued;
    int version;

    // Modes the UI offers. bypassPermissions / dontAsk are deliberately not offered.
    public static readonly string[] Modes = ["default", "acceptEdits", "plan", "auto"];
    // Modes that change files without asking: confirmed when the folder is not an isolated worktree.
    public static bool Risky(string mode) => mode is "auto" or "acceptEdits";
    public bool Isolated => Worktree is not null || TranscriptStore.RootOf(Cwd) != Cwd;

    public LiveSession(string id, string name, string cwd, string mode, string? worktree = null, string? model = null, string? effort = null, bool resumable = false)
    {
        Id = id; Name = name; Cwd = cwd; Mode = mode; Worktree = worktree; Model = model; Effort = effort;
        this.resumable = resumable;
        StartedAt = LastEventAt = DateTimeOffset.Now;
    }

    public string Id { get; }                        // == --session-id == .jsonl name == /session/{Id}
    public string Name { get; private set; }
    public string Cwd { get; private set; }          // replaced by init.cwd (-w)
    public string Mode { get; private set; }
    public string? Branch { get; private set; }
    public string? Repo { get; private set; }
    public string? Worktree { get; }
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
    public JsonElement? InitializeInfo { get; private set; }  // "initialize" response: models, commands, agents, account
    public string? Model { get; private set; }
    public string? Effort { get; private set; }
    public bool Ultracode { get; private set; }               // toggle "for this turn"
    public string FastModeState { get; private set; } = "off";
    public string? FastModeReason { get; private set; }
    public JsonElement? Context { get; private set; }          // raw get_context_usage
    public JsonElement? McpStatus { get; private set; }       // raw mcp_status
    public ImmutableList<BgTask> Tasks { get; private set; } = [];   // background shells and Monitors, in start order
    public RateLimitEvt? Limits { get; private set; }
    public DateTimeOffset LimitsAt { get; private set; }
    public bool HasProcess => proc is not null;
    public string Draft { get; set; } = "";          // the Composer's unsent text: survives navigation, reloads and reconnects
    public IReadOnlyList<UserImage> DraftImages { get; set; } = [];   // and its attached images, likewise
    public List<TodoEntry> Todos => TodoList.From(Items);   // recomputed per read: one pass over Items, cheaper than keeping a cache in sync
    public string? ForkOf { get; init; }   // forked from this session id: --resume ForkOf --fork-session until it has its own transcript
    public string? ForkAt { get; init; }   // --resume-session-at: the message uuid the fork stops after (null = the whole conversation)
    // The CLI forks the persisted transcript: none before the first message, and only part of a turn still running.
    // A fork not sent to yet has items but no transcript: SessionManager.Fork then forks its source instead.
    public bool CanFork => Items.Count > 0 && Status is not (SessionStatus.Running or SessionStatus.Waiting);

    public event Action? Changed;

    // ---------- commands ----------

    public async Task Send(string text, IReadOnlyList<UserImage>? images = null)
    {
        var p = EnsureProcess();
        var uuid = Guid.NewGuid().ToString();
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
    }

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
        try
        {
            foreach (var id in RewindSteps(u.Uuid!))
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
                lock (gate) CutBack(cut, prefill);
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
    internal void CutBack(string uuid, string? prefill)
    {
        if ((Items.OfType<UserItem>().FirstOrDefault(x => x.Uuid == uuid)?.Text ?? (prefill is null ? null : UserText(prefill))) is { } back)
            Draft = string.IsNullOrWhiteSpace(Draft) ? back : back + "\n\n" + Draft;
        Truncate(uuid);
    }

    // The rewind_conversation targets for a rewind to `uuid`: it and every message sent after it, newest first.
    internal IReadOnlyList<string> RewindSteps(string uuid) =>
        [.. Items.OfType<UserItem>().SkipWhile(x => x.Uuid != uuid).Select(x => x.Uuid).OfType<string>().Reverse()];

    // cancel_async_message drops a message the CLI has not started: {cancelled:true}, then a `cancelled` frame removes the
    // chip (verified by --probe-cli queue). A cancel that loses the race to `started` is not probed: the chip leaves on that frame.
    public Task Cancel(QueuedMessage q) => Request("cancel_async_message", new() { ["message_uuid"] = q.Uuid });

    // mode: set_permission_mode before an allow (ExitPlanMode's approvals). message: a deny's text for the model ("keep planning").
    // input: what to allow the tool with instead of its own input (AskUserQuestion's answers, see AskUser).
    public async Task Answer(PendingPermission p, Decision d, string? mode = null, string? message = null, JsonElement? input = null)
    {
        // A bare allow makes AskUserQuestion return "The user did not answer the questions." (see AskUser).
        if (p.Tool == AskUser.Tool && d != Decision.Deny && input is null) throw new InvalidOperationException(Strings.Get("Ask.NeedsAnswers"));
        var c = proc ?? throw new InvalidOperationException(Strings.Get("Session.NoProcess"));
        // The request stays in Pending (its card stays up) until the CLI has its reply; a second answer meanwhile is a no-op.
        lock (gate) if (!answering.Add(p.RequestId)) return;
        try
        {
            if (d == Decision.Deny) await c.Respond(p.RequestId, false, p.Input, message: message);
            else
            {
                JsonNode? updated = null;
                if (d == Decision.AllowSession && p.Suggestions is { ValueKind: JsonValueKind.Array } sg)
                {
                    var setMode = sg.EnumerateArray().FirstOrDefault(s => Events.Str(s, "type") == "setMode");
                    if (Events.Str(setMode, "mode") is { } m) mode = m;
                    else updated = JsonNode.Parse(sg.GetRawText());   // verified by --probe-cli permission-session
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

    public async Task Interrupt()
    {
        if (proc is { } p) await p.Interrupt();
    }

    // Relaunch after a crash or open a past session: --resume when a transcript exists, else a fresh start with the same id.
    public async Task Restart()
    {
        if (proc is { } old) { proc = null; await old.DisposeAsync(); }
        EnsureProcess();
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

    public Task SetFast(bool on) => Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["fastMode"] = on } });

    public Task SetUltracode(bool on)
    {
        Ultracode = on;
        Notify();
        return Task.CompletedTask;
    }

    public async Task RefreshContext() { Context = await Request("get_context_usage"); Notify(); }
    public async Task RefreshMcp() { McpStatus = await Request("mcp_status"); Notify(); }

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
    internal ClaudeSession EnsureProcess()
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
                s = new ClaudeSession(Cwd, args, OnEvent, (code, text) => OnExit(s, code, text),
                    l => { if (Status == SessionStatus.Starting) Console.Error.WriteLine($"[{Id}] boot {l}"); });
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
        var args = new List<string> { "--permission-mode", Mode == "default" ? "manual" : Mode, "--include-partial-messages", "--forward-subagent-text",
            TodoList.AllowedToolsArg, "--include-hook-events" };
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
            Console.WriteLine($"[{Id}] initialize: {Count(info, "models")} models, {Count(info, "commands")} commands");
            var applied = Events.Prop(await p.Request("get_settings"), "applied");
            if (applied is { } a)
            {
                Effort = Events.Str(a, "effort") ?? Effort;
                Model ??= Events.Str(a, "model");
            }
            if (Events.Prop(await p.Request("get_usage"), "rate_limits") is { } rl
                && Events.Prop(rl, "five_hour") is { } h5 && Events.Prop(rl, "seven_day") is { } d7 && Limits is null)
            {
                Limits = new(Pct(h5), Reset(h5), Pct(d7), Reset(d7));   // get_usage is 0..100, normalised to 0..1
                LimitsAt = DateTimeOffset.Now;
            }
        }
        catch (Exception ex) { Console.Error.WriteLine($"[{Id}] startup: {ex.Message}"); }
        lock (gate) if (Status == SessionStatus.Starting) Status = SessionStatus.Idle;
        Notify();

        static int Count(JsonElement e, string n) => Events.Prop(e, n) is { ValueKind: JsonValueKind.Array } x ? x.GetArrayLength() : 0;
        static double Pct(JsonElement w) => Events.Prop(w, "utilization") is { ValueKind: JsonValueKind.Number } u ? u.GetDouble() / 100 : 0;
        static DateTimeOffset Reset(JsonElement w) => DateTimeOffset.TryParse(Events.Str(w, "resets_at"), out var t) ? t : default;
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
            if (e is InitEvt && (Branch is null || cwd != Cwd)) _ = RefreshGit();
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
            Status = SessionStatus.Crashed;
            ExitCode = code; ExitText = text; StartFailed = s is null;
            Pending = [];
            Queued = []; draining = false;
            TurnStartedAt = null;
            ClearStream();
            EndTools(DateTimeOffset.Now, false);
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

    static async Task<string?> Git(string dir, params string[] args)
    {
        try
        {
            var psi = new ProcessStartInfo("git") { RedirectStandardOutput = true, RedirectStandardError = true };
            psi.ArgumentList.Add("-C"); psi.ArgumentList.Add(dir);
            foreach (var a in args) psi.ArgumentList.Add(a);
            using var p = Process.Start(psi)!;
            var output = await p.StandardOutput.ReadToEndAsync();
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

    // The message with this uuid and everything after it (its tools, answers, later turns) leave the thread.
    internal void Truncate(string uuid)
    {
        var i = Items.FindIndex(x => x is UserItem u && u.Uuid == uuid);
        if (i >= 0) Items = Items.RemoveRange(i, Items.Count - i);
    }

    internal void BeginTurn(string text, IReadOnlyList<UserImage>? images = null, string? uuid = null)
    {
        Items = Items.Add(new UserItem(text, DateTimeOffset.Now, Ultracode, images, uuid));
        Status = SessionStatus.Running;
        TurnStartedAt = DateTimeOffset.Now;
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
        switch (e)
        {
            case InitEvt i:
                Init = i;
                if (i.Cwd.Length > 0) Cwd = i.Cwd;
                if (i.Model.Length > 0) Model = i.Model;
                if (i.PermissionMode.Length > 0) Mode = i.PermissionMode;
                FastModeState = i.FastModeState;
                FastModeReason = i.FastModeReason;
                break;

            case StatusEvt { PermissionMode: { } pm }:
                Mode = pm;
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
                Items = Items.Add(new TextItem(a.Text, a.ParentToolUseId, a.Uuid));
                break;

            case ToolUseEvt u:
                if (u.ParentToolUseId is null) ClearStream();
                Items = Items.Add(new ToolItem(u.Id, u.Name, u.Input, u.ParentToolUseId) { State = ToolState.Running, StartedAt = now });
                ToolCount++;
                break;

            case ToolResultEvt { Structured: { } rs } r when Tool(r.ToolUseId) is { State: ToolState.Running } t && ToolKinds.IsAgent(t.Name)
                                                              && Events.Str(rs, "status") == "async_launched":
                // launch receipt, not the outcome: stays Running until the task-notification
                Set(t, t with { Background = true, TaskId = Events.Str(rs, "agentId") ?? t.TaskId, Structured = rs });
                break;

            case ToolResultEvt r when Tool(r.ToolUseId) is { } t:
                Set(t, t with
                {
                    State = r.IsError ? (t.State == ToolState.Denied ? ToolState.Denied : ToolState.Error) : ToolState.Done,
                    ResultText = r.Text,
                    Structured = r.Structured,
                    EndedAt = now,
                    Tokens = r.Structured is { } st && Events.Prop(st, "totalTokens") is { ValueKind: JsonValueKind.Number } tok ? tok.GetInt64() : t.Tokens,
                });
                break;

            case UserTextEvt u when UserText(u.Text) is { } text:
                Items = Items.Add(new UserItem(text, u.At ?? now, false, u.Images));
                break;

            case PermissionEvt p:
                Pending = Pending.Add(new(p.RequestId, p.Tool, p.Input, p.Description, p.ToolUseId, p.Suggestions, now));
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
                if (r.ContextTokens > 0) ContextTokens = r.ContextTokens;
                if (r.ContextWindow is { } w) ContextWindow = w;
                if (r.FastModeState is { } fs) { FastModeState = fs; FastModeReason = r.FastModeReason; }
                EndTools(now);
                Pending = [];
                ClearStream();
                ThinkingTokens = 0;
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
                Limits = l;
                LimitsAt = now;
                break;

            case TaskStartedEvt ts:
                if (Tool(ts.ToolUseId) is { } launcher) Set(launcher, launcher with { TaskId = ts.TaskId });
                if (ts is { TaskType: "local_bash", Backgrounded: true } && !Tasks.Any(x => x.Id == ts.TaskId))
                    Tasks = Tasks.Add(new(ts.TaskId, ts.ToolUseId, ts.Description, now));
                break;

            case TaskUpdatedEvt { Status: { } status } tu when Tasks.FirstOrDefault(x => x.Id == tu.TaskId) is { } bt:
                var ended = bt with { Status = status };
                Tasks = Tasks.Replace(bt, ended with { EndedAt = ended.Running ? null : tu.EndedAt ?? now });
                break;

            case TaskProgressEvt tp when Items.LastOrDefault(i => i is ToolItem t && t.TaskId == tp.TaskId) is ToolItem t:   // IList: scans from the end
                Set(t, t with { Tokens = tp.TotalTokens, SubToolUses = tp.ToolUses });
                break;

            case TaskDoneEvt td when (Tool(td.ToolUseId) ?? Items.LastOrDefault(i => i is ToolItem x && td.TaskId.Length > 0 && x.TaskId == td.TaskId) as ToolItem) is { } t
                                     && (t.State == ToolState.Running || t.Background):   // a resumed background agent notifies again
                Set(t, t with
                {
                    State = td.Status == "completed" ? ToolState.Done : ToolState.Error,
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

            case ResetEvt:
                Items = Items.Add(new ResetItem());
                break;
        }
    }

    // The history a fork shows: up to and including the text block `at` (what --resume-session-at keeps), else all of it.
    internal static ImmutableList<Item> Upto(IReadOnlyList<Item> items, string? at)
    {
        var i = at is null ? -1 : items.ToList().FindIndex(x => x is TextItem t && t.Uuid == at);
        return [.. i < 0 ? items : items.Take(i + 1)];
    }

    // A silent success is the common case (and most of the stream's hook events): nothing to show. Context a hook injects
    // into the prompt is for the model, not to read: a successful SessionStart (plugins: ~11 KB per start) or SubagentStart
    // (~5 KB per Agent call, measured on 2.1.296), and any output that is only hookSpecificOutput.additionalContext.
    internal static bool Shown(HookEvt h) =>
        h.Outcome != "success"
        || h.Output.Length > 0 && !h.Name.StartsWith("SessionStart") && !h.Name.StartsWith("SubagentStart") && !Events.ContextOnly(h.Output);

    // send → tool_use → permission → deny → result, replayed on the reducer only.
    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "LiveSession: " + what);
        var input = JsonDocument.Parse("""{"file_path":"C:\\w\\b.txt","content":"x"}""").RootElement.Clone();
        var fresh = new LiveSession("id", "n", @"C:\w", "default").Args(false);
        Ok(fresh is ["--permission-mode", "manual", _, _, TodoList.AllowedToolsArg, "--include-hook-events", "--session-id", "id", "--name", "n"], "fresh args");
        var resumed = new LiveSession("o", "o", @"C:\w", "plan", model: "haiku").Args(true);
        Ok(resumed is ["--permission-mode", "plan", _, _, _, _, "--resume", "o", "--model", "haiku"], "resume args");
        var fork = new LiveSession("f", "x (fork)", @"C:\w", "default") { ForkOf = "o", ForkAt = "u1" };
        Ok(fork.Args(false) is ["--permission-mode", "manual", _, _, TodoList.AllowedToolsArg, "--include-hook-events", "--resume", "o", "--fork-session", "--session-id", "f", "--name", "x (fork)", "--resume-session-at", "u1"], "fork args");
        Ok(fork.Args(true) is [_, _, _, _, _, _, "--resume", "f"], "fork resumes its own transcript once written");
        Item[] h = [new UserItem("a", default, false), new TextItem("b", null, "u1"), new UserItem("c", default, false), new TextItem("d", null, "u2")];
        Ok(Upto(h, "u1") is [UserItem, TextItem { Uuid: "u1" }] && Upto(h, null).Count == 4, "fork history cut");
        Ok(ClaudeSession.TraceLine(1234, "stderr x") == "+1234 ms stderr x", "trace line format");

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
        s.Resolve(s.Pending[0], Decision.Deny);
        Ok(s.Status == SessionStatus.Running && s.Pending.IsEmpty && s.Items[^1] is ToolItem { State: ToolState.Denied }, "deny");
        s.Apply(new ToolResultEvt("t1", "The user denied this tool use.", true, null));
        Ok(s.Items[^1] is ToolItem { State: ToolState.Denied, ResultText: "The user denied this tool use." }, "denied stays denied");
        s.Apply(new ResultEvt("success", false, 0.01m, 2400, 2, 41878, 1000000, "off", "sdk_opt_in_required"));
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
        Ok(BgTask.Tail("tick 1\r\ntick 2\n\u001b[31mred\u001b[0m\n50%\r100%\n\n", 3) == "tick 2\nred\n100%" && BgTask.Tail("", 5) == "", "output tail");

        Ok(UserText("<command-message>cost</command-message>\n<command-name>/cost</command-name>\n<command-args></command-args>") == "/cost"
            && UserText("<local-command-stdout>Set model</local-command-stdout>") is null && UserText("salut") == "salut", "user text");
    }

    static string? ThrowsMsg(Func<Task> f)
    {
        try { f().GetAwaiter().GetResult(); return null; } catch (InvalidOperationException e) { return e.Message; }
    }

    // Echoed slash commands come back as tags: "<command-name>/x</command-name>…<command-args>a</command-args>" → "/x a"; their output is hidden.
    internal static string? UserText(string t)
    {
        if (t.StartsWith("[Request interrupted by user") || t.StartsWith("<local-command-") || t.StartsWith("<task-notification>")) return null;
        if (System.Text.RegularExpressions.Regex.Match(t, "<command-name>(.*?)</command-name>") is not { Success: true } m) return t;
        var args = System.Text.RegularExpressions.Regex.Match(t, "<command-args>(.*?)</command-args>", System.Text.RegularExpressions.RegexOptions.Singleline).Groups[1].Value.Trim();
        return args.Length > 0 ? $"{m.Groups[1].Value} {args}" : m.Groups[1].Value;
    }
}
