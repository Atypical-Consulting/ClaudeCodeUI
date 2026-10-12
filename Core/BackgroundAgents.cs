using System.Text.Json;

namespace ClaudeCodeUI;

// `claude --bg` sessions: listed by `claude agents --json --all` (the interactive ones running too), driven by the
// logs / stop / respawn / rm subcommands. The daemon owns them: no stream-json, plain CLI calls (claude 2.1.296).
public static class BackgroundAgents
{
    // State: a background session's state (working, blocked, done…), an interactive one's status (busy, idle, waiting).
    // Needs and LastAt come from the session's own job record (~/.claude/jobs/<id>): the question it waits on, its last step.
    public sealed record Agent(string Id, string Kind, string? SessionId, string Name, string Cwd, string State, DateTimeOffset StartedAt, string? WaitingFor)
    {
        public bool Background => Kind == "background";
        public string? Needs { get; init; }
        public string? Mode { get; init; }   // the --permission-mode respawn restarts it in
        public bool Working => State is "working" or "busy" or "running";
        public DateTimeOffset? LastAt { get; init; }
        public bool Stale { get; init; }     // a background session while the daemon is down: its state is the last one recorded
        public string Label => Name.Length > 0 ? Name : Background ? Id : Path.GetFileName(Cwd.TrimEnd('/', '\\'));   // never a PID
        public bool NeedsYou => State is "blocked" or "waiting" && !Stale;   // a frozen question is shown, not raised app-wide
    }

