using System.Text;
using System.Text.Json;

namespace ClaudeCodeUI;

// Type: the CLI's own label (User, Project, Local; verified on 2.1.296), Tokens: null for a file the CLI did not load.
public sealed record MemoryFile(string Path, string Type, long? Tokens, bool Exists);

public enum MemorySave { Saved, Conflict, NotAllowed }

// MemoryPanel's state, kept on the LiveSession: the panel unmounts whenever a permission request (or a selected tool)
// takes the inspector, and an unsaved edit must come back with it. Loaded: the file as read (null = absent), the baseline
// of the conflict check.
public sealed class MemoryEdit
{
    public string? Open, Loaded, Draft;
    public bool Editing, Conflict;
}

// The memory (CLAUDE.md) files of a session, the /memory equivalent. The list is the CLI's: get_context_usage.memoryFiles
// (walk up from cwd, @imports included, verified by --probe-cli memory-files), plus the standard locations it did not
// load so they can be created. Saving only ever writes a path of that list. The CLI reads memory at start and on
// /compact only (--probe-cli memory-reload): the panel says so.
public static class MemoryFiles
{
    const long MaxBytes = 256 * 1024;   // a memory file is loaded into every turn; past this the textarea is the wrong tool

    static readonly StringComparison PathCmp = OperatingSystem.IsLinux() ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase;

    static bool Same(string a, string b) => string.Equals(Path.GetFullPath(a), Path.GetFullPath(b), PathCmp);

    // ponytail: paths are compared as written. A cwd reached through a symlink (/tmp vs /private/tmp) can list a standard
    // file twice, once loaded and once as its candidate; resolve every component if that ever shows up for real.
    public static IReadOnlyList<MemoryFile> List(JsonElement? context, string cwd, string userDir, Func<string, bool> exists)
    {
        var list = new List<MemoryFile>();
        if (context is { } c && Events.Prop(c, "memoryFiles") is { ValueKind: JsonValueKind.Array } a)
            foreach (var m in a.EnumerateArray())
                if (Events.Str(m, "path") is { } p && Path.IsPathFullyQualified(p) && !list.Any(f => Same(f.Path, p)))
                    list.Add(new(p, Events.Str(m, "type") ?? "", Events.Prop(m, "tokens") is { ValueKind: JsonValueKind.Number } t ? (long)t.GetDouble() : null, exists(p)));
        // A cwd that is not absolute (a session still without one) would resolve against the server's own folder.
        var candidates = new List<(string, string)> { (Path.Combine(userDir, "CLAUDE.md"), "User") };
        if (Path.IsPathFullyQualified(cwd))
            candidates.AddRange([(Path.Combine(cwd, "CLAUDE.md"), "Project"), (Path.Combine(cwd, ".claude", "CLAUDE.md"), "Project"), (Path.Combine(cwd, "CLAUDE.local.md"), "Local")]);
        foreach (var (p, type) in candidates)
            if (!list.Any(f => Same(f.Path, p))) list.Add(new(p, type, null, exists(p)));
        return list;
    }

    // ~ for the home folder, relative under cwd: the full path is in the row's title.
    public static string Display(string path, string cwd, string home) =>
        Path.GetRelativePath(cwd, path) is var rel && !rel.StartsWith("..") && !Path.IsPathRooted(rel) ? rel
        : Path.GetRelativePath(home, path) is var h && !h.StartsWith("..") && !Path.IsPathRooted(h) ? "~" + Path.DirectorySeparatorChar + h
        : path;

    // The file as text (BOM stripped), null when it does not exist.
    public static string? Read(string path)
    {
        var f = new FileInfo(path);
        if (!f.Exists) return null;
        if (f.Length > MaxBytes) throw new IOException(Strings.Get("Mem.TooLarge", Fmt.Size(MaxBytes)));
        return File.ReadAllText(path);
    }

