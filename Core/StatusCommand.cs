using System.Diagnostics;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

// The settings' statusLine ({"type":"command","command":…}): the CLI runs it only in its terminal UI, never over
// stream-json, so the app runs it itself, with the JSON the CLI pipes to it on stdin (StatusLineCommandInput, 2.1.296) for
// the fields the app knows, and shows what it prints. Run after the boot and after each turn.
// ponytail: event-driven only; refreshInterval and subagentStatusLine are not run, add a timer / agent rows when needed.
public static class StatusCommand
{
    // effective.statusLine of get_settings when it is a command.
    public static string? Of(JsonElement settings) =>
        Events.Prop(settings, "effective") is { } eff && Events.Prop(eff, "statusLine") is { } sl && Events.Str(sl, "type") == "command"
        && Events.Str(sl, "command") is { Length: > 0 } c ? c : null;

    public static JsonObject Payload(LiveSession s)
    {
        var model = Components.Session.ModelMenu.Current(s);
        var o = new JsonObject
        {
            ["session_id"] = s.Id,
            ["transcript_path"] = TranscriptStore.Find(s.Id) ?? "",
            ["cwd"] = s.Cwd,
            ["session_name"] = s.Name,
            ["permission_mode"] = s.Mode,
            ["model"] = new JsonObject { ["id"] = s.Model, ["display_name"] = model is { } m ? Events.Str(m, "displayName") ?? s.Model : s.Model },
            ["workspace"] = new JsonObject { ["current_dir"] = s.Cwd, ["project_dir"] = s.Cwd, ["added_dirs"] = new JsonArray([.. s.AddDirs.Select(d => (JsonNode?)d)]) },
            ["version"] = s.InitializeInfo is { } info ? Events.Str(info, "claude_code_version") : null,
            ["output_style"] = new JsonObject { ["name"] = s.OutputStyle ?? "default" },
            ["cost"] = new JsonObject { ["total_cost_usd"] = s.CostUsd, ["total_duration_ms"] = (long)(DateTimeOffset.Now - s.StartedAt).TotalMilliseconds },
            ["fast_mode"] = s.FastModeState == "on",
            ["thinking"] = new JsonObject { ["enabled"] = s.Thinking },
        };
        if (s.Effort is { } e) o["effort"] = new JsonObject { ["level"] = e };
        if (s.Limits is { } l)
            o["rate_limits"] = new JsonObject
            {
                ["five_hour"] = new JsonObject { ["used_percentage"] = Math.Round(l.FiveHour * 100, 1), ["resets_at"] = l.FiveHourReset.ToUnixTimeSeconds() },
                ["seven_day"] = new JsonObject { ["used_percentage"] = Math.Round(l.SevenDay * 100, 1), ["resets_at"] = l.SevenDayReset.ToUnixTimeSeconds() },
            };
        return o;
    }

    // What the command printed, as plain lines: ANSI colors dropped, blank lines out, at most 3 lines.
    internal static string? Text(string stdout)
    {
        var lines = PermissionRules.Plain(stdout).Split('\n').Select(l => l.TrimEnd()).Where(l => l.Trim().Length > 0).Take(3).ToList();
        return lines.Count == 0 ? null : string.Join('\n', lines);
    }

    // The command through the platform shell, in the session's folder, 5 s at most (the CLI's own budget is of that order).
    public static async Task<string?> Run(string command, string cwd, JsonObject payload)
    {
        var psi = OperatingSystem.IsWindows() ? new ProcessStartInfo("cmd.exe") : new ProcessStartInfo("/bin/sh");
        foreach (var a in OperatingSystem.IsWindows() ? ["/d", "/s", "/c", command] : new[] { "-c", command }) psi.ArgumentList.Add(a);
        psi.WorkingDirectory = cwd;
        psi.RedirectStandardInput = psi.RedirectStandardOutput = psi.RedirectStandardError = true;
        psi.StandardOutputEncoding = Encoding.UTF8;
        psi.StandardInputEncoding = new UTF8Encoding(false);
        psi.CreateNoWindow = true;
        using var p = Process.Start(psi)!;
        var output = p.StandardOutput.ReadToEndAsync();
        _ = p.StandardError.ReadToEndAsync();   // drained: a chatty script must not block on a full pipe
        try { await p.StandardInput.WriteAsync(payload.ToJsonString()); p.StandardInput.Close(); }
        catch (IOException) { }   // a command that does not read stdin may exit first
        using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        try { await p.WaitForExitAsync(cts.Token); }
        catch (OperationCanceledException) { p.Kill(true); return null; }
        return Text(await output);
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "StatusCommand: " + what);
        Ok(Of(JsonDocument.Parse("""{"effective":{"statusLine":{"type":"command","command":"~/.claude/sl.sh","padding":0}}}""").RootElement) == "~/.claude/sl.sh"
           && Of(JsonDocument.Parse("""{"effective":{}}""").RootElement) is null && Of(JsonDocument.Parse("""{"effective":{"statusLine":{"type":"x","command":"y"}}}""").RootElement) is null, "statusLine from get_settings");
        Ok(Text("\u001b[1;32mmain\u001b[0m · opus\n\n  ctx 12%\nthree\nfour\n") == "main · opus\n  ctx 12%\nthree" && Text(" \n") is null, "printed lines");
        var s = new LiveSession("sl", "name", "/w", "plan", model: "opus", effort: "high");
        Ok(Payload(s) is var o && (string?)o["session_id"] == "sl" && (string?)o["workspace"]!["current_dir"] == "/w" && (string?)o["effort"]!["level"] == "high"
           && (string?)o["output_style"]!["name"] == "default" && o["rate_limits"] is null && (string?)o["model"]!["display_name"] == "opus", "payload");
        if (!OperatingSystem.IsWindows())
            Ok(Run("cat | head -c 200 | grep -o '\"session_id\":\"sl\"'", Path.GetTempPath(), Payload(s)).GetAwaiter().GetResult() == "\"session_id\":\"sl\"", "the command reads the payload on stdin");
    }
}
