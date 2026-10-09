using System.Text.Json;

namespace ClaudeCodeUI;

// Colour and one-line target of a tool. On this Windows CLI the shell is PowerShell; the subagent tool is "Agent" (or "Task").
public static class ToolKinds
{
    public static string Color(string name) => name switch
    {
        "Read" or "NotebookRead" => "var(--read)",
        "Grep" or "Glob" => "var(--grep)",
        "Edit" or "MultiEdit" or "NotebookEdit" => "var(--edit)",
        "Bash" or "PowerShell" or "BashOutput" or "KillShell" => "var(--bash)",
        "Write" => "var(--write)",
        "WebFetch" or "WebSearch" => "var(--web)",
        _ => "var(--fg-3)",
    };

    public static bool IsShell(string name) => name is "Bash" or "PowerShell";
    public static bool IsAgent(string name) => name is "Agent" or "Task";
    public static bool IsEdit(string name) => name is "Edit" or "MultiEdit" or "Write";

    public static string Target(string name, JsonElement input, string cwd)
    {
        if (Events.Str(input, "file_path") is { } f) return Relative(f, cwd);
        if (Events.Str(input, "command") is { } c) return c;
        if (Events.Str(input, "pattern") is { } p)
            return (Events.Str(input, "glob") ?? Events.Str(input, "path")) is { } where && Relative(where, cwd) is var w && w != "." ? $"{p} · {w}" : p;
        if (AskUser.Parse(input) is { Length: > 0 } qs) return string.Join(" · ", qs.Select(q => q.Header.Length > 0 ? q.Header : q.Text));
        return TodoList.Target(name, input) ?? Events.Str(input, "description") ?? Events.Str(input, "url") ?? Events.Str(input, "query") ?? FirstString(input) ?? "";
    }

    // "mcp__server__tool" -> "tool"; the full name stays in the title. Other names are returned as is.
    public static string Label(string name) =>
        name.StartsWith("mcp__", StringComparison.Ordinal) && name.LastIndexOf("__", StringComparison.Ordinal) is var i && i > 3 ? name[(i + 2)..] : name;

    // MCP and other unknown tools: the first string argument is the best one-line target there is.
    static string? FirstString(JsonElement input) =>
        input.ValueKind == JsonValueKind.Object && input.EnumerateObject().Select(p => p.Value).FirstOrDefault(v => v.ValueKind == JsonValueKind.String) is { ValueKind: JsonValueKind.String } v
            ? v.GetString() : null;

    static string Relative(string path, string cwd) =>
        (cwd.Length > 0 && path.StartsWith(cwd, StringComparison.OrdinalIgnoreCase) ? Path.GetRelativePath(cwd, path) : path).Replace('\\', '/');
}
