using System.Diagnostics;
using System.IO.Compression;
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
            [("ultracode", Ultracode), ("permission-session", PermissionSession), ("mcp", Mcp), ("compact", Compact), ("plan", Plan), ("ask-user-question", AskUserQuestion), ("todo-tools", TodoTools), ("image", Image), ("file-mention", FileMention)];
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
        public async Task<List<JsonElement>> Turn(string text, Func<JsonElement, Task<bool>>? onPermission = null, IReadOnlyList<UserImage>? images = null)
        {
            await S.SendUser(text, images);
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
