using System.Diagnostics;
using System.IO.Compression;
using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using System.Threading.Channels;

namespace ClaudeCodeUI;

// `dotnet run -- --probe-cli [case]`: drives the real, logged-in claude CLI (haiku, a throw-away git repo under the temp dir)
// to check the controls docs/PLAN.md §1.2 lists as "accepted but untested". One `PASS|FAIL|SKIP <id>: <detail>` line
// per probe, then the CLI version. Exit 0 = no FAIL, 1 = a FAIL, 2 = claude could not start or an unknown case.
// `--probe-cli plan compact` runs only those cases. Costs a few haiku tokens: opt-in, never part of --self-check or CI.
public static class CliProbe
{
    const int Seconds = 120;   // per probe, overall

    public static async Task<int> Run(string[] cases)
    {
        (string Name, Func<string, Task<IEnumerable<(string, string, string)>>> Probe)[] all =
            [("ultracode", Ultracode), ("permission-session", PermissionSession), ("mcp", Mcp), ("compact", Compact), ("plan", Plan), ("ask-user-question", AskUserQuestion), ("todo-tools", TodoTools), ("image", Image), ("file-mention", FileMention), ("prompt-history", History), ("queue", Queue), ("notify", Waiting), ("rewind", Rewind), ("fork", Fork), ("background-tasks", BackgroundTasks), ("monitor-stop", MonitorStop), ("exit-ends-tasks", ExitEndsTasks), ("hooks", Hooks), ("mcp-auth", McpAuth), ("memory", Memory), ("add-dir", AddDir), ("transcript-search", TranscriptSearch)];
        if (cases.Except(all.Select(p => p.Name)).ToArray() is { Length: > 0 } unknown)
        {
            Console.WriteLine($"unknown case(s) {string.Join(", ", unknown)}; known: {string.Join(", ", all.Select(p => p.Name))}");
            return 2;
        }
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
            foreach (var (_, probe) in all.Where(p => cases.Length == 0 || cases.Contains(p.Name)))
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
            await c.Turn($"Run the shell command: {Cmd}", onPermission: async e =>
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
            var second = await c.Turn($"Run the shell command again: {Cmd}", onPermission: e =>
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

    // mcp-auth: mcp_authenticate {serverName} on a `needs-auth` http server answers {authUrl, callbackExpected, ...}; once
    // the browser follows authUrl, the CLI's own callback listener takes the code, fetches the token and reconnects, so
    // mcp_status turns `connected` without another request. The server is the probe's own OAuth-protected fake (no
    // network, no account); an HttpClient plays the browser. mcp_clear_auth then drops the token the CLI stored.
    static async Task<IEnumerable<(string, string, string)>> McpAuth(string dir)
    {
        using var fake = new FakeOAuthMcp();
        var config = Path.Combine(dir, "mcp-auth.json");
        File.WriteAllText(config, """{"mcpServers":{"probe-auth":{"type":"http","url":"URL"}}}""".Replace("URL", fake.Url + "mcp"));
        await using var c = await Cli.Start(dir, "--mcp-config", config, "--strict-mcp-config");
        return await Guard(["mcp-auth"], async () =>
        {
            async Task<string> Status() =>
                Events.Prop(await c.S.Request("mcp_status"), "mcpServers") is { ValueKind: JsonValueKind.Array } a
                && a.EnumerateArray().FirstOrDefault(m => Events.Str(m, "name") == "probe-auth") is { ValueKind: JsonValueKind.Object } m
                    ? Events.Str(m, "status") ?? "?" : "absent";
            async Task<string> Until(Func<string, bool> done)
            {
                var st = await Status();
                for (var i = 0; i < 40 && !done(st); i++) { await Task.Delay(500); st = await Status(); }
                return st;
            }

            var before = await Until(st => st != "pending");
            if (before != "needs-auth") return [("FAIL", $"status {before}, expected needs-auth")];
            var r = await c.S.Request("mcp_authenticate", new() { ["serverName"] = "probe-auth" }, Seconds);
            Console.WriteLine($"  mcp_authenticate -> {r.GetRawText()}");
            if (McpAuthStart.Parse(r) is not { Url: { } url, CallbackExpected: true }) return [("FAIL", $"no usable authUrl with callbackExpected:true in {r.GetRawText()}")];

            using var browser = new HttpClient();   // follows authorize -> 302 -> the CLI's localhost callback
            var landed = await browser.GetAsync(url);
            var after = await Until(st => st == "connected");

            string cleared;
            try { await c.S.Request("mcp_clear_auth", new() { ["serverName"] = "probe-auth" }, Seconds); cleared = "success"; }
            catch (ClaudeRequestException ex) { cleared = $"error \"{ex.Message}\""; }
            var detail = $"{before} -> authUrl {new Uri(url).GetLeftPart(UriPartial.Path)}, callback HTTP {(int)landed.StatusCode}, "
                + $"token {(fake.TokenIssued ? "issued" : "never asked")}, bearer {(fake.BearerSeen ? "used" : "unused")} -> {after}; mcp_clear_auth {cleared}, then {await Status()}";
            return [after == "connected" && fake.BearerSeen ? ("PASS", detail) : ("FAIL", detail)];
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

    // ExitPlanMode reaches can_use_tool with {plan, planFilePath} and no permission_suggestions (2.1.296). A bare allow
    // drops to `default` on its own; PermissionCard sets the mode explicitly, which is what these three cases check:
    // plan-accept-edits / plan-review-edits: set_permission_mode {mode} then allow → system/status reports that mode and
    // the edit that follows is prompted in `default` only. plan-keep-planning: a deny's message comes back verbatim as
    // ExitPlanMode's tool_result, with no mode change and nothing written. Each runs in its own process, in parallel.
    // The CLI also saves each plan under ~/.claude/plans/: the probe leaves those files.
    static async Task<IEnumerable<(string, string, string)>> Plan(string dir) =>
        (await Task.WhenAll(PlanApprove(dir, "plan-accept-edits", "acceptEdits", "a.txt"), PlanApprove(dir, "plan-review-edits", "default", "b.txt"), PlanKeep(dir)))
        .SelectMany(r => r);

    static async Task<IEnumerable<(string, string, string)>> PlanApprove(string dir, string id, string mode, string file)
    {
        await using var c = await Cli.Start(dir, "--permission-mode", "plan");
        return await Guard([id], async () =>
        {
            string? set = null, suggestions = null;
            var prompts = 0;
            var turn = await c.Turn($"Plan how to create the file {file} containing the word hi, then call ExitPlanMode with the plan. Once it is approved, create the file.", async e =>
            {
                var r = e.GetProperty("request");
                if (Events.Str(r, "tool_name") == "ExitPlanMode" && set is null)
                {
                    suggestions = Events.Prop(r, "permission_suggestions") is { ValueKind: JsonValueKind.Array } sg
                        ? string.Join("+", sg.EnumerateArray().Select(x => Events.Str(x, "type"))) : "none";
                    set = Events.Str(await c.S.Request("set_permission_mode", new() { ["mode"] = mode }), "mode") ?? "?";
                }
                else if (Targets(r.GetProperty("input"), file)) prompts++;
                return false;   // then allowed as asked
            });
            var status = turn.Select(StatusMode).LastOrDefault(m => m is not null);
            var writes = turn.SelectMany(ToolBlocks).Count(b => Events.Str(b, "name") is "Write" or "Edit" && Targets(b.GetProperty("input"), file));
            var detail = $"suggestions {suggestions ?? "-"}, set_permission_mode → {set ?? "-"}, status {status ?? "absent"}, {writes} write(s) of {file}, {prompts} prompt(s)";
            if (set is null || writes == 0) return [("FAIL", "inconclusive: " + detail)];
            return [set == mode && status == mode && (mode == "default" ? prompts > 0 : prompts == 0) ? ("PASS", detail) : ("FAIL", detail)];
        });
    }

    static async Task<IEnumerable<(string, string, string)>> PlanKeep(string dir)
    {
        const string feedback = "Keep planning: the plan must also add a line saying hello to README.md.";
        await using var c = await Cli.Start(dir, "--permission-mode", "plan");
        return await Guard(["plan-keep-planning"], async () =>
        {
            string? toolUseId = null;
            var calls = 0;
            var turn = await c.Turn("Plan how to create the file c.txt containing the word hi, then call ExitPlanMode with the plan.", async e =>
            {
                var r = e.GetProperty("request");
                if (Events.Str(r, "tool_name") != "ExitPlanMode") return false;
                // The revised plan is refused too, so the turn ends without leaving plan mode.
                await c.S.Respond(Events.Str(e, "request_id")!, false, r.GetProperty("input"), message: calls++ == 0 ? feedback : "Stop here.");
                toolUseId ??= Events.Str(r, "tool_use_id");
                return true;
            });
            var result = turn.SelectMany(Events.ParseAll).OfType<ToolResultEvt>().FirstOrDefault(t => t.ToolUseId == toolUseId);
            var modes = turn.Select(StatusMode).OfType<string>().ToList();
            var created = File.Exists(Path.Combine(dir, "c.txt"));
            var detail = $"tool_result {(result is null ? "absent" : $"is_error={result.IsError} \"{result.Text}\"")}, status [{string.Join(",", modes)}], {calls} ExitPlanMode call(s), c.txt {(created ? "created" : "absent")}";
            if (toolUseId is null) return [("FAIL", "inconclusive: " + detail)];
            return [result is { IsError: true } && result.Text == feedback && modes.All(m => m == "plan") && !created ? ("PASS", detail) : ("FAIL", detail)];
        });
    }

    static string? StatusMode(JsonElement e) =>
        Events.Str(e, "type") == "system" && Events.Str(e, "subtype") == "status" ? Events.Str(e, "permissionMode") : null;

    static bool Targets(JsonElement input, string file) => Events.Str(input, "file_path") is { } f && Path.GetFileName(f) == file;


    // ask-user-question: an AskUserQuestion can_use_tool carries questions[{question,header,options[{label,description}],
    // multiSelect}]; allowing it with AskUser.WithAnswers (input + answers {question: text}) reaches the model, whose
    // next turn quotes a free-text answer it could not have guessed.
    const string Secret = "Chartreuse-417";
    static async Task<IEnumerable<(string, string, string)>> AskUserQuestion(string dir)
    {
        await using var c = await Cli.Start(dir);
        return await Guard(["ask-user-question"], async () =>
        {
            string? shape = null;
            var turn = await c.Turn("Use the AskUserQuestion tool to ask me one single-select question, \"Which color do you prefer?\", "
                + "header \"Color\", options \"Red\" and \"Blue\". After I answer, reply with exactly: CHOSEN=<my answer>.", async e =>
            {
                var r = e.GetProperty("request");
                if (Events.Str(r, "tool_name") != AskUser.Tool) return false;
                var input = r.GetProperty("input");
                var qs = AskUser.Parse(input);
                shape = string.Join("; ", qs.Select(q => $"\"{q.Header}\" {q.Options.Length} options multiSelect={q.Multi}"));
                await c.S.Respond(Events.Str(e, "request_id")!, true, AskUser.WithAnswers(input, qs.ToDictionary(q => q.Text, _ => Secret)));
                return true;
            });
            if (shape is null) return [("SKIP", "the model did not call AskUserQuestion")];
            var answered = turn.SelectMany(Events.ParseAll).OfType<ToolResultEvt>().LastOrDefault(r => AskUser.Answered(r.Structured).Length > 0)?.Text ?? "";
            var reply = Events.Str(turn[^1], "result") ?? "";
            var detail = $"{shape}; tool_result \"{answered[..Math.Min(answered.Length, 70)]}…\"; reply \"{reply}\"";
            return [answered.Contains(Secret) && reply.Contains(Secret) ? ("PASS", detail) : ("FAIL", detail)];
        });
    }

    // todo-tools: with LiveSession's --allowedTools opt-in, init.tools lists the task tools, and a turn that uses them
    // rebuilds, through Events.ParseAll + the reducer + TodoList.From, into the list the model was asked for.
    // todo-clear: after /clear the CLI numbers tasks from 1 again; the rebuilt list holds only the new task, updated.
    static async Task<IEnumerable<(string, string, string)>> TodoTools(string dir)
    {
        await using var c = await Cli.Start(dir, TodoList.AllowedToolsArg);
        return await Guard(["todo-tools", "todo-clear"], async () =>
        {
            var turn = await c.Turn("Use your task list tool to create exactly three tasks named alpha, beta and gamma, "
                + "then mark alpha completed and beta in progress. Use no other tool, then reply done.");
            var init = turn.FirstOrDefault(e => Events.Str(e, "type") == "system" && Events.Str(e, "subtype") == "init");
            var exposed = Events.Prop(init, "tools") is { ValueKind: JsonValueKind.Array } t
                ? string.Join("+", t.EnumerateArray().Select(x => x.GetString()).Where(n => TodoList.AllowedTools.Split(',').Contains(n)))
                : "no init";
            var used = string.Join(" ", turn.SelectMany(ToolUses).Where(n => n is "TodoWrite" or "TaskCreate" or "TaskUpdate")
                .GroupBy(n => n).Select(g => $"{g.Count()}×{g.Key}"));
            var s = new LiveSession("probe", "probe", dir, "default");
            foreach (var e in turn)
                foreach (var ev in Events.ParseAll(e)) s.Apply(ev);
            var list = string.Join(", ", s.Todos.Select(x => $"{x.Content}:{x.Status}"));
            var detail = $"init.tools {(exposed.Length > 0 ? exposed : "none")}; {(used.Length > 0 ? used : "no todo tool_use")}; list [{list}]";
            var tools = list == "alpha:Completed, beta:InProgress, gamma:Pending" ? ("PASS", detail) : ("FAIL", detail);

            var clear = await c.Turn("/clear");
            var after = await c.Turn("Use your task list tool to create exactly one task named delta, then mark it in progress. "
                + "Use no other tool, then reply done.");
            foreach (var e in clear.Concat(after))
                foreach (var ev in Events.ParseAll(e)) s.Apply(ev);
            var reset = clear.Any(e => Events.Str(e, "type") == "conversation_reset");
            var ids = string.Join(",", after.Select(e => Events.Prop(e, "tool_use_result") is { } r && Events.Prop(r, "task") is { } k ? Events.Str(k, "id") : null).OfType<string>());
            var list2 = string.Join(", ", s.Todos.Select(x => $"{x.Key}:{x.Content}:{x.Status}"));
            var detail2 = $"conversation_reset {(reset ? "seen" : "absent")}; TaskCreate ids [{ids}]; list [{list2}]";
            return [tools, reset && list2 == "1:delta:InProgress" ? ("PASS", detail2) : ("FAIL", detail2)];
        });
    }

    // image: a user message of a text block then image blocks (base64 source), the Composer's shape (Images.Content).
    // The model must read a random number drawn in a PNG generated here, and the fixed numbers of the JPEG, GIF and WebP
    // fixtures: reading them proves it sees each image, not just that the CLI accepted the message.
    // image-only: the same blocks without a text block, as the Composer sends an image with an empty prompt.
    static async Task<IEnumerable<(string, string, string)>> Image(string dir)
    {
        await using var c = await Cli.Start(dir);
        return await Guard(["image", "image-only"], async () =>
        {
            var n = Random.Shared.Next(100, 1000).ToString();
            string[] want = [n, .. Fixtures.Select(f => f.Number)];
            UserImage[] images = [Images.From(DigitsPng(n))!, .. Fixtures.Select(f => Images.From(Convert.FromBase64String(f.Data))!)];
            var turn = await c.Turn("Each attached image shows a 3-digit number. Reply with the numbers in order, comma-separated, nothing else.", images: images);
            var reply = Events.Str(turn[^1], "result") ?? "";
            var types = string.Join("+", images.Select(i => i.MediaType[6..]));
            var read = want.Count(reply.Contains);
            var first = read == want.Length && Events.Prop(turn[^1], "is_error") is not { ValueKind: JsonValueKind.True }
                ? ("PASS", $"{types}: expected {string.Join(",", want)}, model replied \"{Trunc(reply)}\"")
                : ("FAIL", $"{types}: {read}/{want.Length} read, expected {string.Join(",", want)}, model replied \"{Trunc(reply)}\"");

            var m = Random.Shared.Next(100, 1000).ToString();
            var alone = await c.Turn("", images: [Images.From(DigitsPng(m))!]);
            var said = Events.Str(alone[^1], "result") ?? "";
            return [first, Events.Prop(alone[^1], "is_error") is not { ValueKind: JsonValueKind.True }
                ? ("PASS", $"accepted, the reply {(said.Contains(m) ? "names" : "does not name")} the drawn {m}: \"{Trunc(said)}\"")
                : ("FAIL", $"result is_error: \"{Trunc(said)}\"")];
        });
        static string Trunc(string s) => (s.Length <= 80 ? s : s[..80] + "…").ReplaceLineEndings(" ");
    }

    // JPEG and GIF made with sips, WebP with cwebp -lossless, from DigitsPng(n, 8); each decodes back pixel-identical.
    static readonly (string Number, string Data)[] Fixtures =
    [
        ("305", "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAoHBwgHBgoICAgLCgoLDhgQDg0NDh0VFhEYIx8lJCIfIiEmKzcvJik0KSEiMEExNDk7Pj4+JS5ESUM8SDc9Pjv/wAALCABIAJgBAREA/8QAFgABAQEAAAAAAAAAAAAAAAAAAAgH/8QAFBABAAAAAAAAAAAAAAAAAAAAAP/aAAgBAQAAPwDZgAAAAABGYCzARmCzBGYAswAEZrMRmswEZrMRmswAABGazEZrMEZizEZgLMARmCzBGazEZrMRmswARmswAEZrMRmLMEZrMAEZrMABGazEZrMBGazEZrMBGazBGYCzARmCzARmCzAAAAAAAf/Z"),
        ("718", "R0lGODdhmABIAIAAAAAAAP///yH5BAQAAAAALAAAAACYAEgAAAL/jI+py+0Po5y02ouz3rz7D4biSJbmiabqyrbuC8fyTNPAjef6zvc51yP5hjog8YjEGXfCpG/pjDI3wZGUB71es0qr9kf9arm3phhAPhPTZnFa/QxPRfA3HCsves+1RVUSxZYHJvMXETjYpWEIw/iAuIgXORfj6ACZYVmhycLp5yQ4qVcoeQiaWIaKNuOpgInRakrZWArxehFrW3uSi7tr0Xv5WxK8OUxRzJCs2kaYeTyxLOoMIp1gjYDtO/uhbeAdAI4MPa34zA1MHiL+iW7sPg6/N7qurisPaL9Nj8LuqtZJnwd/1wCuIHiPXzeDKhA+EvjOzUGIoerxmYjPosKB2wxTOBSWEVbHfhSZzfvSzBytkA9ZgtwYkdoLcDRL/nNJzGZBnMp0ZvO5kGc7mPGIRgPKUehNo/mU7mTqEWk4qTWdVqOK1eqBjzFV7pOZTus3qRVFip16lmtRsF1TlXN7DirJtFnltrRroireBnrZBqQLeG/Ps2W/eg0reKhfjImXLm7a+OnjqIEnJ7R8F3NSiW+lpIQ7l3NclCcP57zY+dZmzR3qqFLdmuxR1KPHlAbNa6ThLbdXNdSNmDQd2ZCF1+bdJ7ny5cybO38OPbr06dSrW7+OPbv27SwKAAA7"),
        ("264", "UklGRkIAAABXRUJQVlA4TDYAAAAvl8ARAA8w//M///MfeBALJvlLz6A7ov8TYGwFBfoQIqARkIC8iCpiRQipyoYgBEELqqKyYiw="),
    ];

    // A greyscale PNG of the digits in a 5x7 dot font, `scale` px per dot, black on white, one dot of margin.
    internal static byte[] DigitsPng(string digits, int scale = 12)
    {
        string[] font = ["01110100011001110101110011000101110", "00100011000010000100001000010001110", "01110100010000100010001000100011111",
                         "11110000010000101110000010000111110", "00010001100101010010111110001000010", "11111100001111000001000011000101110",
                         "00110010001000011110100011000101110", "11111000010001000100010000100001000", "01110100011000101110100011000101110",
                         "01110100011000101111000010001001100"];
        int w = (digits.Length * 6 + 1) * scale, h = 9 * scale;
        var raw = new byte[(w + 1) * h];   // each row: filter byte 0, then w grey pixels
        for (var y = 0; y < h; y++)
            for (var x = 0; x < w; x++)
            {
                int gy = y / scale - 1, gx = x / scale - 1;
                var on = gy is >= 0 and < 7 && gx >= 0 && gx / 6 < digits.Length && gx % 6 < 5 && font[digits[gx / 6] - '0'][gy * 5 + gx % 6] == '1';
                raw[y * (w + 1) + 1 + x] = on ? (byte)0 : (byte)255;
            }
        var idat = new MemoryStream();
        using (var z = new ZLibStream(idat, CompressionLevel.SmallestSize)) z.Write(raw);
        var png = new MemoryStream();
        png.Write([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
        Chunk("IHDR", [.. Be(w), .. Be(h), 8, 0, 0, 0, 0]);   // 8-bit greyscale
        Chunk("IDAT", idat.ToArray());
        Chunk("IEND", []);
        return png.ToArray();

        void Chunk(string type, byte[] data)
        {
            byte[] body = [.. Encoding.ASCII.GetBytes(type), .. data];
            png.Write(Be(data.Length));
            png.Write(body);
            png.Write(Be((int)Crc32(body)));
        }
        static byte[] Be(int v) => [(byte)(v >> 24), (byte)(v >> 16), (byte)(v >> 8), (byte)v];
        static uint Crc32(byte[] b)
        {
            var c = ~0u;
            foreach (var x in b)
            {
                c ^= x;
                for (var k = 0; k < 8; k++) c = (c >> 1) ^ (0xEDB88320u & (0u - (c & 1)));
            }
            return ~c;
        }
    }

    // file-mention: in stream-json mode the CLI expands "@path" in the user text itself. With every file-reading tool
    // disallowed, the model can only quote a random code word from a file it was never shown if the CLI attached it.
    // file-mention-quoted: the same through the `@"path with spaces"` form the Composer inserts.
    // file-mention-folder: `@folder/` attaches a listing: the model names a randomly named file inside it.
    static async Task<IEnumerable<(string, string, string)>> FileMention(string dir)
    {
        static string Word() => "CODE-" + Guid.NewGuid().ToString("N")[..8].ToUpperInvariant();
        var (word, spaced, listed) = (Word(), Word(), Word());
        Directory.CreateDirectory(Path.Combine(dir, "notes"));
        File.WriteAllText(Path.Combine(dir, "notes", "secret.txt"), $"The code word is {word}.\n");
        Directory.CreateDirectory(Path.Combine(dir, "my docs"));
        File.WriteAllText(Path.Combine(dir, "my docs", "the word.txt"), $"The code word is {spaced}.\n");
        Directory.CreateDirectory(Path.Combine(dir, "box"));
        File.WriteAllText(Path.Combine(dir, "box", listed + ".txt"), "");
        await using var c = await Cli.Start(dir, "--disallowedTools", "Read,Bash,Glob,Grep,Task,Agent,LS");
        return await Guard(["file-mention", "file-mention-quoted", "file-mention-folder"], async () =>
        {
            async Task<(string, string)> Ask(string prompt, string expected)
            {
                var turn = await c.Turn(prompt + " Reply with only that, and do not use any tool.");
                var tools = turn.Sum(e => ToolUses(e).Count());
                var said = string.Concat(turn.Where(e => Events.Str(e, "type") == "assistant" && Events.Prop(e, "message") is { } m && Events.Prop(m, "content") is { ValueKind: JsonValueKind.Array })
                    .SelectMany(e => e.GetProperty("message").GetProperty("content").EnumerateArray()).Where(b => Events.Str(b, "type") == "text").Select(b => Events.Str(b, "text")));
                var detail = $"{tools} tool_use, answer \"{said.Trim()}\"";
                return tools == 0 && said.Contains(expected) ? ("PASS", $"{expected} quoted from the @-mentioned path: {detail}") : ("FAIL", $"expected {expected}: {detail}");
            }
            return [await Ask("What is the code word in @notes/secret.txt ?", word),
                    await Ask("What is the code word in @\"my docs/the word.txt\" ?", spaced),
                    await Ask("What is the name of the only file in @box/ ?", listed)];
        });
    }

    // prompt-history: prompts sent over stream-json land in ~/.claude/projects/<slug of the resolved cwd>/<id>.jsonl as
    // user lines TranscriptStore.Prompts reads back, newest first (the Composer's ↑ history of past sessions).
    static async Task<IEnumerable<(string, string, string)>> History(string dir)
    {
        var tag = Guid.NewGuid().ToString("N")[..8];
        string[] sent = [$"Reply with only the word one. ({tag})", $"Reply with only the word two. ({tag})"];
        return await Guard(["prompt-history"], async () =>
        {
            await using (var c = await Cli.Start(dir))
                foreach (var p in sent) await c.Turn(p);   // disposed: the CLI has exited and flushed its transcript
            var got = TranscriptStore.Prompts(dir).Where(p => p.Contains(tag)).ToList();
            var detail = $"{TranscriptStore.Slug(TranscriptStore.Real(dir))}: [{string.Join(" | ", got)}]";
            return [got.SequenceEqual(sent.Reverse()) ? ("PASS", detail) : ("FAIL", detail)];
        });
    }

    // A user message written to stdin mid-turn, with its own uuid: the CLI reports its fate as command_lifecycle frames
    // keyed on that uuid (LiveSession.Send / Cancel and the composer's queued chips rest on these four).
    // queue-next-turn: during a text-only turn it is `queued`, `started` after that turn's result, and answered by its own result.
    // queue-fold: written while a tool runs, it is `started` at the tool result and the turn's single result lists both uuids.
    // queue-cancel: cancel_async_message {message_uuid} answers {cancelled:true}, the frame is `cancelled`, no turn runs it.
    // queue-interrupt: an interrupt leaves it queued; it is `started` after the aborted result and answered by its own result.
    static async Task<IEnumerable<(string, string, string)>> Queue(string dir)
    {
        await using var c = await Cli.Start(dir, "--include-partial-messages");
        return await Guard(["queue-next-turn", "queue-fold", "queue-cancel", "queue-interrupt"], async () =>
        {
            const string Numbers = "Write the numbers from 1 to 120 as words, one per line. Nothing else.";
            const string Second = "Reply with exactly the word PINEAPPLE.";
            static bool Streaming(JsonElement e) => Events.Str(e, "type") == "stream_event" && Events.Prop(e, "event") is { } se && Events.Str(se, "type") == "content_block_delta";
            static bool ToolUse(JsonElement e) => ToolUses(e).Any();
            static bool IsResult(JsonElement e) => Events.Str(e, "type") == "result";
            static bool Lists(JsonElement r, string u) => Events.Prop(r, "user_message_uuids") is { ValueKind: JsonValueKind.Array } a && a.EnumerateArray().Any(x => x.GetString() == u);
            static string? Uuids(JsonElement r) => Events.Prop(r, "user_message_uuids")?.GetRawText();
            // The states of u, each with the number of results seen before it: "queued@0 started@1".
            static string Fate(List<JsonElement> seen, string u)
            {
                var (results, states) = (0, new List<string>());
                foreach (var e in seen)
                    if (IsResult(e)) results++;
                    else if (Events.Str(e, "type") == "command_lifecycle" && Events.Str(e, "command_uuid") == u) states.Add($"{Events.Str(e, "state")}@{results}");
                return string.Join(" ", states);
            }
            // Sends `first`, writes Second under its own uuid on the first event matching `when`, then sends the `then` request.
            async Task<(List<JsonElement> Seen, string U, JsonElement? Answer)> Mid(string first, Func<JsonElement, bool> when, int results, string? then = null)
            {
                var u = Guid.NewGuid().ToString();
                JsonElement? answer = null;
                var sent = false;
                await c.S.SendUser(first, uuid: Guid.NewGuid().ToString());
                var seen = await c.Collect(results, onEvent: async e =>
                {
                    if (sent || !when(e)) return;
                    sent = true;
                    await c.S.SendUser(Second, uuid: u);
                    if (then is not null) answer = await c.S.Request(then, then == "cancel_async_message" ? new() { ["message_uuid"] = u } : null);
                });
                return (seen, u, answer);
            }

            var (s1, u1, _) = await Mid(Numbers, Streaming, 2);
            var f1 = Fate(s1, u1);
            var next = f1 == "queued@0 started@1" && Lists(s1[^1], u1)
                ? ("PASS", $"{f1}, second result lists it")
                : ("FAIL", $"{f1}, second result user_message_uuids={Uuids(s1[^1])}");

            var (s2, u2, _) = await Mid("Run the shell command `sleep 3; echo one`, then reply DONE.", ToolUse, 1);
            var f2 = Fate(s2, u2);
            // The thread puts the message after the tool result it joined: `started` must come after that tool_result.
            var toolResult = s2.FindIndex(e => Events.ParseAll(e).Any(x => x is ToolResultEvt));
            var startedAt = s2.FindIndex(e => Events.Str(e, "type") == "command_lifecycle" && Events.Str(e, "command_uuid") == u2 && Events.Str(e, "state") == "started");
            var fold = f2.StartsWith("queued@0 started@0") && Lists(s2[^1], u2) && toolResult >= 0 && startedAt > toolResult
                ? ("PASS", $"{f2}, started after the tool_result (#{toolResult} < #{startedAt}), the one result lists {Uuids(s2[^1])?.Split(',').Length} uuids")
                : ("FAIL", $"{f2}, tool_result #{toolResult}, started #{startedAt}, result user_message_uuids={Uuids(s2[^1])}");

            var (s3, u3, a3) = await Mid(Numbers, Streaming, 1, "cancel_async_message");
            var f3 = Fate(s3, u3);
            var cancel = a3 is { } r3 && Events.Prop(r3, "cancelled") is { ValueKind: JsonValueKind.True } && f3 == "queued@0 cancelled@0" && !Lists(s3[^1], u3)
                ? ("PASS", $"answered {a3?.GetRawText()}, {f3}")
                : ("FAIL", $"answered {a3?.GetRawText() ?? "nothing"}, {f3}");

            var (s4, u4, a4) = await Mid(Numbers, Streaming, 2, "interrupt");
            var f4 = Fate(s4, u4);
            var aborted = Events.Str(s4.First(IsResult), "terminal_reason") ?? "";
            var interrupt = aborted.StartsWith("aborted") && f4 == "queued@0 started@1" && Lists(s4[^1], u4)
                ? ("PASS", $"interrupt answered {a4?.GetRawText()}, first result {aborted}, {f4}")
                : ("FAIL", $"interrupt answered {a4?.GetRawText()}, first result {aborted}, {f4}");
            return [next, fold, cancel, interrupt];
        });
    }

    // notify-*: what a desktop notification tells apart (Notifications.cs). A decision (permission, AskUserQuestion, ExitPlanMode)
    // reaches us as a can_use_tool named after the tool, and every turn, answered or denied, still ends with a `result`.
    // The plan is entered with set_permission_mode, as the mode menu does.
    static async Task<IEnumerable<(string, string, string)>> Waiting(string dir)
    {
        await using var c = await Cli.Start(dir);
        return await Guard(["notify-permission", "notify-question", "notify-plan"], async () =>
        {
            async Task<(string, string)> Case(string want, string prompt, bool allow)
            {
                var asked = new List<string>();
                var turn = await c.Turn(prompt, async e =>
                {
                    var r = e.GetProperty("request");
                    asked.Add(Events.Str(r, "tool_name") ?? "?");
                    await c.S.Respond(Events.Str(e, "request_id")!, allow, r.GetProperty("input"));
                    return true;
                });
                var detail = $"can_use_tool [{string.Join(", ", asked)}], turn ended by result/{Events.Str(turn[^1], "subtype")}";
                return (asked.Contains(want) ? "PASS" : "FAIL", detail);
            }

            var permission = await Case("Bash", $"Run the shell command: {Cmd}", true);
            var question = await Case("AskUserQuestion",
                "Use the AskUserQuestion tool to ask me whether I prefer red or blue (two options). Do nothing else.", false);
            await c.S.Request("set_permission_mode", new() { ["mode"] = "plan" });
            var plan = await Case("ExitPlanMode",
                "Plan adding a one-line CONTRIBUTING.md to this repo, then present the plan for approval with ExitPlanMode.", false);
            return [permission, question, plan];
        });
    }

    // rewind-files: rewind_files {user_message_id: the uuid sent with a user message} lists (dry_run) then puts back on
    // disk what that turn and every later one changed, also once rewind_conversation already ran (LiveSession.Rewind's
    // order). Needs CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING, which ClaudeSession sets.
    // rewind-conversation: rewind_conversation {target_message_uuid} refuses any message but the latest ("stale target"),
    // so a rewind goes newest first back to its target; each step answers rewound, the target's prefillText is its text,
    // and the next turn and the transcript replay (TranscriptStore) no longer have the dropped messages. Run twice: to a
    // message that is not the last, then to the first message.
    static async Task<IEnumerable<(string, string, string)>> Rewind(string dir)
    {
        await using var c = await Cli.Start(dir);
        return await Guard(["rewind-files", "rewind-conversation"], async () =>
        {
            const string first = "Remember the code word PELICAN. Reply with just OK.";
            const string second = "Use the Write tool to create notes.txt containing exactly: first. Then use the Edit tool to change the first line of README.md to: # changed. Reply DONE.";
            const string third = "Use the Write tool to create extra.txt containing exactly: x. Reply DONE.";
            const string echo = "Reply with the exact text of my previous message, or NONE if there is none, nothing else.";
            string readme = Path.Combine(dir, "README.md"), notes = Path.Combine(dir, "notes.txt"), extra = Path.Combine(dir, "extra.txt");
            var before = File.ReadAllText(readme);
            string u1 = Guid.NewGuid().ToString(), u2 = Guid.NewGuid().ToString(), u3 = Guid.NewGuid().ToString(), u4 = Guid.NewGuid().ToString();
            await c.Turn(first, uuid: u1);
            await c.Turn(second, uuid: u2);
            var session = (await c.Turn(third, uuid: u3)).Select(e => Events.Str(e, "session_id")).LastOrDefault(x => x is not null) ?? "";
            if (!File.Exists(notes) || !File.Exists(extra) || File.ReadAllText(readme) == before)
                return [("FAIL", "inconclusive: the turns did not write notes.txt and extra.txt and edit README.md"), ("SKIP", "needs rewind-files")];

            var dry = await c.S.Request("rewind_files", new() { ["user_message_id"] = u2, ["dry_run"] = true });
            var listed = Events.Prop(dry, "filesChanged") is { ValueKind: JsonValueKind.Array } f ? f.GetArrayLength() : 0;

            // u2 is not the latest message: refused, files untouched. Then u3, u2 (the steps LiveSession.Rewind sends).
            string stale;
            try { stale = (await c.S.Request("rewind_conversation", new() { ["target_message_uuid"] = u2 })).GetRawText(); }
            catch (ClaudeRequestException ex) { stale = $"error \"{ex.Message}\""; }
            var untouched = File.Exists(notes) && File.Exists(extra);
            var toU2 = new[] { await Conv(u3), await Conv(u2) };
            string answer;
            try { await c.S.Request("rewind_files", new() { ["user_message_id"] = u2 }); answer = "rewound"; }
            catch (ClaudeRequestException ex) { answer = $"error \"{ex.Message}\""; }
            var restored = !File.Exists(notes) && !File.Exists(extra) && File.ReadAllText(readme) == before;
            var files = $"dry_run canRewind={Events.Prop(dry, "canRewind")?.GetRawText()}, {listed} filesChanged; after rewind_conversation: {answer}, files {(restored ? "restored" : "NOT restored")}";

            var seen2 = string.Concat((await c.Turn(echo, uuid: u4)).SelectMany(Texts)).Trim();
            var replay2 = await Replayed(session, n => n == 2);
            // Back to the first message: u4, then u1.
            var toU1 = new[] { await Conv(u4), await Conv(u1) };
            var replay0 = await Replayed(session, n => n == 0);
            var seen0 = string.Concat((await c.Turn(echo)).SelectMany(Texts)).Trim();

            var ok = stale.Contains("stale") && untouched && toU2.All(r => r.Rewound) && toU2[^1].Prefill == second
                     && seen2.Contains("PELICAN") && !seen2.Contains("notes.txt") && replay2 is [first, echo]
                     && toU1.All(r => r.Rewound) && toU1[^1].Prefill == first && replay0 is [] && !seen0.Contains("PELICAN");
            return [(listed == 3 && untouched && restored ? "PASS" : "FAIL", files),
                (ok ? "PASS" : "FAIL", $"to u2 (not last) directly: {stale}, files {(untouched ? "untouched" : "CHANGED")}; "
                    + $"u3,u2 rewound={string.Join(',', toU2.Select(r => r.Rewound))}, prefillText {(toU2[^1].Prefill == second ? "=" : "≠")} u2; next turn sees \"{seen2}\", "
                    + $"replay [{string.Join(" | ", replay2)}]; to the first message u4,u1 rewound={string.Join(',', toU1.Select(r => r.Rewound))}, "
                    + $"prefillText {(toU1[^1].Prefill == first ? "=" : "≠")} u1, replay [{string.Join(" | ", replay0)}], next turn sees \"{seen0}\"")];
        });

        async Task<(bool Rewound, string? Prefill)> Conv(string uuid)
        {
            var r = await c.S.Request("rewind_conversation", new() { ["target_message_uuid"] = uuid });
            return (Events.Prop(r, "rewound") is { ValueKind: JsonValueKind.True }, Events.Str(r, "prefillText"));
        }
    }

    // User texts TranscriptStore replays from the session's file, once `done` holds on their count (the CLI writes the
    // file asynchronously) or after 5 s.
    static async Task<string[]> Replayed(string session, Func<int, bool> done)
    {
        string[] texts = [];
        for (var i = 0; i < 20; i++)
        {
            texts = [.. TranscriptStore.Load(session).OfType<UserItem>().Select(u => u.Text)];
            if (done(texts.Length)) break;
            await Task.Delay(250);
        }
        return texts;
    }

    // fork: --resume <A> --fork-session --session-id <B> continues A's conversation under the id the UI chose
    // (system/init session_id == B, the model knows A's last codeword, B.jsonl exists) and leaves A.jsonl byte-identical.
    // fork-at: + --resume-session-at <uuid of A's first assistant message> forks the conversation as it stood there.
    // fork-name: --name given with the fork becomes the title the "Recent" list reads from the fork's transcript.
    // fork-app: the app's own path, SessionManager.Fork(original, uuid) then Send, shows the cut history and gets PAPAYA.
    // fork-of-fork: forking that fork before its first message (no transcript of its own yet) forks the original at the
    // same uuid and gets PAPAYA; the raw `--resume <unsent fork>` this avoids is reported alongside.
    static Task<IEnumerable<(string, string, string)>> Fork(string dir) => Guard(["fork", "fork-at", "fork-name", "fork-app", "fork-of-fork"], async () =>
    {
        var a = Guid.NewGuid().ToString();
        string? at;
        await using (var c = await Cli.Start(dir, "--session-id", a))
        {
            at = Events.Str((await c.Turn("Remember the codeword PAPAYA. Reply with just OK.")).Last(e => Events.Str(e, "type") == "assistant"), "uuid");
            await c.Turn("The codeword is now MANGO. Reply with just OK.");
        }
        var original = TranscriptStore.Find(a) is { } f ? File.ReadAllText(f) : null;
        if (original is null) return [("FAIL", "no transcript for the original session"), ("SKIP", "no original"), ("SKIP", "no original"), ("SKIP", "no original"), ("SKIP", "no original")];
        string? title = null;

        async Task<string> Ask(params string[] extra)
        {
            var b = Guid.NewGuid().ToString();
            extra = [.. extra, "--name", "probe (fork)"];
            string? init, answer;
            await using (var c = await Cli.Start(dir, ["--resume", a, "--fork-session", "--session-id", b, .. extra]))
            {
                var turn = await c.Turn("What is the current codeword? Answer with one word.");
                init = turn.Where(e => Events.Str(e, "subtype") == "init").Select(e => Events.Str(e, "session_id")).FirstOrDefault();
                answer = Events.Str(turn[^1], "result");
            }
            title ??= TranscriptStore.Find(b) is { } t ? TranscriptStore.Read(new FileInfo(t))?.Title : null;
            return $"init.session_id {(init == b ? "= new id" : init ?? "absent")}, answer \"{answer?.Trim()}\", "
                 + $"{(TranscriptStore.Find(b) is null ? "no" : "new")} transcript, original {(TranscriptStore.Find(a) is { } p && File.ReadAllText(p) == original ? "unchanged" : "CHANGED")}";
        }
        static bool Ok(string d, string word) => d.StartsWith("init.session_id = new id") && d.Contains(word, StringComparison.OrdinalIgnoreCase)
                                                 && d.Contains("new transcript") && d.EndsWith("original unchanged");

        var whole = await Ask();
        var fork = Ok(whole, "MANGO") ? ("PASS", whole) : ("FAIL", whole);
        var name = title == "probe (fork)" ? ("PASS", $"title \"{title}\"") : ("FAIL", $"title \"{title}\"");
        if (at is null) return [fork, ("FAIL", "no uuid on the assistant event"), name, ("SKIP", "no uuid"), ("SKIP", "no uuid")];
        var part = await Ask("--resume-session-at", at);
        var forkAt = Ok(part, "PAPAYA") && !part.Contains("MANGO", StringComparison.OrdinalIgnoreCase) ? ("PASS", part) : ("FAIL", part);

        await using var sm = new SessionManager();
        var app = sm.Fork(sm.Open(TranscriptStore.Read(new FileInfo(TranscriptStore.Find(a)!))!), at);
        var history = app.Items.Count;
        var cut = app.Items is [UserItem { Text: var u }, TextItem { Uuid: var x }] && u.Contains("PAPAYA") && x == at;

        // fork-of-fork: the fork, booted but not sent to yet, has no transcript, so the CLI has nothing under its id;
        // SessionManager.Fork must fork the fork's source at the same cut instead.
        while (app.Status == SessionStatus.Starting) await Task.Delay(200);
        var unsent = TranscriptStore.Find(app.Id) is null ? "no transcript after boot" : "a transcript after boot";
        var exit = new TaskCompletionSource<string>(TaskCreationOptions.RunContinuationsAsynchronously);
        string raw;
        await using (new ClaudeSession(dir, ["--model", "haiku", "--permission-mode", "manual", "--resume", app.Id, "--fork-session", "--session-id", Guid.NewGuid().ToString()],
                         _ => Task.CompletedTask, (_, text) => exit.TrySetResult(text)))
            try { raw = await exit.Task.WaitAsync(TimeSpan.FromSeconds(90)); }
            catch (TimeoutException) { raw = "still running after 90 s"; }
        var again = sm.Fork(app);
        var inherits = again.ForkOf == a && again.ForkAt == at && again.Items.Count == history;
        var history2 = again.Items.Count;

        static async Task<string?> Answer(LiveSession s)
        {
            await s.Send("What is the current codeword? Answer with one word.");
            while (s.LastResultAt is null && s.Status != SessionStatus.Crashed) await Task.Delay(200);
            return s.Items.OfType<TextItem>().LastOrDefault()?.Markdown.Trim();
        }
        static bool Got(LiveSession s, string? reply) => s.Status == SessionStatus.Idle && TranscriptStore.Find(s.Id) is not null
                                                        && reply?.Contains("PAPAYA", StringComparison.OrdinalIgnoreCase) == true;

        var reply2 = await Answer(again);   // first, while app still has no transcript of its own
        var detail2 = $"source fork had {unsent}; raw --resume <it> --fork-session: {raw}; app fork of it "
                    + $"{(inherits ? "forks the original at the same uuid" : $"ForkOf {again.ForkOf} ForkAt {again.ForkAt}")}, {history2} history items, "
                    + $"status {again.Status} {again.LastResultSubtype}, answer \"{reply2}\", {(TranscriptStore.Find(again.Id) is null ? "no" : "new")} transcript";
        var reply = await Answer(app);
        var detail = $"{history} history items{(cut ? " ending at the uuid" : "")}, status {app.Status}, answer \"{reply}\", {(TranscriptStore.Find(app.Id) is null ? "no" : "new")} transcript";
        return [fork, forkAt, name, cut && Got(app, reply) ? ("PASS", detail) : ("FAIL", detail), inherits && Got(again, reply2) ? ("PASS", detail2) : ("FAIL", detail2)];
    }, 600);

    // background-tasks: a run_in_background shell command reports task_started (task_type local_bash, is_backgrounded)
    // and get_task_output returns the end of what it printed so far.
    // stop-task: stop_task on it is answered and followed by task_updated patch.status "killed".
    // task-completed: a background command that exits on its own is reported by task_updated "completed" with an end_time.
    static async Task<IEnumerable<(string, string, string)>> BackgroundTasks(string dir)
    {
        await using var c = await Cli.Start(dir);
        return await Guard(["background-tasks", "stop-task", "task-completed"], async () =>
        {
            var turn = await c.Turn("With your shell tool and run_in_background set to true, run a command that prints \"tick N\" once per second for 60 seconds. "
                + "Do not wait for it and do not read its output. Reply only 'started'.");
            if (turn.Select(Events.Parse).OfType<TaskStartedEvt>().FirstOrDefault(t => t.TaskType == "local_bash") is not { } ts)
                return [("FAIL", "no task_started with task_type local_bash"), ("SKIP", "no background task to stop"), ("SKIP", "no background task")];
            await Task.Delay(3000);
            var o = await c.S.Request("get_task_output", new() { ["task_id"] = ts.TaskId });
            var output = Events.Str(o, "output") ?? "";
            // The panel shows the launching tool's input.command.
            var cmd = turn.SelectMany(Events.ParseAll).OfType<ToolUseEvt>().FirstOrDefault(u => u.Id == ts.ToolUseId) is { } launcher ? Events.Str(launcher.Input, "command") : null;
            var detail = $"task {ts.TaskId} is_backgrounded={ts.Backgrounded}, input.command {(cmd is null ? "absent" : "present")}, "
                + $"output {Events.Prop(o, "total_bytes")?.GetRawText() ?? "?"} bytes \"{output.Trim().Split('\n')[^1]}\"";
            var read = ts.Backgrounded && cmd is not null && output.Contains("tick") ? ("PASS", detail) : ("FAIL", detail);

            var stop = await Stopped(c, ts.TaskId);

            turn = await c.Turn("With your shell tool and run_in_background set to true, run: echo done; sleep 2. Do not wait for it. Reply only 'started'.");
            if (turn.Select(Events.Parse).OfType<TaskStartedEvt>().FirstOrDefault(t => t.TaskType == "local_bash") is not { } t2)
                return [read, stop, ("FAIL", "second command: no task_started with task_type local_bash")];
            // It may end inside the turn, before the result: look there first.
            bool Done(JsonElement e) => Events.Parse(e) is TaskUpdatedEvt { Status: not (null or "running" or "pending") } u && u.TaskId == t2.TaskId;
            var done = Events.Parse(turn.FirstOrDefault(Done) is { ValueKind: JsonValueKind.Object } inTurn ? inTurn : await c.Until(Done)) as TaskUpdatedEvt;
            return [read, stop, done is { Status: "completed", EndedAt: not null }
                ? ("PASS", $"task {t2.TaskId} task_updated status completed, end_time {done.EndedAt:O}")
                : ("FAIL", $"task {t2.TaskId} task_updated status {done?.Status}, end_time {done?.EndedAt}")];
        });
    }

    // monitor-stop: a Monitor command is a local_bash task too, and stop_task ends it (task_updated "killed"). stop_task
    // answers {} even for an id it ignores, so only the task_updated proves the stop.
    static async Task<IEnumerable<(string, string, string)>> MonitorStop(string dir)
    {
        await using var c = await Cli.Start(dir);
        return await Guard(["monitor-stop"], async () =>
        {
            var turn = await c.Turn("Use the Monitor tool (not the shell tool) to watch a command that prints \"tick N\" once per second for 60 seconds. "
                + "Do not wait for it. Reply only 'started'.");
            if (turn.SelectMany(Events.ParseAll).OfType<ToolUseEvt>().FirstOrDefault(u => u.Name == "Monitor") is not { } monitor)
                return [("SKIP", $"haiku called no Monitor tool ({string.Join(", ", turn.SelectMany(ToolUses).Distinct())})")];
            if (turn.Select(Events.Parse).OfType<TaskStartedEvt>().FirstOrDefault(t => t.ToolUseId == monitor.Id) is not { } ts)
                return [("FAIL", "Monitor called, no task_started for it")];
            // The panel lists local_bash + is_backgrounded tasks and shows input.command.
            var listed = ts is { TaskType: "local_bash", Backgrounded: true };
            var stop = await Stopped(c, ts.TaskId);
            return [(listed ? stop.Verdict : "FAIL", $"task_type {ts.TaskType} is_backgrounded={ts.Backgrounded} "
                + $"input.command {(Events.Str(monitor.Input, "command") is null ? "absent" : "present")}; {stop.Detail}")];
        });
    }

    // exit-ends-tasks: ending the claude process (ClaudeSession.DisposeAsync → ProcessJob) takes its background shells
    // with it, which LiveSession.EndTools records as "killed". Unix only (pgrep); a sleep of a random length marks the shell.
    static async Task<IEnumerable<(string, string, string)>> ExitEndsTasks(string dir)
    {
        if (OperatingSystem.IsWindows()) return [("SKIP", "exit-ends-tasks", "pgrep: Unix only")];
        var marker = $"sleep {Random.Shared.Next(600, 999)}";
        var c = await Cli.Start(dir);
        var r = await Guard(["exit-ends-tasks"], async () =>
        {
            var turn = await c.Turn($"With your shell tool and run_in_background set to true, run exactly: {marker}. Do not wait for it. Reply only 'started'.");
            if (turn.Select(Events.Parse).OfType<TaskStartedEvt>().FirstOrDefault(t => t.TaskType == "local_bash") is not { } ts)
                return [("FAIL", "no task_started with task_type local_bash")];
            await Task.Delay(2000);
            var before = Pgrep(marker);   // the witness: without it "gone after exit" would prove nothing
            await c.DisposeAsync();
            await Task.Delay(1000);
            var after = Pgrep(marker);
            return [(before.Length > 0 && after.Length == 0 ? "PASS" : "FAIL",
                $"task {ts.TaskId} \"{marker}\": pids [{before}] while claude runs, [{after}] 1 s after it is disposed")];
        });
        await c.DisposeAsync();
        return r;
    }

    static string Pgrep(string pattern)
    {
        var psi = new ProcessStartInfo("pgrep") { RedirectStandardOutput = true };
        psi.ArgumentList.Add("-f");
        psi.ArgumentList.Add(pattern);
        using var p = Process.Start(psi)!;
        var pids = p.StandardOutput.ReadToEnd().Trim().ReplaceLineEndings(" ");
        p.WaitForExit();
        return pids;
    }

    // stop_task, then the task_updated that reports the task's new status.
    static async Task<(string Verdict, string Detail)> Stopped(Cli c, string taskId)
    {
        await c.S.Request("stop_task", new() { ["task_id"] = taskId });
        var upd = await c.Until(e => Events.Parse(e) is TaskUpdatedEvt { Status: not null } u && u.TaskId == taskId);
        var status = (Events.Parse(upd) as TaskUpdatedEvt)?.Status;
        return status == "killed" ? ("PASS", "answered, task_updated status killed") : ("FAIL", $"answered, task_updated status {status}");
    }

    // hooks-listing: get_hooks_listing returns the hooks of the folder's .claude/settings.json with event, matcher, type,
    // command and source (the Extensions page's Hooks tab).
    // hooks-events: with --include-hook-events (LiveSession.Args) a PreToolUse hook that exits 2 streams a hook_response
    // (outcome error, exit 2, its stderr) and the tool does not run; a hook that prints streams its output.
    // hooks-subagent: a SubagentStart hook streams between the Agent tool_use and its tool_result, named SubagentStart:<type>,
    // and its hookSpecificOutput.additionalContext output is hidden by the reducer (LiveSession.Shown); PostToolBatch streams
    // after the tool_result under that name.
    const string Blocked = "ccui-probe-blocked.txt";
    static async Task<IEnumerable<(string, string, string)>> Hooks(string dir)
    {
        var settings = Path.Combine(dir, ".claude");
        Directory.CreateDirectory(settings);
        File.WriteAllText(Path.Combine(settings, "settings.json"), """
            {"hooks":{
              "PreToolUse":[{"matcher":"Write","hooks":[{"type":"command","command":"echo ccui-probe-block >&2; exit 2"}]}],
              "UserPromptSubmit":[{"hooks":[{"type":"command","command":"echo ccui-probe-prompt","timeout":5}]}],
              "SubagentStart":[{"hooks":[{"type":"command","command":"echo '{\"hookSpecificOutput\":{\"hookEventName\":\"SubagentStart\",\"additionalContext\":\"ccui-probe-ctx\"}}'"}]}],
              "PostToolBatch":[{"hooks":[{"type":"command","command":"echo ccui-probe-batch"}]}]}}
            """);
        try
        {
            await using var c = await Cli.Start(dir, "--include-hook-events");
            return await Guard(["hooks-listing", "hooks-events", "hooks-subagent"], async () =>
            {
                var (rows, _) = Events.Hooks(await c.S.Request("get_hooks_listing"));
                var mine = rows.Where(r => r.Source == "projectSettings").ToList();
                var found = $"{rows.Count} hooks, projectSettings: {string.Join(", ", mine.Select(r => $"{r.Event}[{r.Matcher}] {r.Type} \"{r.Command}\""))}";
                var listing = mine.Any(r => r is { Event: "PreToolUse", Matcher: "Write", Type: "command" } && r.Command.Contains("ccui-probe-block"))
                              && mine.Any(r => r is { Event: "UserPromptSubmit", Timeout: 5 })
                    ? ("PASS", found) : ("FAIL", found);

                var turn = await c.Turn($"Use the Write tool to create {Blocked} containing hi. Do nothing else.");
                var seen = turn.SelectMany(Events.ParseAll).OfType<HookEvt>().ToList();
                var block = seen.FirstOrDefault(h => h.Output.Contains("ccui-probe-block"));
                var prompt = seen.Any(h => h.Name == "UserPromptSubmit" && h.Output == "ccui-probe-prompt");
                var written = File.Exists(Path.Combine(dir, Blocked));
                var detail = $"{seen.Count} hook_response, block {(block is null ? "absent" : $"{block.Name} {block.Outcome} exit {block.ExitCode}")}, "
                             + $"UserPromptSubmit output {(prompt ? "seen" : "absent")}, {Blocked} {(written ? "written" : "not written")}";
                var events = block is { Name: "PreToolUse:Write", Outcome: "error", ExitCode: 2 } && prompt && !written ? ("PASS", detail) : ("FAIL", detail);

                var sub = await c.Turn("Use the Agent tool once: a general-purpose agent with the prompt 'Reply with the word one. Use no tools.'. Then reply done.");
                int At(Func<JsonElement, bool> f) => sub.FindIndex(e => f(e));
                bool Hook(JsonElement e, string name) => Events.Str(e, "subtype") == "hook_response" && Events.Str(e, "hook_name")?.StartsWith(name) == true
                                                         && Events.Str(e, "output")?.Contains(name == "SubagentStart" ? "ccui-probe-ctx" : "ccui-probe-batch") == true;
                var use = At(e => ToolUses(e).Any(n => n is "Agent" or "Task"));
                var start = At(e => Hook(e, "SubagentStart"));
                var result = At(e => Events.Str(e, "type") == "user" && e.GetRawText().Contains("\"tool_result\"") && Events.Str(e, "parent_tool_use_id") is null);
                var batch = At(e => Hook(e, "PostToolBatch"));
                var h = start >= 0 ? Events.ParseAll(sub[start]).OfType<HookEvt>().FirstOrDefault() : null;
                var order = $"Agent tool_use #{use}, {h?.Name ?? "SubagentStart"} {h?.Outcome} #{start}, tool_result #{result}, PostToolBatch #{batch}";
                var hidden = h is not null && h.Outcome == "success" && Events.ContextOnly(h.Output) && !LiveSession.Shown(h);
                return [listing, events, use >= 0 && use < start && start < result && result < batch && hidden && h!.Name.StartsWith("SubagentStart:")
                    ? ("PASS", $"{order}, context-only output hidden") : ("FAIL", $"{order}, hidden={hidden}")];
            });
        }
        finally { try { Directory.Delete(settings, true); } catch { } }
    }

    // memory-files: get_context_usage lists the memory files the session loaded, @imports included, with their type
    // (MemoryPanel's list). memory-reload: an edit to a loaded CLAUDE.md is not seen by the next turn but is after
    // /compact (what MemoryPanel tells the user). Writes memory files into the shared probe repo, removed afterwards.
    static async Task<IEnumerable<(string, string, string)>> Memory(string dir)
    {
        string claudeMd = Path.Combine(dir, "CLAUDE.md"), notes = Path.Combine(dir, "notes.md"),
            dotClaude = Path.Combine(dir, ".claude", "CLAUDE.md"), local = Path.Combine(dir, "CLAUDE.local.md");
        File.WriteAllText(claudeMd, "Project codeword: APPLE.\nSee @notes.md\n");
        File.WriteAllText(notes, "Imported note.\n");
        Directory.CreateDirectory(Path.GetDirectoryName(dotClaude)!);
        File.WriteAllText(dotClaude, "Dot-claude memory.\n");
        File.WriteAllText(local, "Local memory.\n");
        try
        {
            await using var c = await Cli.Start(dir);
            return await Guard(["memory-files", "memory-reload"], async () =>
            {
                // The CLI may report the temp dir through a symlink (/var is /private/var on macOS): match on the tail.
                var files = Events.Prop(await c.S.Request("get_context_usage"), "memoryFiles") is { ValueKind: JsonValueKind.Array } a
                    ? a.EnumerateArray().Select(m => (Path: Events.Str(m, "path") ?? "", Type: Events.Str(m, "type") ?? "?",
                        Tokens: Events.Prop(m, "tokens") is { ValueKind: JsonValueKind.Number })).ToList() : [];
                string Missing(string path, string type) =>
                    files.Any(f => f.Type == type && f.Path.EndsWith(Path.DirectorySeparatorChar + Path.GetRelativePath(dir, path))) ? "" : $"missing {type} {Path.GetFileName(path)}; ";
                var miss = Missing(claudeMd, "Project") + Missing(notes, "Project") + Missing(dotClaude, "Project") + Missing(local, "Local");
                // MemoryFiles.List drops a relative path and reads tokens as a number: either drift would empty the panel silently.
                miss += string.Concat(files.Where(f => !Path.IsPathFullyQualified(f.Path) || !f.Tokens).Select(f => $"not absolute or no numeric tokens: {f.Path}; "));
                // The user file is the machine's own: only checked when there is one.
                var user = Path.Combine(Path.GetDirectoryName(TranscriptStore.Root)!, "CLAUDE.md");
                if (File.Exists(user) && !files.Any(f => f.Type == "User")) miss += "missing User CLAUDE.md; ";
                var listed = "memoryFiles " + string.Join(", ", files.Select(f => $"{f.Type} {Path.GetFileName(Path.GetDirectoryName(f.Path))}/{Path.GetFileName(f.Path)}"));
                var listing = miss == "" ? ("PASS", listed) : ("FAIL", miss + listed);

                const string Q = "Quote verbatim the line that starts with 'Project codeword' from the CLAUDE.md project instructions in your context. Output only that line. Do not use any tool.";
                async Task<string> Ask() => Events.Str((await c.Turn(Q))[^1], "result") ?? "";
                var before = await Ask();
                File.WriteAllText(claudeMd, "Project codeword: BANANA.\nSee @notes.md\n");
                var edited = await Ask();
                await c.Turn("/compact");
                var compacted = await Ask();
                var detail = $"before \"{before}\", after the edit \"{edited}\", after /compact \"{compacted}\"";
                return [listing, !before.Contains("APPLE") ? ("FAIL", "inconclusive: " + detail)
                    : edited.Contains("APPLE") && compacted.Contains("BANANA") ? ("PASS", detail)
                    : ("FAIL", detail)];
            });
        }
        finally
        {
            foreach (var f in new[] { claudeMd, notes, dotClaude, local }) File.Delete(f);
        }
    }

    // add-dir: `--add-dir <folder>` lets Read open a file there with 0 can_use_tool, and list_permission_rules lists it.
    // add-dir-live: mid-session, apply_flag_settings {permissions:{additionalDirectories}} does the same, and a later
    // apply_flag_settings for another key (SetEffort) keeps it; the payload is the whole list, as the UI sends it. ("/add-dir" as user text answers "isn't available in this
    // environment" on 2.1.296; the add_directory control request is for cloud containers: mount_path under /uploads/.)
    // add-dir-edits: after set_permission_mode acceptEdits, Write creates a file in an --add-dir folder with 0 can_use_tool:
    // an extra folder is never isolated, so a risky mode with one is confirmed (LiveSession.Isolated).
    // add-dir-resume: --resume alone forgets the folders (the CLI asks again), --resume with one --add-dir each keeps them.
    static async Task<IEnumerable<(string, string, string)>> AddDir(string dir)
    {
        string a = dir + "-extra-a", b = dir + "-extra-b", id = Guid.NewGuid().ToString();
        foreach (var (d, word) in new[] { (a, "PELICAN"), (b, "HERON") })
        {
            Directory.CreateDirectory(d);
            File.WriteAllText(Path.Combine(d, "note.txt"), $"The secret word is {word}.\n");
        }
        try
        {
            IEnumerable<(string, string, string)> first;
            await using (var c = await Cli.Start(dir, "--session-id", id, "--add-dir", a))
                first = await Guard(["add-dir", "add-dir-live", "add-dir-edits"], async () =>
                {
                    var (ran, asked, word) = await ReadNote(c, a, "PELICAN");
                    var start = Verdict(ran, asked, word, await Listed(c, a));

                    await c.S.Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["permissions"] = new JsonObject { ["additionalDirectories"] = new JsonArray(a, b) } } });
                    await c.S.Request("apply_flag_settings", new() { ["settings"] = new JsonObject { ["effortLevel"] = "low" } });
                    (ran, asked, word) = await ReadNote(c, b, "HERON");
                    var live = Verdict(ran, asked, word, await Listed(c, b) && await Listed(c, a));

                    await c.S.Request("set_permission_mode", new() { ["mode"] = "acceptEdits" });
                    var target = Path.Combine(a, "edited.txt");
                    var prompts = 0;
                    var turn = await c.Turn($"Use the Write tool to create {target} containing the word OSPREY. Reply done.", e =>
                    {
                        if (Events.Str(e.GetProperty("request"), "tool_name") is "Write" or "Edit") prompts++;
                        return Task.FromResult(false);
                    });
                    var writes = turn.Sum(e => ToolUses(e).Count(n => n == "Write"));
                    var written = File.Exists(target) && File.ReadAllText(target).Contains("OSPREY");
                    var edits = $"acceptEdits: {writes} Write, {prompts} prompt(s), file {(written ? "written" : "absent")}";
                    return [start, live, writes == 0 ? ("FAIL", "inconclusive: no Write tool_use") : prompts == 0 && written ? ("PASS", edits) : ("FAIL", edits)];
                });

            var resume = await Guard(["add-dir-resume"], async () =>
            {
                int bare;
                await using (var c = await Cli.Start(dir, "--resume", id))
                    (_, bare, _) = await ReadNote(c, a, "PELICAN");
                await using var r = await Cli.Start(dir, "--resume", id, "--add-dir", a, "--add-dir", b);
                var (ranA, askedA, wordA) = await ReadNote(r, a, "PELICAN");
                var (ranB, askedB, wordB) = await ReadNote(r, b, "HERON");
                var detail = $"--resume alone: {bare} prompt(s); with --add-dir a --add-dir b: {ranA + ranB} Read, {askedA + askedB} prompt(s), words {(wordA && wordB ? "found" : "missing")}";
                return [bare > 0 && ranA > 0 && ranB > 0 && askedA + askedB == 0 && wordA && wordB ? ("PASS", detail) : ("FAIL", detail)];
            }, Seconds * 3);   // two more cold starts
            return [.. first, .. resume];
        }
        finally { foreach (var d in new[] { a, b }) try { Directory.Delete(d, true); } catch { } }

        static (string, string) Verdict(int ran, int asked, bool word, bool listed) =>
            ran == 0 ? ("FAIL", "inconclusive: no Read tool_use")
            : asked == 0 && word && listed ? ("PASS", "Read ran with 0 prompts, word found, listed in workspaceDirectories")
            : ("FAIL", $"{asked} prompt(s), word {(word ? "found" : "missing")}, listed={listed}");
    }

