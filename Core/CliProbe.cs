using System.Diagnostics;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading.Channels;

namespace ClaudeCodeUI;

// `dotnet run -- --probe-cli`: drives the real, logged-in claude CLI (haiku, a throw-away git repo under the temp dir)
// to check the controls docs/PLAN.md §1.2 lists as "accepted but untested". One `PASS|FAIL|SKIP <id>: <detail>` line
// per probe, then the CLI version. Exit 0 = no FAIL, 1 = a FAIL, 2 = claude could not start.
// Costs a few haiku tokens: opt-in, never part of --self-check or CI.
public static class CliProbe
{
    const int Seconds = 120;   // per probe, overall

    public static async Task<int> Run()
    {
        if (!ClaudeSession.OnPath()) { Console.WriteLine("claude not found on PATH"); return 2; }
        var dir = Path.Combine(Path.GetTempPath(), "ccui-probe-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(dir);
        try
        {
            Git(dir, "init", "-q");
            File.WriteAllText(Path.Combine(dir, "README.md"), "# probe\n\nScratch repo for ClaudeCodeUI --probe-cli.\n");
            Git(dir, "add", "README.md");
            Git(dir, "-c", "user.name=probe", "-c", "user.email=probe@localhost", "commit", "-q", "-m", "init");

            var fails = 0;
            foreach (var probe in new Func<string, Task<IEnumerable<(string, string, string)>>>[] { Ultracode })
                foreach (var (verdict, id, detail) in await probe(dir))
                {
                    Console.WriteLine($"{verdict} {id}: {detail}");
                    if (verdict == "FAIL") fails++;
                }
            Console.WriteLine($"claude {Version()}");
            return fails > 0 ? 1 : 0;
        }
        catch (StartException ex) { Console.WriteLine($"claude could not start: {ex.Message}"); return 2; }
        finally { try { Directory.Delete(dir, true); } catch { } }
    }

    // ---------- probes ----------

    // ultracode-on: apply_flag_settings {ultracode:true} makes the turn spawn a sub-agent.
    // ultracode-off: {ultracode:false} after the result is accepted and read back as false.
    static async Task<IEnumerable<(string, string, string)>> Ultracode(string dir)
    {
        await using var c = await Cli.Start(dir);
        return await Guard(["ultracode-on", "ultracode-off"], async () =>
        {
            var applied = Events.Prop(await c.S.Request("get_settings"), "applied");
            if (applied is not { } a || Events.Prop(a, "ultracodeAvailable") is not { ValueKind: JsonValueKind.True })
                return [("SKIP", "ultracodeAvailable=false"), ("SKIP", "ultracodeAvailable=false")];
            await c.S.Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["ultracode"] = true } });
            var set = Events.Prop(await c.S.Request("get_settings"), "applied") is { } t ? Events.Prop(t, "ultracode") : null;
            var turn = await c.Turn("Use two parallel agents: one lists the files in this repo, one counts the lines in README.md.");
            var agents = turn.Count(e => ToolUse(e) is "Agent" or "Task");
            var started = turn.Count(e => Events.Str(e, "type") == "system" && Events.Str(e, "subtype") == "task_started");
            // The prompt asks for agents, so agents alone would prove little: the flag must also read back as applied.
            var on = set is { ValueKind: JsonValueKind.True } && agents + started > 0
                ? ("PASS", $"applied.ultracode=true, {agents} Agent tool_use, {started} task_started")
                : ("FAIL", $"applied.ultracode={set?.GetRawText() ?? "absent"}, {agents} Agent tool_use, {started} task_started");

            try { await c.S.Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["ultracode"] = false } }); }
            catch (ClaudeRequestException ex) { return [on, ("FAIL", $"apply_flag_settings refused: {ex.Message}")]; }
            var after = Events.Prop(await c.S.Request("get_settings"), "applied") is { } b ? Events.Prop(b, "ultracode") : null;
            return [on, after is { ValueKind: JsonValueKind.False } or null
                ? ("PASS", $"accepted, applied.ultracode={(after is null ? "absent" : "false")}")
                : ("FAIL", $"accepted, but applied.ultracode={after.Value.GetRawText()}")];
        });
    }

    // ---------- plumbing ----------

    // Runs a probe body that yields one (verdict, detail) per id; a throw or timeout fails every id it had not answered.
    static async Task<IEnumerable<(string, string, string)>> Guard(string[] ids, Func<Task<(string Verdict, string Detail)[]>> body)
    {
        (string, string)[] r;
        try { r = await body().WaitAsync(TimeSpan.FromSeconds(Seconds)); }
        catch (TimeoutException) { r = [.. ids.Select(_ => ("FAIL", "timeout"))]; }
        catch (Exception ex) when (ex is not StartException) { r = [.. ids.Select(_ => ("FAIL", ex.Message))]; }
        return ids.Zip(r, (id, v) => (v.Item1, id, v.Item2));
    }

    static string? ToolUse(JsonElement e) =>
        Events.Str(e, "type") == "assistant" && Events.Prop(e, "message") is { } m && Events.Prop(m, "content") is { ValueKind: JsonValueKind.Array } c
            ? c.EnumerateArray().Where(b => Events.Str(b, "type") == "tool_use").Select(b => Events.Str(b, "name")).FirstOrDefault()
            : null;

    static void Git(string dir, params string[] args)
    {
        var psi = new ProcessStartInfo("git") { WorkingDirectory = dir, RedirectStandardOutput = true, RedirectStandardError = true };
        foreach (var a in args) psi.ArgumentList.Add(a);
        using var p = Process.Start(psi)!;
        p.WaitForExit();
        if (p.ExitCode != 0) throw new InvalidOperationException($"git {string.Join(' ', args)}: {p.StandardError.ReadToEnd()}");
    }

    static string Version()
    {
        try
        {
            var psi = new ProcessStartInfo("claude", "--version") { RedirectStandardOutput = true };
            using var p = Process.Start(psi)!;
            var v = p.StandardOutput.ReadToEnd().Trim();
            p.WaitForExit();
            return v;
        }
        catch (Exception ex) { return $"--version failed: {ex.Message}"; }
    }

    sealed class StartException(string message) : Exception(message);

    // One claude process (haiku, manual permissions) whose stdout events land in a channel.
    sealed class Cli : IAsyncDisposable
    {
        readonly Channel<JsonElement> events = Channel.CreateUnbounded<JsonElement>();
        public ClaudeSession S { get; private set; } = null!;

        public static async Task<Cli> Start(string cwd, params string[] extra)
        {
            var c = new Cli();
            try
            {
                c.S = new ClaudeSession(cwd, ["--model", "haiku", "--permission-mode", "manual", .. extra],
                    e => c.events.Writer.WriteAsync(e).AsTask(), (_, text) => c.events.Writer.TryComplete(new InvalidOperationException(text)));
            }
            catch (Exception ex) { throw new StartException(ex.Message); }
            try { await c.S.Request("initialize", null, Seconds); }
            catch (Exception ex) { await c.DisposeAsync(); throw new StartException($"initialize: {ex.Message}"); }
            return c;
        }

        // Sends a user turn and collects its events until `result` (included). A can_use_tool goes to onPermission when
        // given (true = handled), else it is allowed as asked, so no probe can hang on an unexpected prompt.
        public async Task<List<JsonElement>> Turn(string text, Func<JsonElement, Task<bool>>? onPermission = null)
        {
            await S.SendUser(text);
            var seen = new List<JsonElement>();
            while (true)
            {
                var e = await events.Reader.ReadAsync();
                seen.Add(e);
                if (Events.Str(e, "type") == "result") return seen;
                if (Events.Str(e, "type") == "control_request" && Events.Prop(e, "request") is { } r && Events.Str(r, "subtype") == "can_use_tool"
                    && (onPermission is null || !await onPermission(e)))
                    await S.Respond(Events.Str(e, "request_id")!, true, r.GetProperty("input"));
            }
        }

        public async ValueTask DisposeAsync()
        {
            if (S is not null) await S.DisposeAsync();
        }
    }
}
