using System.Text.Json;
using System.Text.RegularExpressions;

namespace ClaudeCodeUI;

// MCP helpers shared by the thread and the Extensions page.
public static class Mcp
{
    // One tool of an mcp_status server: annotations are readOnly / destructive / openWorld (only true ones are sent).
    public record ToolInfo(string Name, string? Description, bool ReadOnly, bool Destructive, bool OpenWorld);

    public static ToolInfo[] Tools(JsonElement server) =>
        Events.Prop(server, "tools") is { ValueKind: JsonValueKind.Array } a
            ? [.. a.EnumerateArray().Select(t =>
              {
                  var an = Events.Prop(t, "annotations") ?? default;
                  return new ToolInfo(Events.Str(t, "name") ?? "", Events.Str(t, "description"), True(an, "readOnly"), True(an, "destructive"), True(an, "openWorld"));
              })]
            : [];

    static bool True(JsonElement e, string n) => Events.Prop(e, n) is { ValueKind: JsonValueKind.True };

    // The server a tool name belongs to: the CLI names tools mcp__<server>__<tool>, the server name with every character
    // outside [A-Za-z0-9_-] replaced by "_" (a "claude.ai " connector's runs of "_" collapsed and trimmed; CLI In()).
    public static string? ServerOf(string tool, IEnumerable<string> servers) =>
        tool.StartsWith("mcp__", StringComparison.Ordinal) ? servers.FirstOrDefault(s => tool.StartsWith($"mcp__{Prefix(s)}__", StringComparison.Ordinal)) : null;

    internal static string Prefix(string server)
    {
        var r = Regex.Replace(server, "[^a-zA-Z0-9_-]", "_");
        return server.StartsWith("claude.ai ") ? Regex.Replace(r, "_+", "_").Trim('_') : r;
    }

    // An MCP tool call refused for its credential: the CLI's texts for a 401, a 403 insufficient_scope (also after a step-up
    // retry), and a server or headersHelper that rejected the credential (claude 2.1.296 strings).
    public static bool AuthError(string? text) =>
        text is not null && Regex.IsMatch(text, @"\b401\b|insufficient_scope|Unauthorized|rejected the (session )?credential|re-?authenticat", RegexOptions.IgnoreCase);

    // Server names of a session: mcp_status when read, else the init brief.
    public static IEnumerable<string> Names(LiveSession s) =>
        s.McpStatus is { } st && Events.Prop(st, "mcpServers") is { ValueKind: JsonValueKind.Array } a
            ? a.EnumerateArray().Select(m => Events.Str(m, "name") ?? "")
            : (s.Init?.Mcp ?? []).Select(m => m.Name);

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Mcp: " + what);
        // claude 2.1.296 mcp_status for the probe's fake server.
        var srv = JsonDocument.Parse("""{"name":"probe","status":"connected","tools":[{"name":"ask","annotations":{"readOnly":true}},{"name":"link","description":"Visit","annotations":{"destructive":true,"openWorld":true}},{"name":"bare"}]}""").RootElement;
        Ok(Tools(srv) is [{ Name: "ask", ReadOnly: true, Destructive: false, Description: null }, { Name: "link", Description: "Visit", Destructive: true, OpenWorld: true }, { Name: "bare", ReadOnly: false }], "tools");
        string[] names = ["plugin:github:github", "claude.ai Gmail", "probe"];
        Ok(ServerOf("mcp__plugin_github_github__create_issue", names) == "plugin:github:github" && ServerOf("mcp__claude_ai_Gmail__send", names) == "claude.ai Gmail"
           && ServerOf("mcp__probe__ask", names) == "probe" && ServerOf("mcp__other__x", names) is null && ServerOf("Bash", names) is null, "server of a tool");
        Ok(AuthError("Tool call returned 403 insufficient_scope (scope: repo)") && AuthError("Tool call returned 401 Unauthorized - token may have expired")
           && AuthError("MCP server \"x\" rejected the session credential (HTTP 403)") && !AuthError("ENOENT: no such file") && !AuthError(null), "auth error texts");
    }
}