    // One turn reading <folder>/note.txt. A can_use_tool is counted, then allowed (Turn's default) so the turn ends.
    static async Task<(int Ran, int Asked, bool Word)> ReadNote(Cli c, string folder, string word)
    {
        var asked = 0;
        var turn = await c.Turn($"Use the Read tool on {Path.Combine(folder, "note.txt")} and reply with the secret word only.", e =>
        {
            if (Events.Str(e.GetProperty("request"), "tool_name") == "Read") asked++;
            return Task.FromResult(false);
        });
        return (turn.Sum(e => ToolUses(e).Count(n => n == "Read")), asked, Events.Str(turn[^1], "result")?.Contains(word) == true);
    }

    // list_permission_rules.state.workspaceDirectories[].path names the folder (realpath: /tmp may read /private/tmp).
    static async Task<bool> Listed(Cli c, string folder) =>
        Events.Prop(await c.S.Request("list_permission_rules"), "state") is { } st
        && Events.Prop(st, "workspaceDirectories") is { ValueKind: JsonValueKind.Array } a
        && a.EnumerateArray().Any(d => Events.Str(d, "path")?.EndsWith(Path.GetFileName(folder)) == true);

    // transcript-search: the CLI writes the turn to ~/.claude/projects/<slug>/<session_id>.jsonl in the shape
    // TranscriptStore.Search reads: the user prompt as user text, the reply as an assistant text block.
    // The reply's token is not in the prompt (the prompt splits it), so a hit on it can only come from assistant text.
    // search-bounds: a query matching nothing scans the transcripts within the byte budget and returns.
    static Task<IEnumerable<(string, string, string)>> TranscriptSearch(string dir) => Guard(["transcript-search", "search-bounds"], async () =>
    {
        var token = "ccuiprobe" + Guid.NewGuid().ToString("N")[..10];
        string? id;
        await using (var c = await Cli.Start(dir))   // exited before searching: the file is complete
        {
            var turn = await c.Turn($"Join the two words {token[..9]} and {token[9..]} without any space or other character. Reply with the joined word only.");
            id = turn.Select(e => Events.Str(e, "session_id")).LastOrDefault(s => s is not null);
        }
        if (id is null) return [("FAIL", "no session_id in the turn"), ("SKIP", "no session")];

        string Found(string q) =>
            TranscriptStore.Search(q, 5, CancellationToken.None).Hits.FirstOrDefault(h => h.Session.Id == id) is { } h ? $"\"{h.Snippet}\"" : "none";
        var user = Found($"{token[..9]} and {token[9..]}");
        var assistant = Found(token);
        var detail = $"{Path.GetFileName(TranscriptStore.Find(id))}: user hit {user}; assistant hit {assistant}";

        var watch = Stopwatch.StartNew();
        var (none, complete) = TranscriptStore.Search("ccui-no-such-text-" + Guid.NewGuid().ToString("N"), 20, CancellationToken.None);
        return [(user != "none" && assistant != "none" ? "PASS" : "FAIL", detail),
            none.Count == 0
                ? ("PASS", $"0 hits in {watch.ElapsedMilliseconds} ms, {(complete ? "every transcript read" : "stopped at the byte budget")}")
                : ("FAIL", $"{none.Count} hits for a random token")];
    });

