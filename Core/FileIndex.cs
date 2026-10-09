using System.Diagnostics;
using System.Text;

namespace ClaudeCodeUI;

// "@" file mentions in the Composer: the files and folders of a session cwd, and the fuzzy filter over them.
// The CLI expands `@path` / `@"path with spaces"` itself in stream-json mode (file content, or a folder listing,
// attached to the turn): verified by `--probe-cli file-mention`. The UI only has to insert the text.
public static class FileIndex
{
    public const int MaxEntries = 20_000;   // scan cap (files + folders): a huge tree is cut, never read whole
    public const int MaxResults = 10;
    static readonly TimeSpan Budget = TimeSpan.FromSeconds(5);
    static readonly HashSet<string> Skipped = new(StringComparer.OrdinalIgnoreCase) { "bin", "obj", "node_modules", ".git" };

    // Paths relative to the cwd, '/'-separated, folders ending with '/'. Truncated = the cap was hit.
    public sealed record Index(string[] Paths, bool Truncated);

    // `git ls-files` (tracked + untracked-not-ignored) in a git repo, else a bounded walk. Off the circuit's thread.
    public static Task<Index> Scan(string cwd, CancellationToken ct) => Task.Run(async () =>
    {
        using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct);
        cts.CancelAfter(Budget);
        var set = new HashSet<string>(StringComparer.Ordinal);
        var truncated = await Git(cwd, set, cts.Token) is not { } t ? Walk(cwd, set, cts.Token) : t;
        ct.ThrowIfCancellationRequested();
        return new Index([.. set.Where(p => Inside(cwd, p))], truncated || cts.IsCancellationRequested);
    }, ct);

    // null = not a git repo (or no git): the caller walks instead.
    static async Task<bool?> Git(string cwd, HashSet<string> set, CancellationToken ct)
    {
        var psi = new ProcessStartInfo("git") { RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false, CreateNoWindow = true,
            StandardOutputEncoding = Encoding.UTF8 };
        foreach (var a in new[] { "-C", cwd, "-c", "core.quotePath=off", "ls-files", "--cached", "--others", "--exclude-standard" }) psi.ArgumentList.Add(a);
        psi.Environment["GIT_OPTIONAL_LOCKS"] = "0";
        Process p;
        try { p = Process.Start(psi)!; }
        catch (Exception) { return null; }
        using (p)
        {
            _ = p.StandardError.ReadToEndAsync(ct);
            var truncated = false;
            try
            {
                while (await p.StandardOutput.ReadLineAsync(ct) is { } line)
                {
                    if (line.StartsWith('"')) continue;   // still C-quoted (newline, tab, quote in the name): not insertable
                    if (set.Count >= MaxEntries) { truncated = true; break; }
                    set.Add(line);
                    for (var i = line.LastIndexOf('/'); i > 0; i = line.LastIndexOf('/', i - 1))   // parent folders, once each
                        if (!set.Add(line[..(i + 1)])) break;
                }
                if (truncated) p.Kill(true);
                else await p.WaitForExitAsync(ct);
            }
            catch (OperationCanceledException) { try { p.Kill(true); } catch (Exception) { } return true; }
            return truncated ? true : p.ExitCode == 0 ? false : set.Count > 0 ? false : null;
        }
    }

    // Breadth first, so a cut keeps the shallow entries. Skips bin/obj/node_modules/.git and never follows a link.
    static bool Walk(string cwd, HashSet<string> set, CancellationToken ct)
    {
        var opts = new EnumerationOptions { IgnoreInaccessible = true, AttributesToSkip = 0 };
        var queue = new Queue<string>([""]);
        while (queue.TryDequeue(out var rel))
        {
            if (ct.IsCancellationRequested) return true;
            IEnumerable<FileSystemInfo> entries;
            try { entries = new DirectoryInfo(Path.Combine(cwd, rel)).EnumerateFileSystemInfos("*", opts); }
            catch (Exception) { continue; }
            foreach (var e in entries.Take(MaxEntries).OrderBy(e => e.Name, StringComparer.Ordinal))   // a huge folder is cut before the sort
            {
                if (set.Count >= MaxEntries) return true;
                if (e.Name.Any(char.IsControl)) continue;   // tab, newline: not insertable, like git's C-quoted names
                if (e is DirectoryInfo d)
                {
                    if (Skipped.Contains(d.Name)) continue;
                    set.Add(rel + d.Name + "/");
                    if (!d.Attributes.HasFlag(FileAttributes.ReparsePoint)) queue.Enqueue(rel + d.Name + "/");
                }
                else set.Add(rel + e.Name);
            }
        }
        return false;
    }

    // Lexically inside the cwd: not rooted, no ".." that climbs out. Symlinks are not resolved.
    public static bool Inside(string cwd, string rel)
    {
        if (rel.Length == 0 || rel.Contains('"') || Path.IsPathRooted(rel)) return false;
        var root = Path.TrimEndingDirectorySeparator(Path.GetFullPath(cwd)) + Path.DirectorySeparatorChar;
        var cmp = OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
        return Path.GetFullPath(Path.Combine(root, rel)).StartsWith(root, cmp);
    }

    // The query of the mention being typed at the end of the text ("fix @src/Fo" -> "src/Fo"), or null.
    // The '@' must start a word, so an e-mail address is not a mention.
    public static string? Query(string text)
    {
        var at = text.LastIndexOf('@');
        if (at < 0 || (at > 0 && !char.IsWhiteSpace(text[at - 1]))) return null;
        var q = text[(at + 1)..];
        return q.Any(c => char.IsWhiteSpace(c) || c == '"') ? null : q;
    }

    // Replaces the mention being typed by the picked path; quoted when it holds a space (the CLI's @"a b" form).
    public static string Insert(string text, string path)
    {
        var at = text.LastIndexOf('@');
        if (at < 0) return text;   // a late click after the '@' was deleted: nothing to replace
        return text[..at] + (path.Any(char.IsWhiteSpace) ? $"@\"{path}\"" : "@" + path) + " ";
    }

    // Best first: name starts with the query, name contains it, path contains it, then the query's letters in order.
    // An empty query lists the shallowest entries.
    public static List<string> Filter(IEnumerable<string> paths, string query, int max = MaxResults)
    {
        if (query.Length == 0)
            return [.. paths.OrderBy(p => p.TrimEnd('/').Count(c => c == '/')).ThenBy(p => p, StringComparer.OrdinalIgnoreCase).Take(max)];
        return [.. paths.Select(p => (p, s: Score(p, query))).Where(x => x.s >= 0)
            .OrderBy(x => x.s).ThenBy(x => x.p.Length).ThenBy(x => x.p, StringComparer.OrdinalIgnoreCase).Take(max).Select(x => x.p)];
    }

    // Last segment, keeping a folder's trailing '/': "src/Bar/" -> "Bar/".
    public static string Name(string path) => path[(path.LastIndexOf('/', Math.Max(path.Length - 2, 0)) + 1)..];

    static int Score(string path, string q)
    {
        const StringComparison ic = StringComparison.OrdinalIgnoreCase;
        var name = Name(path);
        if (name.StartsWith(q, ic)) return 0;
        if (name.Contains(q, ic)) return 1;
        if (path.Contains(q, ic)) return 2;
        var i = 0;
        foreach (var c in path)
            if (i < q.Length && char.ToLowerInvariant(c) == char.ToLowerInvariant(q[i])) i++;
        return i == q.Length ? 3 : -1;
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "FileIndex: " + what);
        Ok(Query("look at @src/Fo") == "src/Fo", "query at the end");
        Ok(Query("@") == "", "bare @ opens the list");
        Ok(Query("mail me@host.com") is null, "e-mail is not a mention");
        Ok(Query("@src/Foo.cs then") is null, "finished mention closes");
        Ok(Query("no mention") is null, "no @");
        Ok(Insert("fix @Fo", "src/Foo.cs") == "fix @src/Foo.cs ", "insert replaces the query");
        Ok(Insert("@my", "my notes/a b.txt") == "@\"my notes/a b.txt\" ", "spaces are quoted");
        Ok(Insert("no mention left", "src/Foo.cs") == "no mention left", "insert without an @ keeps the text");

        var cwd = Path.Combine(Path.GetTempPath(), "ccui-cwd");
        Ok(Inside(cwd, "src/a.cs") && Inside(cwd, "src/../a.cs") && Inside(cwd, "src/"), "relative paths inside");
        Ok(!Inside(cwd, "../etc/passwd") && !Inside(cwd, "src/../../x") && !Inside(cwd, "/etc/passwd") && !Inside(cwd, ""), "traversal and rooted rejected");
        Ok(!Inside(cwd, "../ccui-cwd-evil/x"), "sibling with the same prefix rejected");
        Ok(!Inside(cwd, "a\"b"), "quote rejected");

        string[] paths = ["README.md", "src/", "src/Foo.cs", "src/Bar/FooBar.cs", "docs/foo-guide.md", "tests/", "tests/FooTests.cs", "Fmt.cs"];
        Ok(Filter(paths, "foo") is ["src/Foo.cs", "docs/foo-guide.md", "src/Bar/FooBar.cs", "tests/FooTests.cs"], $"ranking: {string.Join(",", Filter(paths, "foo"))}");
        Ok(Filter(paths, "sfc") is ["src/Foo.cs", "src/Bar/FooBar.cs", "tests/FooTests.cs"], $"subsequence: {string.Join(",", Filter(paths, "sfc"))}");
        Ok(Filter(paths, "src/") is ["src/", "src/Foo.cs", "src/Bar/FooBar.cs"], $"folder: {string.Join(",", Filter(paths, "src/"))}");
        Ok(Filter(paths, "", 4) is ["Fmt.cs", "README.md", "src/", "tests/"], $"empty query: top level first: {string.Join(",", Filter(paths, ""))}");
        Ok(Filter(paths, "zzz").Count == 0 && Filter(paths, "", 2).Count == 2, "no match, cap");
        Ok(Name("src/Bar/") == "Bar/" && Name("src/Foo.cs") == "Foo.cs" && Name("a") == "a", "name");

        // Both scanners on a real tree: the walk skips bin/obj/node_modules, git skips what .gitignore ignores.
        var dir = Path.Combine(Path.GetTempPath(), "ccui-index-" + Guid.NewGuid().ToString("N"));
        try
        {
            var tabbed = !OperatingSystem.IsWindows();   // a tab is not a legal file-name character on Windows
            foreach (var f in new[] { "src/a b.cs", "node_modules/x.js", "bin/y.dll", "out/z.log", ".gitignore" }.Concat(tabbed ? ["src/t\tab.cs"] : []))
            {
                Directory.CreateDirectory(Path.GetDirectoryName(Path.Combine(dir, f))!);
                File.WriteAllText(Path.Combine(dir, f), f == ".gitignore" ? "out/\n" : "");
            }
            var walk = Scan(dir, default).GetAwaiter().GetResult().Paths;
            Ok(walk.Contains("src/") && walk.Contains("src/a b.cs") && walk.Contains("out/z.log") && !walk.Any(p => p.StartsWith("node_modules") || p.StartsWith("bin") || p.Contains('\t')),
                $"walk: {string.Join(",", walk)}");
            var git = Process.Start(new ProcessStartInfo("git", ["-C", dir, "init", "-q"]) { RedirectStandardError = true })!;
            git.WaitForExit();
            var listed = Scan(dir, default).GetAwaiter().GetResult().Paths;
            Ok(listed.Contains("src/") && listed.Contains("src/a b.cs") && listed.Contains("node_modules/x.js") && !listed.Any(p => p.StartsWith("out") || p.Contains('\t')),
                $"git ls-files: {string.Join(",", listed)}");
        }
        finally { try { Directory.Delete(dir, true); } catch (Exception) { } }
    }
}
