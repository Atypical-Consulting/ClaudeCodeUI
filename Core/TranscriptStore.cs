using System.Text.RegularExpressions;

namespace ClaudeCodeUI;

public record PastSession(string Id, string Cwd, string? Branch, string Title, decimal? CostUsd, DateTimeOffset LastWrite, string? WorktreePath);

// Past sessions read from ~/.claude/projects/<slug>/<id>.jsonl. Body: WP3.
public static class TranscriptStore
{
    public static IReadOnlyList<PastSession> Recent(int take = 30) => [];

    public static IReadOnlyList<Item> Load(string id) => [];

    public static string Slug(string cwd) => Regex.Replace(cwd, "[^A-Za-z0-9]", "-");
}
