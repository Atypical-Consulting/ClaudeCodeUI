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

    public static string Target(JsonElement input, string cwd)
    {
        if (Events.Str(input, "file_path") is { } f) return Relative(f, cwd);
        if (Events.Str(input, "command") is { } c) return c;
        if (Events.Str(input, "pattern") is { } p)
            return (Events.Str(input, "glob") ?? Events.Str(input, "path")) is { } where && Relative(where, cwd) is var w && w != "." ? $"{p} · {w}" : p;
        return Events.Str(input, "description") ?? Events.Str(input, "url") ?? Events.Str(input, "query") ?? "";
    }

    static string Relative(string path, string cwd) =>
        (cwd.Length > 0 && path.StartsWith(cwd, StringComparison.OrdinalIgnoreCase) ? Path.GetRelativePath(cwd, path) : path).Replace('\\', '/');
}
