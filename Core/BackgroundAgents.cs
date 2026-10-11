using System.Text.Json;

namespace ClaudeCodeUI;

// `claude --bg` sessions: listed by `claude agents --json --all` (the interactive ones running too), driven by the
// logs / stop / respawn / rm subcommands. The daemon owns them: no stream-json, plain CLI calls (claude 2.1.296).
public static class BackgroundAgents
{
    // State: a background session's state (working, blocked, done…), an interactive one's status (busy, idle, waiting).
    public sealed record Agent(string Id, string Kind, string? SessionId, string Name, string Cwd, string State, DateTimeOffset StartedAt, string? WaitingFor)
    {
        public bool Background => Kind == "background";
    }

    static string Home => Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);

    public static async Task<List<Agent>> List()
    {
        var (code, output, err) = await Plugins.Run(Home, ["--json", "--all"], command: "agents");
        if (code != 0) throw new InvalidOperationException((err + "\n" + output).Trim());
        return Parse(JsonDocument.Parse(output).RootElement);
    }

    internal static List<Agent> Parse(JsonElement r) => r.ValueKind == JsonValueKind.Array
        ? [.. r.EnumerateArray().Select(a => new Agent(
            Events.Str(a, "id") ?? Events.Long(a, "pid").ToString(), Events.Str(a, "kind") ?? "", Events.Str(a, "sessionId"),
            Events.Str(a, "name") ?? "", Events.Str(a, "cwd") ?? "", Events.Str(a, "state") ?? Events.Str(a, "status") ?? "",
            Events.Long(a, "startedAt") is > 0 and < 253402300800000 and var ms ? DateTimeOffset.FromUnixTimeMilliseconds(ms) : default,
            Events.Str(a, "waitingFor")))]
        : [];

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
    }
}
