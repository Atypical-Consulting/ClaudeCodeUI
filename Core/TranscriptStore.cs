using System.Globalization;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace ClaudeCodeUI;

public record PastSession(string Id, string Cwd, string? Branch, string Title, decimal? CostUsd, DateTimeOffset LastWrite, string? WorktreePath, string? Mode = null);

// Past sessions read from ~/.claude/projects/<slug>/<id>.jsonl (top-level files only; <id>/subagents are excluded).
public static class TranscriptStore
{
    static readonly string Root = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".claude", "projects");
    const int Chunk = 64 * 1024;
    static (DateTimeOffset At, int Take, IReadOnlyList<PastSession> List) cache;

    // Newest first, last 30 days, cached 30 s.
    public static IReadOnlyList<PastSession> Recent(int take = 30)
    {
        var c = cache;
        if (c.List is not null && c.Take >= take && DateTimeOffset.Now - c.At < TimeSpan.FromSeconds(30)) return [.. c.List.Take(take)];
        var since = DateTime.UtcNow.AddDays(-30);
        List<PastSession> list = [];
        try
        {
            foreach (var f in new DirectoryInfo(Root).EnumerateDirectories().SelectMany(d => d.EnumerateFiles("*.jsonl"))
                         .Where(f => f.LastWriteTimeUtc >= since).OrderByDescending(f => f.LastWriteTimeUtc))
            {
                if (list.Count >= take) break;
                if (Read(f) is { } p) list.Add(p);
            }
        }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
        cache = (DateTimeOffset.Now, take, list);
        return list;
    }

    // Replays the user/assistant lines through Events.ParseAll and the session reducer.
    public static IReadOnlyList<Item> Load(string id) => Find(id) is { } path ? Replay(File.ReadLines(path)) : [];

    static IReadOnlyList<Item> Replay(IEnumerable<string> lines)
    {
        var s = new LiveSession("", "", "", "default");
        foreach (var line in lines)
        {
            if (!line.Contains("\"type\":\"user\"") && !line.Contains("\"type\":\"assistant\"")) continue;
            JsonElement e;
            // The file names the diff field toolUseResult; the stream (and Events) say tool_use_result.
            try { e = JsonDocument.Parse(line.Replace("\"toolUseResult\":", "\"tool_use_result\":")).RootElement; }
            catch (JsonException) { continue; }
            if (Events.Str(e, "type") is not ("user" or "assistant") || Events.Prop(e, "isSidechain") is { ValueKind: JsonValueKind.True }
                || Events.Prop(e, "isMeta") is { ValueKind: JsonValueKind.True } || Events.Prop(e, "isCompactSummary") is { ValueKind: JsonValueKind.True }
                || Events.Prop(e, "isVisibleInTranscriptOnly") is { ValueKind: JsonValueKind.True }) continue;
            DateTimeOffset? at = DateTimeOffset.TryParse(Events.Str(e, "timestamp"), out var t) ? t : null;
            foreach (var ev in Events.ParseAll(e))
            {
                s.Apply(ev);
                if (at is null) continue;
                if (ev is ToolUseEvt u && Tool(s, u.Id) is { } tu) tu.StartedAt = at.Value;
                if (ev is ToolResultEvt r && Tool(s, r.ToolUseId) is { } tr) tr.EndedAt = tr.Background ? null : at.Value;
                if (ev is TaskDoneEvt { DurationMs: 0 } td && (Tool(s, td.ToolUseId) ?? s.Items.OfType<ToolItem>().LastOrDefault(x => x.TaskId == td.TaskId)) is { } tt) tt.EndedAt = at.Value;
            }
        }
        foreach (var t in s.Items.OfType<ToolItem>().Where(t => t.State is ToolState.Running or ToolState.Waiting))
        {
            t.State = ToolState.Error;   // no result in the file: the turn was cut short
            t.EndedAt ??= t.StartedAt;
        }
        return s.Items;

        static ToolItem? Tool(LiveSession s, string id) => s.Items.LastOrDefault(i => i is ToolItem t && t.Id == id) as ToolItem;
    }

    // Last cost-state of the transcript: where --resume makes total_cost_usd start again.
    // Raw cost-state: what the CLI restores on --resume (EnsureProcess subtracts it).
    public static decimal PersistedCost(string id) => Find(id) is { } path ? CostState(new FileInfo(path)) ?? 0 : 0;

    // cost-state is only written on a clean CLI exit; the ledger holds what the UI saw (ResultEvt.TotalCostUsd).
    static readonly string CostDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ClaudeCodeUI", "costs");

    public static void RecordCost(string id, decimal usd)
    {
        try { Directory.CreateDirectory(CostDir); File.WriteAllText(Path.Combine(CostDir, id + ".txt"), usd.ToString(CultureInfo.InvariantCulture)); }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }
    }

    internal static void DeleteCost(string id) { try { File.Delete(Path.Combine(CostDir, id + ".txt")); } catch (IOException) { } }

    internal static decimal? RecordedCost(string id)
    {
        try { return decimal.TryParse(File.ReadAllText(Path.Combine(CostDir, id + ".txt")), NumberStyles.Number, CultureInfo.InvariantCulture, out var d) ? d : null; }
        catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return null; }
    }

    internal static decimal? CostState(FileInfo f) => Read(f, false)?.CostUsd;

    public static string Slug(string cwd) => Regex.Replace(cwd, "[^A-Za-z0-9]", "-");

    // "ClaudeCodeUI" for C:\repo\ClaudeCodeUI and for its -w worktrees (…\ClaudeCodeUI\.claude\worktrees\x).
    internal static string RepoName(string cwd)
    {
        // Both separators: transcripts carry the cwd of the OS that wrote them.
        cwd = cwd.Replace('\\', '/').TrimEnd('/');
        var i = cwd.IndexOf("/.claude/worktrees/", StringComparison.OrdinalIgnoreCase);
        return (i > 0 ? cwd[..i] : cwd).Split('/')[^1];
    }

    // The repo root for a worktree cwd, else the cwd itself.
    internal static string RootOf(string cwd)
    {
        var m = Regex.Match(cwd, @"[\\/]\.claude[\\/]worktrees[\\/]", RegexOptions.IgnoreCase);
        return m.Success && m.Index > 0 ? cwd[..m.Index] : cwd;
    }

    // C:\Users\me\repo\api -> ~\repo\api
    internal static string Short(string path)
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        return path.StartsWith(home + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase) ? "~" + path[home.Length..] : path;
    }

    static string? Find(string id)
    {
        try { return Directory.EnumerateDirectories(Root).Select(d => Path.Combine(d, id + ".jsonl")).FirstOrDefault(File.Exists); }
        catch (IOException) { return null; }
    }

    // Head and tail (64 KB each) are enough: metadata lines are rewritten near the end, cwd/branch sit in the first user line.
    static PastSession? Read(FileInfo f, bool ledger = true)
    {
        string text;
        try
        {
            using var fs = new FileStream(f.FullName, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
            text = fs.Length <= 2 * Chunk ? ReadAt(fs, 0, (int)fs.Length) : ReadAt(fs, 0, Chunk) + "\n" + ReadAt(fs, fs.Length - Chunk, Chunk);
        }
        catch (IOException) { return null; }

        string? cwd = Field(text, "cwd"), branch = Field(text, "gitBranch");
        if (cwd is null) return null;   // no message yet
        string? custom = null, agent = null, ai = null, last = null, firstUser = null, worktree = null;
        decimal? cost = null;
        foreach (var line in text.Split('\n'))
        {
            if (!line.StartsWith("{\"type\":\"") && !(firstUser is null && line.Contains("\"type\":\"user\""))) continue;
            if (Parse(line) is not { } e) continue;
            switch (Events.Str(e, "type"))
            {
                case "custom-title": custom = Events.Str(e, "customTitle") ?? custom; break;
                case "agent-name": agent = Events.Str(e, "agentName") ?? agent; break;
                case "ai-title": ai = Events.Str(e, "aiTitle") ?? ai; break;
                case "last-prompt": last = Events.Str(e, "lastPrompt") ?? last; break;
                case "cost-state" when Events.Prop(e, "totalCostUSD") is { ValueKind: JsonValueKind.Number } n: cost = n.GetDecimal(); break;
                case "worktree-state" when Events.Prop(e, "worktreeSession") is { } w: worktree = Events.Str(w, "worktreePath") ?? worktree; break;
                case "user" when firstUser is null && Events.Prop(e, "isMeta") is not { ValueKind: JsonValueKind.True } && Events.Prop(e, "message") is { } m
                                 && Events.Str(m, "content") is { Length: > 0 } c && !c.StartsWith('<'):
                    firstUser = c;
                    break;
            }
        }
        var title = custom ?? agent ?? ai ?? Trunc(last) ?? Trunc(firstUser) ?? Path.GetFileName(Path.TrimEndingDirectorySeparator(cwd));
        var id = Path.GetFileNameWithoutExtension(f.Name);
        if (ledger && RecordedCost(id) is { } rec) cost = Math.Max(cost ?? 0, rec);
        // Latest permissionMode of the file (permission-mode records and user lines both carry it).
        var modes = Regex.Matches(text, "\"permissionMode\":\"([A-Za-z]+)\"");
        return new(id, cwd, branch, title, cost, f.LastWriteTime, worktree, modes.Count > 0 ? modes[^1].Groups[1].Value : null);
    }

    static string ReadAt(FileStream fs, long at, int count)
    {
        var buf = new byte[count];
        fs.Position = at;
        fs.ReadExactly(buf);
        return Encoding.UTF8.GetString(buf);
    }

    static JsonElement? Parse(string line)
    {
        try { return JsonDocument.Parse(line).RootElement; }
        catch (JsonException) { return null; }   // cut at a chunk edge
    }

    // First "name":"value" anywhere in the text; works on lines cut at a chunk edge.
    static string? Field(string text, string name)
    {
        var m = Regex.Match(text, $"\"{name}\":(\"(?:[^\"\\\\]|\\\\.)*\")");
        return m.Success ? JsonSerializer.Deserialize<string>(m.Groups[1].Value) is { Length: > 0 } v ? v : null : null;
    }

    static string? Trunc(string? s)
    {
        if (string.IsNullOrWhiteSpace(s)) return null;
        s = string.Join(' ', s.Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries));
        return s.Length <= 60 ? s : s[..59] + "…";
    }

    // Slug of the first words of a prompt: "Ajoute la persistance des sessions" -> "ajoute-la-persistance-des".
    internal static string NameFrom(string text, int words = 4)
    {
        var plain = new string(text.Normalize(NormalizationForm.FormD).Where(ch => char.GetUnicodeCategory(ch) != System.Globalization.UnicodeCategory.NonSpacingMark).ToArray());
        return string.Join('-', Regex.Matches(plain.ToLowerInvariant(), "[a-z0-9]+").Select(m => m.Value).Take(words));
    }

    // Shapes of real transcript lines (CLI 2.1.295).
    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "TranscriptStore: " + what);
        Ok(NameFrom("Ajoute la persistance des sessions : un fichier") == "ajoute-la-persistance-des", "NameFrom");
        Ok(NameFrom("Évite l'échec") == "evite-l-echec", "NameFrom accents");
        Ok(RepoName(@"C:\repo\api\.claude\worktrees\x") == "api" && RepoName(@"C:\repo\api") == "api", "RepoName");

        var path = Path.Combine(Path.GetTempPath(), $"cc-ui-check-{Guid.NewGuid()}.jsonl");
        File.WriteAllLines(path, [
            """{"type":"permission-mode","permissionMode":"default","sessionId":"s"}""",
            """{"parentUuid":null,"isSidechain":false,"type":"user","message":{"role":"user","content":"<command-name>/x</command-name>"},"cwd":"C:\\repo\\api","gitBranch":"main","timestamp":"2026-10-09T12:00:00Z"}""",
            """{"parentUuid":"a","isSidechain":false,"type":"user","message":{"role":"user","content":"corrige   le test\nd'auth"},"cwd":"C:\\repo\\api","gitBranch":"main","timestamp":"2026-10-09T12:00:01Z"}""",
            """{"parentUuid":"b","isSidechain":false,"type":"assistant","message":{"model":"claude-haiku-5-5","id":"m1","role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":"C:\\repo\\api\\a.txt","old_string":"a","new_string":"b"}}]},"timestamp":"2026-10-09T12:00:02Z"}""",
            """{"parentUuid":"c","isSidechain":false,"type":"user","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"ok"}]},"toolUseResult":{"filePath":"C:\\repo\\api\\a.txt","structuredPatch":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-a","+b"]}]},"timestamp":"2026-10-09T12:00:04Z"}""",
            """{"parentUuid":"d","isSidechain":false,"type":"assistant","message":{"model":"claude-haiku-5-5","id":"m2","role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Read","input":{"file_path":"C:\\repo\\api\\b.txt"}}]},"timestamp":"2026-10-09T12:00:05Z"}""",
            """{"type":"last-prompt","lastPrompt":"corrige le test d'auth","sessionId":"s"}""",
            """{"type":"cost-state","sessionId":"s","totalCostUSD":0.66}""",
        ]);
        try
        {
            var p = Read(new FileInfo(path));
            Ok(p is { Cwd: @"C:\repo\api", Branch: "main", Title: "corrige le test d'auth", CostUsd: 0.66m, WorktreePath: null, Mode: "default" }, "Read last-prompt + cost");
            File.AppendAllLines(path, ["""{"type":"permission-mode","permissionMode":"plan","sessionId":"s"}"""]);
            Ok(Read(new FileInfo(path)) is { Mode: "plan" }, "latest permission mode");
            File.AppendAllLines(path, ["""{"type":"custom-title","customTitle":"auth-fix","sessionId":"s"}"""]);
            Ok(Read(new FileInfo(path)) is { Title: "auth-fix" }, "custom-title wins");
            File.WriteAllLines(path, File.ReadAllLines(path).Where(l => !l.Contains("-title") && !l.Contains("last-prompt")));
            Ok(Read(new FileInfo(path)) is { Title: "corrige le test d'auth" }, "first user text, tags skipped");

            var items = Replay(File.ReadLines(path));
            Ok(items is [UserItem, UserItem, ToolItem { State: ToolState.Done, Structured: not null } t1, ToolItem { State: ToolState.Error } t2]
               && t1.EndedAt - t1.StartedAt == TimeSpan.FromSeconds(2) && t2.EndedAt == t2.StartedAt, "replay");
        }
        finally { File.Delete(path); }

        // The cost-state is stale (clean exits only); the UI ledger carries the real spend.
        var cid = Guid.NewGuid().ToString();
        var cpath = Path.Combine(Path.GetTempPath(), cid + ".jsonl");
        File.WriteAllLines(cpath, [
            """{"type":"user","message":{"role":"user","content":"x"},"cwd":"C:\\repo\\api","gitBranch":"main"}""",
            """{"type":"cost-state","sessionId":"s","totalCostUSD":0.00062763}""",
        ]);
        try
        {
            RecordCost(cid, 0.01506324m);
            Ok(Read(new FileInfo(cpath)) is { CostUsd: 0.01506324m }, "ledger beats stale cost-state");
            Ok(CostState(new FileInfo(cpath)) == 0.00062763m, "cost-state stays raw");
        }
        finally { File.Delete(cpath); DeleteCost(cid); }

        // Background agent: launch receipt, then the task-notification 5 s later (no duration tag → timestamp decides).
        var bg = Replay([
            """{"type":"assistant","message":{"id":"m","role":"assistant","content":[{"type":"tool_use","id":"toolu_A","name":"Agent","input":{}}]},"timestamp":"2026-10-09T12:00:00Z"}""",
            """{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_A","type":"tool_result","content":"Async agent launched"}]},"toolUseResult":{"isAsync":true,"status":"async_launched","agentId":"aa5"},"timestamp":"2026-10-09T12:00:01Z"}""",
            """{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>aa5</task-id>\n<tool-use-id>toolu_A</tool-use-id>\n<status>completed</status>\n<result>pong</result>\n<usage><subagent_tokens>31599</subagent_tokens></usage>\n</task-notification>"},"timestamp":"2026-10-09T12:00:05Z"}""",
        ]);
        var api = Replay(["""{"type":"assistant","isApiErrorMessage":true,"message":{"model":"<synthetic>","id":"m3","role":"assistant","content":[{"type":"text","text":"API Error: Output blocked by content filtering policy"}]},"timestamp":"2026-10-09T12:00:00Z"}"""]);
        Ok(api is [ApiErrorItem], "replay api error");
        Ok(bg is [ToolItem { State: ToolState.Done, ResultText: "pong", Tokens: 31599 } a] && a.EndedAt - a.StartedAt == TimeSpan.FromSeconds(5), "replay background agent");
    }
}