    // ---------- plumbing ----------

    // Text blocks of an assistant message.
    static IEnumerable<string> Texts(JsonElement e) =>
        Events.Str(e, "type") == "assistant" && Events.Prop(e, "message") is { } m && Events.Prop(m, "content") is { ValueKind: JsonValueKind.Array } c
            ? c.EnumerateArray().Where(b => Events.Str(b, "type") == "text").Select(b => Events.Str(b, "text") ?? "")
            : [];

    // Runs a probe body that yields one (verdict, detail) per id; a throw or timeout fails every id it had not answered.
    static async Task<IEnumerable<(string, string, string)>> Guard(string[] ids, Func<Task<(string Verdict, string Detail)[]>> body, int seconds = Seconds)
    {
        (string, string)[] r;
        try { r = await body().WaitAsync(TimeSpan.FromSeconds(seconds)); }
        catch (TimeoutException) { r = [.. ids.Select(_ => ("FAIL", "timeout"))]; }
        catch (Exception ex) when (ex is not StartException) { r = [.. ids.Select(_ => ("FAIL", (ex.InnerException ?? ex).Message))]; }   // claude exiting closes the channel
        return ids.Zip(r, (id, v) => (v.Item1, id, v.Item2));
    }

    // Names of every tool_use block in an assistant message (parallel calls share one message).
    static IEnumerable<string?> ToolUses(JsonElement e) => ToolBlocks(e).Select(b => Events.Str(b, "name"));

