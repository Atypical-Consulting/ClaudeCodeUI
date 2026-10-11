using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

// The CLI's own writers for what stream-json cannot write (update_settings takes outputStyle and effortLevel only):
// `claude edit-hook --json` and `claude edit-memory-settings --json` (2.1.296, hidden, run by the VS Code extension).
// One JSON edit on stdin; exit 0 with {"ok":true…}, else the CLI's error. They refuse to run from inside a Claude Code
// session: Plugins.Run drops the variables that say so.
public static class CliEdit
{
    // null = written, else why not.
    public static async Task<string?> Run(string cwd, string command, JsonNode edit)
    {
        var (code, output, err) = await Plugins.Run(cwd, ["--json"], edit.ToJsonString(), command);
        return code == 0 ? null : Error(output, err, command);
    }

    // edit-hook prints {"ok":false,"error":…} and the same text on stderr; edit-memory-settings only the stderr line.
    internal static string Error(string output, string err, string command)
    {
        foreach (var line in output.Split('\n').Reverse().Where(l => l.TrimStart().StartsWith('{')))
            try { if (Events.Str(JsonDocument.Parse(line).RootElement, "error") is { } e) return e; }
            catch (JsonException) { }
        return (err + "\n" + output).Split('\n').Select(l => l.Trim()).LastOrDefault(l => l.Length > 0) ?? "claude " + command;
    }

    // A hook edit as edit-hook reads it: add (no target), replace (hook and target), remove (target only). The target is
    // the row's stored entry (HookRow.Edit), so a hook changed on disk since the listing is refused, not overwritten.
    public static JsonObject Hook(string op, string source, string ev, string matcher, JsonNode? hook, HookRow? target)
    {
        var o = new JsonObject { ["op"] = op, ["source"] = source, ["event"] = ev, ["matcher"] = matcher };
        if (hook is not null) o["hook"] = hook;
        if (target is { Edit: { } t })
            o["target"] = new JsonObject { ["event"] = target.Event, ["matcher"] = t.Matcher, ["hook"] = JsonNode.Parse(t.Config.GetRawText()) };
        return o;
    }

    // The command hook the form describes, over the stored one (statusMessage, async, if… kept). onFailure "block" makes a
    // failing hook stop what it guards, as the terminal's /hooks writes it.
    public static JsonObject CommandHook(JsonElement? stored, string command, int? timeout, bool block)
    {
        var h = stored is { } s ? JsonNode.Parse(s.GetRawText())!.AsObject() : new JsonObject();
        h["type"] = "command";
        h["command"] = command;
        if (timeout is > 0) h["timeout"] = timeout; else h.Remove("timeout");
        if (block) h["onFailure"] = "block"; else h.Remove("onFailure");
        return h;
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "CliEdit: " + what);
        // claude 2.1.296: edit-hook's refusal, edit-memory-settings' success line, a bare stderr failure.
        Ok(Error("""{"ok":false,"error":"edit_hook: event is not a known hook event","code":"bad_request"}""" + "\n", "edit_hook: event is not a known hook event\n", "edit-hook")
           == "edit_hook: event is not a known hook event" && Error("", "boom\n\n", "x") == "boom" && Error("", "", "x") == "claude x", "error text");
        var row = new HookRow("PostToolUse", "", "command", "echo bye", null, "projectSettings", null, null, null,
            Edit: new("", JsonDocument.Parse("""{"type":"command","command":"echo bye","onFailure":"block","statusMessage":"Bye"}""").RootElement.Clone()));
        var add = Hook("add", "projectSettings", "PreToolUse", "Bash", CommandHook(null, "echo hi", 5, false), null).ToJsonString();
        Ok(add == """{"op":"add","source":"projectSettings","event":"PreToolUse","matcher":"Bash","hook":{"type":"command","command":"echo hi","timeout":5}}""", "add, the shape the probe wrote");
        var rep = Hook("replace", row.Source, "PostToolUse", "Edit", CommandHook(row.Edit!.Config, "echo x", null, false), row);
        Ok(rep["target"]!["hook"]!["onFailure"]!.GetValue<string>() == "block" && rep["hook"]!["statusMessage"]!.GetValue<string>() == "Bye"
           && rep["hook"]!["onFailure"] is null && rep["hook"]!["timeout"] is null && rep["target"]!["event"]!.GetValue<string>() == "PostToolUse", "replace keeps other keys, targets the stored entry");
        Ok(Hook("remove", row.Source, row.Event, "", null, row) is var rm && rm["hook"] is null && rm["target"] is not null, "remove names only the target");
    }
}