    // Writes text if path is one of known and the file still holds what was read (loaded, null = did not exist).
    // WriteAllText, not write-then-rename: CLAUDE.md is often a symlink (to AGENTS.md) and a rename would replace it.
    public static MemorySave Save(IReadOnlyList<MemoryFile> known, string path, string? loaded, string text)
    {
        if (!known.Any(f => Same(f.Path, path))) return MemorySave.NotAllowed;
        if (Read(path) != loaded) return MemorySave.Conflict;
        var bom = loaded is not null && File.ReadAllBytes(path) is [0xEF, 0xBB, 0xBF, ..];
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, Eol(loaded, text), new UTF8Encoding(bom));
        return MemorySave.Saved;
    }

    // A textarea hands back \n: keep the file's CRLF if it had them.
    internal static string Eol(string? loaded, string text) =>
        text.ReplaceLineEndings(loaded?.Contains("\r\n") == true ? "\r\n" : "\n");

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "MemoryFiles: " + what);
        var root = Path.Combine(Path.GetTempPath(), "ccui-mem-" + Guid.NewGuid().ToString("N"));
        string home = Path.Combine(root, "home"), cwd = Path.Combine(root, "repo");
        var ctx = JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            memoryFiles = new object[]
            {
                new { path = Path.Combine(home, ".claude", "CLAUDE.md"), type = "User", tokens = 206 },
                new { path = Path.Combine(cwd, "CLAUDE.md"), type = "Project", tokens = 23 },
                new { path = Path.Combine(cwd, "docs", "extra.md"), type = "Project", tokens = 12 },   // an @import
                new { path = "relative.md", type = "Project", tokens = 1 },                             // never trusted
            },
        })).RootElement;
        var list = List(ctx, cwd, Path.Combine(home, ".claude"), p => p.EndsWith("CLAUDE.md"));
        Ok(list.Select(f => Display(f.Path, cwd, home)).SequenceEqual([
            $"~{Path.DirectorySeparatorChar}{Path.Combine(".claude", "CLAUDE.md")}", "CLAUDE.md", Path.Combine("docs", "extra.md"),
            Path.Combine(".claude", "CLAUDE.md"), "CLAUDE.local.md"]), "CLI files first, then the missing standard ones, no relative path");
        Ok(list[0] is { Type: "User", Tokens: 206, Exists: true } && list[2] is { Exists: false } && list[4] is { Type: "Local", Tokens: null, Exists: false }, "row fields");
        Ok(List(null, cwd, Path.Combine(home, ".claude"), _ => false).Count == 4, "no context: the standard locations");
        Ok(List(null, "", Path.Combine(home, ".claude"), _ => false) is [{ Type: "User" }] &&List(ctx, "repo", Path.Combine(home, ".claude"), _ => true).Count == 3,
            "relative or empty cwd: no candidate resolved against the server's folder");
        Ok(Display(Path.Combine(root, "elsewhere.md"), cwd, home) == Path.Combine(root, "elsewhere.md"), "outside cwd and home: full path");
        Ok(Eol("a\r\nb", "a\nb\nc") == "a\r\nb\r\nc" && Eol("a\nb", "x\r\ny") == "x\ny" && Eol(null, "x\ny") == "x\ny", "line endings");

        try
        {
            var md = Path.Combine(cwd, "CLAUDE.md");
            Ok(Save(list, Path.Combine(cwd, "evil.md"), null, "x") == MemorySave.NotAllowed && !File.Exists(Path.Combine(cwd, "evil.md")), "unknown path refused");
            Ok(Save(list, Path.Combine(cwd, "sub", "..", "..", "repo", "x.md"), null, "x") == MemorySave.NotAllowed, "dot-dot path refused");
            Ok(Save(list, md, null, "one\n") == MemorySave.Saved && Read(md) == "one\n", "create a missing file (folder included)");
            Ok(Save(list, md, null, "two\n") == MemorySave.Conflict && Read(md) == "one\n", "created meanwhile: conflict, file untouched");
            File.WriteAllText(md, "changed on disk\n");
            Ok(Save(list, md, "one\n", "mine\n") == MemorySave.Conflict && Read(md) == "changed on disk\n", "changed since load: conflict");
            File.WriteAllText(md, "a\r\nb\r\n", new UTF8Encoding(true));
            Ok(Save(list, md, Read(md), "a\nb\nc\n") == MemorySave.Saved && File.ReadAllBytes(md) is [0xEF, 0xBB, 0xBF, (byte)'a', (byte)'\r', ..], "BOM and CRLF kept");
        }
        finally { try { Directory.Delete(root, true); } catch { } }
    }
}