    static IEnumerable<JsonElement> ToolBlocks(JsonElement e) =>
        Events.Str(e, "type") == "assistant" && Events.Prop(e, "message") is { } m && Events.Prop(m, "content") is { ValueKind: JsonValueKind.Array } c
            ? c.EnumerateArray().Where(b => Events.Str(b, "type") == "tool_use")
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

    // Minimal OAuth 2.1 authorization server + streamable-HTTP MCP server on a loopback port: 401 with resource metadata,
    // dynamic client registration, an authorize endpoint that approves at once, a token endpoint, then initialize and
    // tools/list for the bearer. Just enough for the CLI's own OAuth client to run its whole flow against it.
    sealed class FakeOAuthMcp : IDisposable
    {
        const string Token = "ccui-probe-token";
        readonly HttpListener http = new();
        public string Url { get; }
        public bool TokenIssued { get; private set; }
        public bool BearerSeen { get; private set; }

        public FakeOAuthMcp()
        {
            var l = new TcpListener(IPAddress.Loopback, 0);
            l.Start();
            var port = ((IPEndPoint)l.LocalEndpoint).Port;
            l.Stop();
            Url = $"http://127.0.0.1:{port}/";
            http.Prefixes.Add(Url);
            http.Start();
            _ = Task.Run(Loop);
        }