    static string Home => Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);

    // The rail's count of sessions that need you, from the last read: the page's, or the one-a-minute watch the rail starts.
    public static int NeedsYouCount { get; private set; }
    public static event Action? Changed;
    static int watching;

    public static void Watch()
    {
        if (Interlocked.Exchange(ref watching, 1) == 1) return;
        _ = Task.Run(async () =>
        {
            while (true)
            {
                try { await List(); } catch (Exception) { }   // List already reset the count
                await Task.Delay(TimeSpan.FromMinutes(1));
            }
        });
    }

    // The daemon down means no background session runs: the list still says their last state. `daemon status` never starts it.
    public static async Task<(List<Agent> Rows, bool DaemonUp)> List()
    {
        try { return await Read(); }
        catch (Exception)
        {
            if (NeedsYouCount != 0) { NeedsYouCount = 0; Changed?.Invoke(); }   // unknown is not "1 needs you": the rail claims nothing
            throw;
        }
    }

    static async Task<(List<Agent> Rows, bool DaemonUp)> Read()
    {
        var (code, output, err) = await Plugins.Run(Home, ["--json", "--all"], command: "agents");
        if (code != 0) throw new InvalidOperationException((err + "\n" + output).Trim());
        var rows = Parse(JsonDocument.Parse(output).RootElement).Select(WithJob).ToList();
        var up = !rows.Any(r => r.Background) || !(await Plugins.Run(Home, ["status"], command: "daemon")).Out.TrimStart().StartsWith("not running");
        if (!up) rows = [.. rows.Select(r => r.Background ? r with { Stale = true } : r)];
        if (rows.Count(r => r.NeedsYou) is var n && n != NeedsYouCount) { NeedsYouCount = n; Changed?.Invoke(); }
        return (Order(rows), up);
    }

    // Needs you first, then working, then a frozen question, then the rest; the CLI's own order inside each.
    internal static List<Agent> Order(IEnumerable<Agent> rows) =>
        [.. rows.OrderBy(a => a.NeedsYou ? 0 : !a.Stale && a.Working ? 1 : a.State is "blocked" or "waiting" ? 2 : 3)];

    internal static List<Agent> Parse(JsonElement r) => r.ValueKind == JsonValueKind.Array
        ? [.. r.EnumerateArray().Select(a => new Agent(
            Events.Str(a, "id") ?? Events.Long(a, "pid").ToString(), Events.Str(a, "kind") ?? "", Events.Str(a, "sessionId"),
            Events.Str(a, "name") ?? "", Events.Str(a, "cwd") ?? "", Events.Str(a, "state") ?? Events.Str(a, "status") ?? "",
            Events.Long(a, "startedAt") is > 0 and < 253402300800000 and var ms ? DateTimeOffset.FromUnixTimeMilliseconds(ms) : default,
            Events.Str(a, "waitingFor")))]
        : [];

    static Agent WithJob(Agent a)
    {
        if (!a.Background || a.Id.Contains('/') || a.Id.Contains('.')) return a;
        var dir = Path.Combine(Home, ".claude", "jobs", a.Id);
        try
        {
            var state = File.Exists(Path.Combine(dir, "state.json")) ? File.ReadAllText(Path.Combine(dir, "state.json")) : null;
            var last = File.Exists(Path.Combine(dir, "timeline.jsonl")) ? File.ReadLines(Path.Combine(dir, "timeline.jsonl")).LastOrDefault(l => l.Length > 0) : null;
            return Job(a, state, last);
        }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException or JsonException) { return a; }
    }

    internal static Agent Job(Agent a, string? state, string? lastLine)
    {
        var root = state is null ? default : JsonDocument.Parse(state).RootElement;
        var needs = state is null ? null : Events.Str(root, "needs");
        var flags = state is not null && Events.Prop(root, "respawnFlags") is { ValueKind: JsonValueKind.Array } f ? f.EnumerateArray().Select(x => x.GetString() ?? "").ToList() : [];
        var mode = flags.IndexOf("--permission-mode") is >= 0 and var i && i + 1 < flags.Count ? flags[i + 1] : null;
        DateTimeOffset? at = lastLine is not null && Events.Str(JsonDocument.Parse(lastLine).RootElement, "at") is { } s && DateTimeOffset.TryParse(s, out var t) ? t : null;
        return a with { Needs = string.IsNullOrWhiteSpace(needs) ? null : needs.Trim(), LastAt = at, Mode = mode };
    }

    // logs | stop | respawn | rm <id>: the CLI's own text back (rm says what it would not delete, and the flag that would).
    public static async Task<(bool Ok, string Text)> Run(string command, string id)
    {
        if (command is not ("logs" or "stop" or "respawn" or "rm")) throw new ArgumentOutOfRangeException(nameof(command));
        var (code, output, err) = await Plugins.Run(Home, [id], command: command);
        return (code == 0, (output + "\n" + err).Trim());
    }

    // claude 2.1.296's answer, trimmed.
    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "BackgroundAgents: " + what);
        var list = Parse(JsonDocument.Parse("""[{"id":"632fca02","cwd":"/r/.claude/worktrees/x","kind":"background","startedAt":1785952770170,"sessionId":"632fca02-153e-4f54-a711-8199f18e140c","name":"github issues planning","state":"blocked"},{"pid":69764,"cwd":"/r","kind":"interactive","startedAt":1791561876803,"sessionId":"3b30179d-689e-475a-813c-34f01c89eeb4","name":"ui","status":"waiting","waitingFor":"input needed"}]""").RootElement);
        Ok(list is [{ Id: "632fca02", Background: true, State: "blocked", Name: "github issues planning" }, { Id: "69764", Background: false, State: "waiting", WaitingFor: "input needed" }]
           && list[0].StartedAt == DateTimeOffset.FromUnixTimeMilliseconds(1785952770170), "agents --json");
        Ok(Parse(JsonDocument.Parse("{}").RootElement) is [], "not a list");
        var job = Job(list[0], """{"state":"blocked","needs":"delete the verify/* branches too?","tempo":"blocked","respawnFlags":["--reply-on-resume","--permission-mode","bypassPermissions","--model","opus"]}""", """{"at":"2026-08-12T16:38:48.935Z","state":"blocked"}""");
        Ok(job is { Needs: "delete the verify/* branches too?", Mode: "bypassPermissions", LastAt: { } at } && at == DateTimeOffset.Parse("2026-08-12T16:38:48.935Z"), "job record");
        Ok(Job(list[0], """{"needs":null}""", null) is { Needs: null, LastAt: null, Mode: null }, "job record without a question");
        var idle = list[0] with { State = "idle", Id = "a" };
        Ok(Order([idle, list[0] with { State = "working", Id = "b" }, list[1]]) is [{ Id: "69764" }, { Id: "b" }, { Id: "a" }], "needs you first");
        Ok(list[0] with { Stale = true } is { NeedsYou: false } && Order([idle, list[0] with { Stale = true }, list[1]]) is [{ Id: "69764" }, { Id: "632fca02" }, { Id: "a" }], "a frozen question is not raised, sorts after the live ones");
        Ok(list[1] with { Name = "", Cwd = "/r/app" } is { Label: "app" }, "a nameless terminal session is named by its folder, not its pid");
    }
}
