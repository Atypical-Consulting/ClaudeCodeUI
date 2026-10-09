using System.Text.Json;

namespace ClaudeCodeUI;

public enum DiffKind { Ctx, Add, Del, Hunk }
public record DiffLine(DiffKind Kind, int? N, string Text);

// Diff lines for Edit/Write/MultiEdit: before approval (from the tool input) or after (structuredPatch). Body: WP2.
public static class EditDiff
{
    public static IReadOnlyList<DiffLine> FromInput(string tool, JsonElement input) => [];

    public static IReadOnlyList<DiffLine> FromPatch(JsonElement structuredPatch) => [];

    // highlight.js language of a file; "" when unknown.
    public static string Lang(string path) => Path.GetExtension(path).ToLowerInvariant() switch
    {
        ".cs" or ".razor" or ".cshtml" => "csharp",
        ".js" or ".mjs" or ".cjs" => "javascript",
        ".ts" or ".tsx" => "typescript",
        ".json" => "json",
        ".css" => "css",
        ".html" or ".htm" or ".xml" or ".csproj" or ".props" or ".targets" or ".svg" => "xml",
        ".md" => "markdown",
        ".sh" => "bash",
        ".ps1" or ".psm1" => "powershell",
        ".yml" or ".yaml" => "yaml",
        ".py" => "python",
        ".sql" => "sql",
        _ => "",
    };

    internal static void Check() { }
}