        async Task Loop()
        {
            while (http.IsListening)
            {
                HttpListenerContext ctx;
                try { ctx = await http.GetContextAsync(); } catch { return; }
                try { Handle(ctx); } catch { ctx.Response.StatusCode = 500; }
                finally { ctx.Response.Close(); }
            }
        }

        void Handle(HttpListenerContext ctx)
        {
            var (req, res) = (ctx.Request, ctx.Response);
            var path = req.Url!.AbsolutePath;
            var body = new StreamReader(req.InputStream).ReadToEnd();
            void Json(object o, int code = 200) { res.StatusCode = code; res.ContentType = "application/json"; res.OutputStream.Write(JsonSerializer.SerializeToUtf8Bytes(o)); }

            if (path.StartsWith("/.well-known/oauth-protected-resource"))
                Json(new { resource = Url + "mcp", authorization_servers = new[] { Url.TrimEnd('/') } });
            else if (path.StartsWith("/.well-known/oauth-authorization-server") || path.StartsWith("/.well-known/openid-configuration"))
                Json(new
                {
                    issuer = Url.TrimEnd('/'), authorization_endpoint = Url + "authorize", token_endpoint = Url + "token", registration_endpoint = Url + "register",
                    response_types_supported = new[] { "code" }, grant_types_supported = new[] { "authorization_code", "refresh_token" },
                    code_challenge_methods_supported = new[] { "S256" }, token_endpoint_auth_methods_supported = new[] { "none" },
                });
            else if (path == "/register")
            {
                var n = JsonNode.Parse(body)!.AsObject();
                n["client_id"] = "ccui-probe-client";
                n["client_id_issued_at"] = DateTimeOffset.UtcNow.ToUnixTimeSeconds();
                n["token_endpoint_auth_method"] = "none";
                Json(n, 201);
            }
            else if (path == "/authorize")
            {
                var q = System.Web.HttpUtility.ParseQueryString(req.Url.Query);
                res.StatusCode = 302;
                res.RedirectLocation = $"{q["redirect_uri"]}?code=ccui-probe-code&state={Uri.EscapeDataString(q["state"] ?? "")}";
            }
            else if (path == "/token")
            {
                TokenIssued = true;
                Json(new { access_token = Token, token_type = "Bearer", expires_in = 3600, refresh_token = "ccui-probe-refresh" });
            }
            else if (path == "/mcp" && req.Headers["Authorization"] != "Bearer " + Token)
            {
                res.StatusCode = 401;
                res.AddHeader("WWW-Authenticate", $"Bearer resource_metadata=\"{Url}.well-known/oauth-protected-resource\"");
            }
            else if (path == "/mcp" && req.HttpMethod == "POST")
            {
                BearerSeen = true;
                var m = JsonNode.Parse(body)!;
                if (m["id"] is not { } id) { res.StatusCode = 202; return; }   // a notification
                object? result = (string?)m["method"] switch
                {
                    "initialize" => new { protocolVersion = (string?)m["params"]?["protocolVersion"] ?? "2025-06-18", capabilities = new { tools = new { } }, serverInfo = new { name = "ccui-probe", version = "1.0.0" } },
                    "tools/list" => new { tools = new[] { new { name = "ping", description = "probe", inputSchema = new { type = "object" } } } },
                    _ => null,
                };
                Json(result is null
                    ? new { jsonrpc = "2.0", id = id.DeepClone(), error = (object)new { code = -32601, message = "method not found" } }
                    : new { jsonrpc = "2.0", id = id.DeepClone(), result });
            }
            else res.StatusCode = path == "/mcp" ? 405 : 404;   // GET /mcp: no server-initiated stream
        }

