using System.Collections.Immutable;
using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

public enum SessionStatus { Starting, Idle, Running, Waiting, Exited, Crashed }
public enum ToolState { Running, Done, Error, Waiting, Denied }
public enum Decision { Allow, AllowSession, Deny }

public abstract record Item;
public record UserItem(string Text, DateTimeOffset At, bool Ultracode) : Item;
public record TextItem(string Markdown, string? ParentToolUseId) : Item;
public sealed record ToolItem(string Id, string Name, JsonElement Input, string? ParentToolUseId) : Item
{
    public ToolState State { get; set; }
    public string? ResultText { get; set; }
    public JsonElement? Structured { get; set; }
    public DateTimeOffset StartedAt { get; set; }
    public DateTimeOffset? EndedAt { get; set; }
    public string? TaskId { get; set; }          // Agent only
    public long Tokens { get; set; }
    public int SubToolUses { get; set; }
    public bool Background { get; set; }         // Agent launched async: outlives its turn, ends on its task-notification
}
public record PendingPermission(string RequestId, string Tool, JsonElement Input, string? Description, string? ToolUseId,
                                JsonElement? Suggestions, DateTimeOffset At = default);

// One claude process and its state. The reducer (Apply) runs on the process reader thread; lists are
// immutable snapshots replaced on write, so components read them without locking.
public sealed class LiveSession : IAsyncDisposable
{
    readonly Lock gate = new();
    ClaudeSession? proc;
    readonly bool resumable; // opened from a transcript: always --resume
    bool turnUltra;
    int turns;                                                 // sent and not yet answered by a result (the CLI queues them)
    long lastNotify;
    int notifyQueued;

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
    public string StreamingText { get; private set; } = "";
    public int ThinkingTokens { get; private set; }
    public decimal CostUsd { get; internal set; }
    public decimal LastTurnCostUsd { get; private set; }
    decimal costBase;                                          // cost of earlier processes, minus what --resume restores
    public int ToolCount { get; internal set; }
    public long ContextTokens { get; private set; }
    public long? ContextWindow { get; private set; }
    public DateTimeOffset StartedAt { get; }
    public DateTimeOffset LastEventAt { get; private set; }
    public DateTimeOffset? TurnStartedAt { get; private set; }
    public TimeSpan? LastTurn { get; private set; }
    public string? LastResultSubtype { get; private set; }   // "interrompu" after an interrupt
    public DateTimeOffset? LastResultAt { get; private set; }
    public int? ExitCode { get; private set; }
    public string? ExitText { get; private set; }
    public InitEvt? Init { get; private set; }
    public JsonElement? InitializeInfo { get; private set; }  // "initialize" response: models, commands, agents, account
    public string? Model { get; private set; }
    public string? Effort { get; private set; }
    public bool Ultracode { get; private set; }               // toggle "pour ce tour"
    public string FastModeState { get; private set; } = "off";
    public string? FastModeReason { get; private set; }
    public JsonElement? Context { get; private set; }          // raw get_context_usage
    public JsonElement? McpStatus { get; private set; }       // raw mcp_status
    public RateLimitEvt? Limits { get; private set; }
    public DateTimeOffset LimitsAt { get; private set; }
    public bool HasProcess => proc is not null;

    public event Action? Changed;

    // ---------- commands ----------

