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
            foreach (var probe in new Func<string, Task<IEnumerable<(string, string, string)>>>[] { Ultracode, PermissionSession, Mcp, Compact })
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
            var agents = turn.Sum(e => ToolUses(e).Count(n => n is "Agent" or "Task"));
            var started = turn.Count(e => Events.Str(e, "type") == "system" && Events.Str(e, "subtype") == "task_started");
            // The prompt asks for agents, so agents alone would prove little: the flag must also read back as applied.
            var on = set is { ValueKind: JsonValueKind.True } && agents + started > 0
                ? ("PASS", $"applied.ultracode=true, {agents} Agent tool_use, {started} task_started")
                : ("FAIL", $"applied.ultracode={set?.GetRawText() ?? "absent"}, {agents} Agent tool_use, {started} task_started");

            try { await c.S.Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["ultracode"] = false } }); }
            catch (ClaudeRequestException ex) { return [on, ("FAIL", $"apply_flag_settings refused: {ex.Message}")]; }
            var after = Events.Prop(await c.S.Request("get_settings"), "applied") is { } b ? Events.Prop(b, "ultracode") : null;
            return [on, after is { ValueKind: JsonValueKind.False }
                ? ("PASS", "accepted, applied.ultracode=false")
                : ("FAIL", $"accepted, but applied.ultracode={after?.GetRawText() ?? "absent"}")];
        });
    }

    // permission-session: answering a can_use_tool with updatedPermissions = its non-setMode suggestions stops the CLI
    // from asking again for the same request. The command must write: 2.1.295 auto-allows read-only ones like
    // `git status` (the issue's original prompt), which never reach can_use_tool.
    const string Cmd = "touch probe.txt";
    static async Task<IEnumerable<(string, string, string)>> PermissionSession(string dir)
    {
        await using var c = await Cli.Start(dir);
        return await Guard(["permission-session"], async () =>
        {
            string? tool = null, types = null;
            await c.Turn($"Run the shell command: {Cmd}", async e =>
            {
                var r = e.GetProperty("request");
                if (tool is not null || Events.Prop(r, "permission_suggestions") is not { ValueKind: JsonValueKind.Array } sg) return false;
                tool = Events.Str(r, "tool_name");
                // 2.1.295 offers setMode next to addRules/addDirectories for a write: send only the latter, which is
                // exactly what LiveSession.Answer() sends when no setMode is offered.
                var rules = new JsonArray([.. sg.EnumerateArray().Where(x => Events.Str(x, "type") != "setMode").Select(x => JsonNode.Parse(x.GetRawText()))]);
                types = string.Join("+", rules.Select(x => (string?)x!["type"]));
                if (rules.Count == 0) return false;
                await c.S.Respond(Events.Str(e, "request_id")!, true, r.GetProperty("input"), rules);
                return true;
            });
            if (tool is null) return [("SKIP", "no can_use_tool with suggestions in the first turn")];
            if (types is "") return [("SKIP", "only a setMode suggestion: that path is already verified")];
            var asked = 0;
            var second = await c.Turn($"Run the shell command again: {Cmd}", e =>
            {
                if (Events.Str(e.GetProperty("request"), "tool_name") == tool) asked++;
                return Task.FromResult(false);
            });
            var ran = second.Sum(e => ToolUses(e).Count(n => n == tool));
            return [ran == 0 ? ("FAIL", $"inconclusive: no {tool} tool_use in the second turn")
                : asked == 0 ? ("PASS", $"{types} honoured, {tool} ran again with 0 re-prompts")
                : ("FAIL", $"{types} not honoured, {asked} re-prompt(s) for {tool}")];
        });
    }

    // mcp-toggle: mcp_toggle {enabled:false} turns a server `disabled` in mcp_status, {enabled:true} brings it back.
    // mcp-reconnect: mcp_reconnect is answered (success or error) and leaves the unstartable server `failed`.
    // The server is the probe's own (--strict-mcp-config): a stdio command that does not exist, so it is always `failed`.
    static async Task<IEnumerable<(string, string, string)>> Mcp(string dir)
    {
        var config = Path.Combine(dir, "mcp.json");
        File.WriteAllText(config, """{"mcpServers":{"probe":{"command":"ccui-probe-missing-binary"}}}""");
        await using var c = await Cli.Start(dir, "--mcp-config", config, "--strict-mcp-config");
        return await Guard(["mcp-toggle", "mcp-reconnect"], async () =>
        {
            async Task<string> Status() =>
                Events.Prop(await c.S.Request("mcp_status"), "mcpServers") is { ValueKind: JsonValueKind.Array } a
                && a.EnumerateArray().FirstOrDefault(m => Events.Str(m, "name") == "probe") is { ValueKind: JsonValueKind.Object } m
                    ? Events.Str(m, "status") ?? "?" : "absent";
            async Task<string> Toggle(bool on)
            {
                // Enabling a server that cannot start may answer an error: the status read back is what counts.
                var err = "";
                try { await c.S.Request("mcp_toggle", new() { ["serverName"] = "probe", ["enabled"] = on }, Seconds); }
                catch (ClaudeRequestException ex) { err = $" (error \"{ex.Message}\")"; }
                return await Status() + err;
            }

            var before = await Status();
            var off = await Toggle(false);
            var on = await Toggle(true);
            var toggle = off == "disabled" && !on.StartsWith("disabled")
                ? ("PASS", $"{before} → {off} → {on}")
                : ("FAIL", $"{before} → {off} → {on}");

            string answer;
            try { await c.S.Request("mcp_reconnect", new() { ["serverName"] = "probe" }, Seconds); answer = "success"; }
            catch (ClaudeRequestException ex) { answer = $"error \"{ex.Message}\""; }
            catch (TimeoutException) { return [toggle, ("FAIL", "mcp_reconnect unanswered")]; }
            var after = await Status();
            return [toggle, after == "failed" ? ("PASS", $"answered {answer}, status {after}") : ("FAIL", $"answered {answer}, status {after}")];
        });
    }

    // compact: "/compact" sent as user text (ContextPanel's button) compacts: a compact_boundary or a smaller context.
    static async Task<IEnumerable<(string, string, string)>> Compact(string dir)
    {
        await using var c = await Cli.Start(dir);
        return await Guard(["compact"], async () =>
        {
            async Task<long> Total() =>
                Events.Prop(await c.S.Request("get_context_usage"), "totalTokens") is { ValueKind: JsonValueKind.Number } n ? (long)n.GetDouble() : -1;

            await c.Turn("Say hello in one word.");
            var before = await Total();
            var turn = await c.Turn("/compact");
            var after = await Total();
            var boundary = turn.Any(e => Events.Str(e, "type") == "system" && Events.Str(e, "subtype") == "compact_boundary");
            var error = Events.Prop(turn[^1], "is_error") is { ValueKind: JsonValueKind.True };
            var detail = $"compact_boundary {(boundary ? "seen" : "absent")}, context {before}→{after}{(error ? ", result is_error" : "")}";
            return [!error && (boundary || (after >= 0 && after < before)) ? ("PASS", detail) : ("FAIL", detail)];
        });
    }

    // ---------- plumbing ----------

    // Runs a probe body that yields one (verdict, detail) per id; a throw or timeout fails every id it had not answered.
    static async Task<IEnumerable<(string, string, string)>> Guard(string[] ids, Func<Task<(string Verdict, string Detail)[]>> body)
    {
        (string, string)[] r;
        try { r = await body().WaitAsync(TimeSpan.FromSeconds(Seconds)); }
        catch (TimeoutException) { r = [.. ids.Select(_ => ("FAIL", "timeout"))]; }
        catch (Exception ex) when (ex is not StartException) { r = [.. ids.Select(_ => ("FAIL", (ex.InnerException ?? ex).Message))]; }   // claude exiting closes the channel
        return ids.Zip(r, (id, v) => (v.Item1, id, v.Item2));
    }

    // Names of every tool_use block in an assistant message (parallel calls share one message).
    static IEnumerable<string?> ToolUses(JsonElement e) =>
        Events.Str(e, "type") == "assistant" && Events.Prop(e, "message") is { } m && Events.Prop(m, "content") is { ValueKind: JsonValueKind.Array } c
            ? c.EnumerateArray().Where(b => Events.Str(b, "type") == "tool_use").Select(b => Events.Str(b, "name"))
            : [];

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