        public void Dispose() { try { http.Close(); } catch { } }
    }

    sealed class StartException(string message) : Exception(message);

    // One claude process (haiku, manual permissions unless extra names a --permission-mode) whose stdout events land in a channel.
    sealed class Cli : IAsyncDisposable
    {
        readonly Channel<JsonElement> events = Channel.CreateUnbounded<JsonElement>();
        public ClaudeSession S { get; private set; } = null!;

        public static async Task<Cli> Start(string cwd, params string[] extra)
        {
            var c = new Cli();
            try
            {
                c.S = new ClaudeSession(cwd, ["--model", "haiku", .. extra.Contains("--permission-mode") ? [] : (string[])["--permission-mode", "manual"], .. extra],
                    e => c.events.Writer.WriteAsync(e).AsTask(), (_, text) => c.events.Writer.TryComplete(new InvalidOperationException(text)));
            }
            catch (Exception ex) { throw new StartException(ex.Message); }
            try { await c.S.Request("initialize", null, Seconds); }
            catch (Exception ex) { await c.DisposeAsync(); throw new StartException($"initialize: {ex.Message}"); }
            return c;
        }

        // Sends a user turn and collects its events until `result` (included). A can_use_tool goes to onPermission when
        // given (true = handled), else it is allowed as asked, so no probe can hang on an unexpected prompt.
        public async Task<List<JsonElement>> Turn(string text, Func<JsonElement, Task<bool>>? onPermission = null, IReadOnlyList<UserImage>? images = null, string? uuid = null)
        {
            await S.SendUser(text, images, uuid);
            return await Collect(1, onPermission);
        }