    public async Task Send(string text)
    {
        var p = EnsureProcess();
        if (Ultracode) await p.Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["ultracode"] = true } });
        lock (gate) { turnUltra = Ultracode; BeginTurn(text); }
        Notify();
        await p.SendUser(text);
    }

    public async Task Answer(PendingPermission p, Decision d)
    {
        var c = proc ?? throw new InvalidOperationException("Aucun processus claude pour cette session.");
        lock (gate) Resolve(p, d);
        Notify();
        if (d == Decision.Deny) { await c.Respond(p.RequestId, false, p.Input); return; }
        JsonNode? updated = null;
        if (d == Decision.AllowSession && p.Suggestions is { ValueKind: JsonValueKind.Array } sg)
        {
            var setMode = sg.EnumerateArray().FirstOrDefault(s => Events.Str(s, "type") == "setMode");
            if (Events.Str(setMode, "mode") is { } mode) await c.Request("set_permission_mode", new() { ["mode"] = mode });
            else updated = JsonNode.Parse(sg.GetRawText());   // ponytail: unverified on 2.1.295 (plan §1.2)
        }
        await c.Respond(p.RequestId, true, p.Input, updated);
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

    // ---------- process ----------

    internal ClaudeSession EnsureProcess()
    {
        if (proc is { } p) return p;
        var resume = resumable || File.Exists(Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
                ".claude", "projects", TranscriptStore.Slug(Cwd), Id + ".jsonl"));
        var args = Args(resume);

        ClaudeSession s = null!;
        try
        {
            // --resume restores the last persisted cost-state: total_cost_usd counts on from it.
            costBase = CostUsd - (resume ? TranscriptStore.PersistedCost(Id) : 0);
            s = new ClaudeSession(Cwd, args, OnEvent, (code, text) => OnExit(s, code, text),
                l => { if (Status == SessionStatus.Starting) Console.Error.WriteLine($"[{Id}] boot {l}"); });
        }
        catch (Exception ex)
        {
            OnExit(null, -1, $"claude n'a pas pu démarrer : {ex.Message}");
            throw;
        }
        proc = s;
        ExitCode = null; ExitText = null;
        Status = SessionStatus.Starting;
        Notify();
        _ = Boot(s);
        return s;
    }

    // The exact list EnsureProcess launches claude with (also reused by --boot-probe).
    internal List<string> Args(bool resume)
    {
        var args = new List<string> { "--permission-mode", Mode == "default" ? "manual" : Mode, "--include-partial-messages", "--forward-subagent-text" };
        if (resume) args.AddRange(["--resume", Id]);
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
            var init = p.Request("initialize", null, 180);
            if (await Task.WhenAny(init, Task.Delay(60_000)) != init) Console.Error.WriteLine($"[{Id}] initialize : still waiting after 60 s");
            var info = await init;
            InitializeInfo = info;
            Console.WriteLine($"[{Id}] initialize : {Count(info, "models")} modèles, {Count(info, "commands")} commandes");
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
        catch (Exception ex) { Console.Error.WriteLine($"[{Id}] démarrage : {ex.Message}"); }
        lock (gate) if (Status == SessionStatus.Starting) Status = SessionStatus.Idle;
        Notify();

        static int Count(JsonElement e, string n) => Events.Prop(e, n) is { ValueKind: JsonValueKind.Array } x ? x.GetArrayLength() : 0;
        static double Pct(JsonElement w) => Events.Prop(w, "utilization") is { ValueKind: JsonValueKind.Number } u ? u.GetDouble() / 100 : 0;
        static DateTimeOffset Reset(JsonElement w) => DateTimeOffset.TryParse(Events.Str(w, "resets_at"), out var t) ? t : default;
    }

    Task OnEvent(JsonElement raw)
    {
        foreach (var e in Events.ParseAll(raw))
        {
            var cwd = Cwd;
            Apply(e);
            if (e is TextDeltaEvt) Throttled(); else Notify();
            if (e is InitEvt && (Branch is null || cwd != Cwd)) _ = RefreshGit();
            if (e is ResultEvt && turnUltra)
            {
                turnUltra = false;
                Ultracode = false;
                _ = Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["ultracode"] = false } })
                    .ContinueWith(t => Console.Error.WriteLine($"[{Id}] ultracode off : {t.Exception?.InnerException?.Message}"), TaskContinuationOptions.OnlyOnFaulted);
            }
        }
        return Task.CompletedTask;
    }

    void OnExit(ClaudeSession? s, int code, string text)
    {
        lock (gate)
        {
            if (s is not null && proc != s) return;
            proc = null;
            Status = SessionStatus.Crashed;
            ExitCode = code; ExitText = text;
            Pending = [];
            TurnStartedAt = null;
            StreamingText = "";
            EndTools(DateTimeOffset.Now, false);
        }
        Notify();
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
        var p = proc;
        proc = null;
        if (p is not null) await p.DisposeAsync();
        lock (gate)
        {
            if (Status != SessionStatus.Crashed) Status = SessionStatus.Exited;
            Pending = [];
            TurnStartedAt = null;
            EndTools(DateTimeOffset.Now, false);
        }
        Notify();
    }

    // ---------- notifications: at most one per 50 ms while text streams ----------

    void Notify()
    {
        Interlocked.Exchange(ref lastNotify, Environment.TickCount64);
        Changed?.Invoke();
    }

    void Throttled()
    {
        var wait = 50 - (Environment.TickCount64 - Interlocked.Read(ref lastNotify));
        if (wait <= 0) { Notify(); return; }
        if (Interlocked.Exchange(ref notifyQueued, 1) == 1) return;
        _ = Task.Delay((int)wait).ContinueWith(_ => { Interlocked.Exchange(ref notifyQueued, 0); Notify(); });
    }

    // ---------- reducer ----------

    internal void BeginTurn(string text)
    {
        Items = Items.Add(new UserItem(text, DateTimeOffset.Now, Ultracode));
        Status = SessionStatus.Running;
        turns++;
        TurnStartedAt = DateTimeOffset.Now;
        StreamingText = "";
        ThinkingTokens = 0;
    }

    internal void Resolve(PendingPermission p, Decision d)
    {
        Pending = Pending.Remove(p);
        if (Tool(p.ToolUseId) is { } t) t.State = d == Decision.Deny ? ToolState.Denied : ToolState.Running;
        if (Pending.IsEmpty && Status == SessionStatus.Waiting) Status = SessionStatus.Running;
    }

    // Tools still open when the turn or the process ends: they never got a result.
    internal void EndTools(DateTimeOffset now, bool keepBackground = true)
    {
        turns = 0;
        foreach (var t in Items.OfType<ToolItem>().Where(t => t.State is ToolState.Running or ToolState.Waiting && !(keepBackground && t.Background)))
        {
            t.State = ToolState.Error;
            t.EndedAt = now;
        }
    }

    ToolItem? Tool(string? id) => id is null ? null : Items.LastOrDefault(i => i is ToolItem t && t.Id == id) as ToolItem;

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
                StreamingText += d.Text;
                break;

            case AssistantTextEvt a:
                if (a.ParentToolUseId is null) StreamingText = "";
                Items = Items.Add(new TextItem(a.Text, a.ParentToolUseId));
                break;

            case ToolUseEvt u:
                if (u.ParentToolUseId is null) StreamingText = "";
                Items = Items.Add(new ToolItem(u.Id, u.Name, u.Input, u.ParentToolUseId) { State = ToolState.Running, StartedAt = now });
                ToolCount++;
                break;

            case ToolResultEvt { Structured: { } rs } r when Tool(r.ToolUseId) is { State: ToolState.Running } t && ToolKinds.IsAgent(t.Name)
                                                              && Events.Str(rs, "status") == "async_launched":
                t.Background = true;   // launch receipt, not the outcome: stays Running until the task-notification
                t.TaskId = Events.Str(rs, "agentId") ?? t.TaskId;
                t.Structured = rs;
                break;

            case ToolResultEvt r when Tool(r.ToolUseId) is { } t:
                t.State = r.IsError ? (t.State == ToolState.Denied ? ToolState.Denied : ToolState.Error) : ToolState.Done;
                t.ResultText = r.Text;
                t.Structured = r.Structured;
                t.EndedAt = now;
                if (r.Structured is { } s && Events.Prop(s, "totalTokens") is { ValueKind: JsonValueKind.Number } tok) t.Tokens = tok.GetInt64();
                break;

            case UserTextEvt u when UserText(u.Text) is { } text:
                Items = Items.Add(new UserItem(text, u.At ?? now, false));
                break;

            case PermissionEvt p:
                Pending = Pending.Add(new(p.RequestId, p.Tool, p.Input, p.Description, p.ToolUseId, p.Suggestions, now));
                if (Tool(p.ToolUseId) is { } wt) wt.State = ToolState.Waiting;
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
                var aborted = r.TerminalReason?.StartsWith("aborted") == true;   // an interrupt also drops the queued messages
                LastResultSubtype = aborted ? "interrompu" : r.Subtype;
                LastResultAt = now;
                if (r.ContextTokens > 0) ContextTokens = r.ContextTokens;
                if (r.ContextWindow is { } w) ContextWindow = w;
                if (r.FastModeState is { } fs) { FastModeState = fs; FastModeReason = r.FastModeReason; }
                var left = aborted ? 0 : Math.Max(0, turns - 1);
                EndTools(now);
                turns = left;
                Pending = [];
                StreamingText = "";
                ThinkingTokens = 0;
                if (left > 0) break;   // a message sent during the turn runs next
                TurnStartedAt = null;
                if (Status is SessionStatus.Running or SessionStatus.Waiting or SessionStatus.Starting) Status = SessionStatus.Idle;
                break;

            case RateLimitEvt l:
                Limits = l;
                LimitsAt = now;
                break;

            case TaskStartedEvt ts when Tool(ts.ToolUseId) is { } t:
                t.TaskId = ts.TaskId;
                break;

            case TaskProgressEvt tp when Items.OfType<ToolItem>().LastOrDefault(t => t.TaskId == tp.TaskId) is { } t:
                t.Tokens = tp.TotalTokens;
                t.SubToolUses = tp.ToolUses;
                break;

            case TaskDoneEvt td when (Tool(td.ToolUseId) ?? Items.OfType<ToolItem>().LastOrDefault(x => td.TaskId.Length > 0 && x.TaskId == td.TaskId)) is { } t
                                     && (t.State == ToolState.Running || t.Background):   // a resumed background agent notifies again
                t.State = td.Status == "completed" ? ToolState.Done : ToolState.Error;
                t.EndedAt = td.DurationMs > 0 ? t.StartedAt.AddMilliseconds(td.DurationMs) : now;
                if (td.Result is not null) t.ResultText = td.Result;
                if (td.Tokens > 0) t.Tokens = td.Tokens;
                if (td.ToolUses > 0) t.SubToolUses = td.ToolUses;
                break;

            case TitleEvt { Title.Length: > 0 } ti:
                Name = ti.Title;
                break;
        }
    }

    // send → tool_use → permission → deny → result, replayed on the reducer only.
    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "LiveSession: " + what);
        var input = JsonDocument.Parse("""{"file_path":"C:\\w\\b.txt","content":"x"}""").RootElement.Clone();
        var fresh = new LiveSession("id", "n", @"C:\w", "default").Args(false);
        Ok(fresh is ["--permission-mode", "manual", _, _, "--session-id", "id", "--name", "n"], "fresh args");
        var resumed = new LiveSession("o", "o", @"C:\w", "plan", model: "haiku").Args(true);
        Ok(resumed is ["--permission-mode", "plan", _, _, "--resume", "o", "--model", "haiku"], "resume args");
        Ok(ClaudeSession.TraceLine(1234, "stderr x") == "+1234 ms stderr x", "trace line format");

        var s = new LiveSession("id", "essai", @"C:\w", "default");

        s.BeginTurn("crée b.txt");
        Ok(s.Status == SessionStatus.Running && s.TurnStartedAt is not null && s.Items is [UserItem { Text: "crée b.txt" }], "send");
        s.Apply(new TextDeltaEvt("je ", null));
        s.Apply(new TextDeltaEvt("crée", null));
        Ok(s.StreamingText == "je crée", "streaming text");
        s.Apply(new AssistantTextEvt("m1", "je crée", null, false));
        Ok(s.StreamingText == "" && s.Items[^1] is TextItem { Markdown: "je crée" }, "text block replaces stream");
        s.Apply(new ToolUseEvt("t1", "Write", input, null));
        Ok(s.ToolCount == 1 && s.Items[^1] is ToolItem { State: ToolState.Running }, "tool_use");
        s.Apply(new PermissionEvt("r1", "Write", input, "b.txt", "t1", null));
        Ok(s.Status == SessionStatus.Waiting && s.Pending.Count == 1 && s.Items[^1] is ToolItem { State: ToolState.Waiting }, "permission");
        s.Resolve(s.Pending[0], Decision.Deny);
        Ok(s.Status == SessionStatus.Running && s.Pending.IsEmpty && s.Items[^1] is ToolItem { State: ToolState.Denied }, "deny");
        s.Apply(new ToolResultEvt("t1", "Refusé par l’utilisateur", true, null));
        Ok(s.Items[^1] is ToolItem { State: ToolState.Denied, ResultText: "Refusé par l’utilisateur" }, "denied stays denied");
        s.Apply(new ResultEvt("success", false, 0.01m, 2400, 2, 41878, 1000000, "off", "sdk_opt_in_required"));
        Ok(s.Status == SessionStatus.Idle && s.CostUsd == 0.01m && s.LastTurn == TimeSpan.FromMilliseconds(2400) && s.ContextTokens == 41878, "result");

        s.BeginTurn("encore");
        s.Apply(new PermissionEvt("r2", "Edit", input, null, "nope", null));
        s.Apply(new ResultEvt("error_during_execution", true, 0.03m, 900, 1, 0, null, null, null, "aborted_streaming"));
        Ok(s.CostUsd == 0.03m && s.LastTurnCostUsd == 0.02m, "cost assigned, not added");
        Ok(s.Pending.IsEmpty && s.Status == SessionStatus.Idle && s.LastResultSubtype == "interrompu" && s.ContextTokens == 41878, "interrupted turn");
        s.Apply(new ResultEvt("success", false, 0, 10, 0, 0, null, null, null));
        Ok(s.CostUsd == 0.03m, "slash command result keeps cost");

        s.BeginTurn("un"); s.BeginTurn("deux");   // second message queued by the CLI
        s.Apply(new ResultEvt("success", false, 0.04m, 10, 1, 0, null, null, null));
        Ok(s.Status == SessionStatus.Running && s.TurnStartedAt is not null, "queued turn keeps running");
        s.Apply(new ResultEvt("success", false, 0.05m, 10, 1, 0, null, null, null));
        Ok(s.Status == SessionStatus.Idle && s.TurnStartedAt is null, "queued turn done");

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
        Ok(UserText("<command-message>cost</command-message>\n<command-name>/cost</command-name>\n<command-args></command-args>") == "/cost"
            && UserText("<local-command-stdout>Set model</local-command-stdout>") is null && UserText("salut") == "salut", "user text");
    }

    // Echoed slash commands come back as tags: "<command-name>/x</command-name>…<command-args>a</command-args>" → "/x a"; their output is hidden.
    static string? UserText(string t)
    {
        if (t.StartsWith("[Request interrupted by user") || t.StartsWith("<local-command-") || t.StartsWith("<task-notification>")) return null;
        if (System.Text.RegularExpressions.Regex.Match(t, "<command-name>(.*?)</command-name>") is not { Success: true } m) return t;
        var args = System.Text.RegularExpressions.Regex.Match(t, "<command-args>(.*?)</command-args>", System.Text.RegularExpressions.RegexOptions.Singleline).Groups[1].Value.Trim();
        return args.Length > 0 ? $"{m.Groups[1].Value} {args}" : m.Groups[1].Value;
    }
}