        // Collects events until the `results`-th result (included); onEvent sees each one first.
        public async Task<List<JsonElement>> Collect(int results, Func<JsonElement, Task<bool>>? onPermission = null, Func<JsonElement, Task>? onEvent = null)
        {
            var seen = new List<JsonElement>();
            while (true)
            {
                var e = await events.Reader.ReadAsync();
                seen.Add(e);
                if (onEvent is not null) await onEvent(e);
                if (Events.Str(e, "type") == "result" && --results == 0) return seen;
                if (Events.Str(e, "type") == "control_request" && Events.Prop(e, "request") is { } r && Events.Str(r, "subtype") == "can_use_tool"
                    && (onPermission is null || !await onPermission(e)))
                    await S.Respond(Events.Str(e, "request_id")!, true, r.GetProperty("input"));
            }
        }

        // Next event (outside a turn) matching the predicate; the probe's Guard bounds the wait.
        public async Task<JsonElement> Until(Func<JsonElement, bool> match)
        {
            while (true)
                if (await events.Reader.ReadAsync() is var e && match(e)) return e;
        }

        bool disposed;   // exit-ends-tasks disposes inside the probe, then again on the way out

        public async ValueTask DisposeAsync()
        {
            if (S is not null && !disposed) { disposed = true; await S.DisposeAsync(); }
        }
    }
}
