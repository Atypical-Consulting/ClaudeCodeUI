using System.Collections.Concurrent;
using System.Diagnostics;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace ClaudeCodeUI;

public enum WtState { Safe, Check, Active, Orphan }

public sealed record WorktreeInfo(string Repo, string Path, string Name, string? Branch, string? Head,
    WtState State, string Why, long? SizeBytes, DateTimeOffset LastActivity,
    bool Locked, string? LockReason, int Dirty, int Unpushed, bool Merged, bool SquashMerged, bool UpstreamGone, bool BranchOnly = false, string? Unreadable = null,
    string? Pushed = null, int? RecentMin = null, string? Kept = null)   // Pushed: the remote ref holding every commit; RecentMin: touched under an hour ago; Kept: ignored files that are not build output
{
    // Commits nothing else holds: merged or squash-merged commits are kept by the base even with no remote.
    public int AtRisk => Merged || SquashMerged ? 0 : Unpushed;
}

public sealed record CleanupStep(string Repo, string Display, string[] GitArgs, bool FailureIsFatal, bool KeptOk = false);   // KeptOk: the user accepted losing the row's ignored files

// Facts collected by git for one worktree; Classify turns them into a state.
public sealed record WtFacts(int Dirty = 0, int Ahead = 0, bool Merged = false, bool SquashMerged = false, bool UpstreamGone = false,
    bool Locked = false, int? LockPid = null, bool PidAlive = false, bool Exists = true, bool Prunable = false,
    string? ActiveSessionName = null, bool Detached = false, bool NoWorktree = false, string? Unreadable = null,   // Unreadable: git failed, never Safe
    string? OnRemote = null, string? InUseBy = null, int? RecentMin = null, string? TwinOf = null, bool TwinActive = false, string? Kept = null,
    string? OnRemoteStale = null, string? RemoteGone = null, int? MergedPr = null, string? DirtyNames = null);   // DirtyNames: the first changed files, named in the reason   // MergedPr: GitHub says a PR holding this tip was merged into the base   // RemoteGone: only a ref to a branch origin deleted holds it. OnRemoteStale: a remote-tracking ref holds it, but origin could not be asked whether that branch still exists
// OnRemote: a remote ref holds every commit (nothing is lost with the folder). InUseBy: "name (pid N)" of a process whose cwd is inside.
// RecentMin: touched under an hour ago, so maybe still in use. TwinOf: the folder a worktree-<folder> branch was made for (now elsewhere).
// Kept: ignored files that are not build output (git worktree remove deletes ignored files without --force).

// Scan, classify and safely clean the worktrees of known repos. Never --force, never -D.
public sealed class WorktreeService(SessionManager sessions)
{
    internal SessionManager Sessions { get; } = sessions;

    readonly ConcurrentDictionary<string, int> cleanable = new(StringComparer.OrdinalIgnoreCase);
    IReadOnlyList<string>? repos;

    public event Action? Changed;   // raised after a scan changes CleanableCount

    internal void SetCleanable(string repo, int count)
    {
        var had = cleanable.TryGetValue(repo, out var old);
        cleanable[repo] = count;
        if (!had || old != count) Changed?.Invoke();
    }

    public int? CleanableCount => cleanable.IsEmpty ? null : cleanable.Values.Sum();   // Safe + Orphan of the last scan

    // The last full scan of each repo: a page opens on it at once and reconciles with its own scan (a scan takes seconds).
    readonly ConcurrentDictionary<string, IReadOnlyList<WorktreeInfo>> lastScan = new(StringComparer.OrdinalIgnoreCase);
    public DateTimeOffset? CachedAt { get; private set; }
    public IReadOnlyList<WorktreeInfo> Cached => [.. lastScan.Values.SelectMany(x => x)];

    // Repo roots: cwds of the live sessions, plus the newest transcript folders of the last 30 days (not under %TEMP%).
    // Cached; refresh = true rescans.
    public async Task<IReadOnlyList<string>> DiscoverReposAsync(CancellationToken ct) => await DiscoverReposAsync(false, ct);

    // Roots resolved from live sessions are remembered here (and in ReposFile) until the repo vanishes from disk.
    internal string ReposFile = System.IO.Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ClaudeCodeUI", "repos.json");
    readonly object rememberedGate = new();
    HashSet<string>? remembered;

    public async Task<IReadOnlyList<string>> DiscoverReposAsync(bool refresh, CancellationToken ct)
    {
        var live = Sessions.All.Select(s => Norm(StripWorktree(s.Cwd))).ToHashSet(StringComparer.OrdinalIgnoreCase);
        string[] mem;
        lock (rememberedGate) mem = [.. remembered ??= LoadRemembered()];
        if (repos is null || refresh) repos = await Task.Run(() => FromTranscripts().ToList(), ct);
        var roots = new ConcurrentDictionary<string, byte>(StringComparer.OrdinalIgnoreCase);
        var liveRoots = new ConcurrentDictionary<string, byte>(StringComparer.OrdinalIgnoreCase);
        var dead = new ConcurrentDictionary<string, byte>(StringComparer.OrdinalIgnoreCase);
        await Parallel.ForEachAsync(live.Concat(mem).Concat(repos).Distinct(StringComparer.OrdinalIgnoreCase), ct, async (cwd, ct) =>
        {
            var r = await RootOf(cwd, ct);
            if (r is null) { dead.TryAdd(Norm(cwd), 0); return; }
            roots.TryAdd(r, 0);
            if (live.Contains(Norm(cwd))) liveRoots.TryAdd(r, 0);
        });
        lock (rememberedGate)
        {
            var before = remembered!.ToArray();
            remembered.UnionWith(liveRoots.Keys.Select(Norm));
            foreach (var d in dead.Keys) if (!live.Contains(d) && !Directory.Exists(d)) remembered.Remove(d);   // a folder that still exists may just be a transient git failure
            if (!remembered.SetEquals(before)) SaveRemembered();
        }
        return [.. roots.Keys.Order(StringComparer.OrdinalIgnoreCase)];
    }

    HashSet<string> LoadRemembered()
    {
        try { return new((JsonSerializer.Deserialize<string[]>(File.ReadAllText(ReposFile)) ?? []).Select(Norm), StringComparer.OrdinalIgnoreCase); }
        catch (Exception) { return new(StringComparer.OrdinalIgnoreCase); }   // missing or corrupt: nothing remembered
    }

    void SaveRemembered()
    {
        try
        {
            Directory.CreateDirectory(System.IO.Path.GetDirectoryName(ReposFile)!);
            File.WriteAllText(ReposFile, JsonSerializer.Serialize(remembered!.Order(StringComparer.OrdinalIgnoreCase)));
        }
        catch (Exception) { }   // a cache: losing it only costs rediscovery
    }

    static string StripWorktree(string cwd) => Regex.Replace(cwd, @"[\\/]\.claude[\\/]worktrees[\\/][^\\/]+$", "");

    static IEnumerable<string> FromTranscripts()
    {
        var dir = TranscriptStore.Root;
        if (!Directory.Exists(dir)) yield break;
        var temp = TranscriptStore.Slug(System.IO.Path.TrimEndingDirectorySeparator(System.IO.Path.GetTempPath()));
        var since = DateTime.Now.AddDays(-30);
        var folders = new DirectoryInfo(dir).EnumerateDirectories()
            .Where(d => d.LastWriteTime > since && !d.Name.StartsWith(temp, StringComparison.OrdinalIgnoreCase))
            .OrderByDescending(d => d.LastWriteTime).Take(200);   // ponytail: cap, raise if real repos get missed
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var d in folders)
        {
            var f = d.EnumerateFiles("*.jsonl").MaxBy(f => f.LastWriteTime);
            if (f is null || CwdOf(f.FullName) is not { } cwd) continue;
            cwd = StripWorktree(cwd);
            if (Directory.Exists(cwd) && seen.Add(cwd)) yield return cwd;
        }
    }

    static string? CwdOf(string jsonl)
    {
        try
        {
            using var fs = new FileStream(jsonl, FileMode.Open, FileAccess.Read, FileShare.ReadWrite);
            var buf = new byte[64 * 1024];
            var head = Encoding.UTF8.GetString(buf, 0, fs.Read(buf));
            var m = Regex.Match(head, "\"cwd\":(\"(?:[^\"\\\\]|\\\\.)*\")");
            return m.Success ? JsonSerializer.Deserialize<string>(m.Groups[1].Value) : null;
        }
        catch (Exception) { return null; }
    }

    static async Task<string?> RootOf(string cwd, CancellationToken ct)
    {
        if (!Directory.Exists(cwd)) return null;
        var (exit, o, _) = await Git(cwd, ct, "rev-parse", "--path-format=absolute", "--git-common-dir");
        if (exit != 0) return null;
        var common = o.Trim().TrimEnd('/', '\\');
        return System.IO.Path.GetFileName(common) == ".git" ? Norm(System.IO.Path.GetDirectoryName(common)!) : null;   // bare repos skipped
    }

    // Every worktree of the repo but the main one, classified (report §2–§3). SizeBytes stays null: see SizeAsync.
    // only: re-validate that one worktree path (no orphan branches, CleanableCount untouched).
    public async Task<IReadOnlyList<WorktreeInfo>> ScanAsync(string repoRoot, CancellationToken ct, string? only = null)
    {
        var (exit, list, err) = await Git(repoRoot, ct, "worktree", "list", "--porcelain", "-z");
        if (exit != 0) throw new InvalidOperationException(err.Trim());
        var records = list.Split("\0\0", StringSplitOptions.RemoveEmptyEntries)
            .Select(r => r.Split('\0').Select(l => l.Split(' ', 2)).GroupBy(kv => kv[0]).ToDictionary(g => g.Key, g => g.First().ElementAtOrDefault(1) ?? ""))
            .Where(r => r.ContainsKey("worktree")).ToList();
        if (records.Count == 0) return [];

        var bas = await BaseBranch(repoRoot, records[0], ct);
        var baseTree = (await Git(repoRoot, ct, "rev-parse", bas + "^{tree}")).Out.Trim();
        var hasRemoteBase = (await Git(repoRoot, ct, "rev-parse", "--verify", "-q", "refs/remotes/origin/" + bas)).Exit == 0;
        // Unborn or missing base (repo with no commits): nothing is merged into it, and --merged would exit 128.
        var baseOk = (await Git(repoRoot, ct, "rev-parse", "--verify", "-q", bas + "^{commit}")).Exit == 0;
        // Two --merged filters are a union for git: in the local base or in origin's, the content is kept either way.
        var merged = !baseOk ? [] : Ok(await Git(repoRoot, ct, ["for-each-ref", "refs/heads", "--merged", bas, .. hasRemoteBase ? ["--merged", "origin/" + bas] : Array.Empty<string>(), "--format=%(refname:short)"]))
            .Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries).ToHashSet();
        var refs = Ok(await Git(repoRoot, ct, "for-each-ref", "refs/heads", "--format=%(refname:short)%00%(upstream:short)%00%(upstream:track,nobracket)%00%(committerdate:iso-strict)"))
            .Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(l => l.TrimEnd('\r').Split('\0')).Where(p => p.Length == 4).ToDictionary(p => p[0]);
        var live = Sessions.All.Where(s => s.HasProcess).ToList();
        var cwds = ProcessCwds();
        var liveHeads = await LiveHeads(repoRoot, ct);
        var basePids = new ConcurrentDictionary<string, Task<HashSet<string>>>();
        var prs = await MergedPrs(repoRoot, bas, ct);
        // A PR merged into the base whose head is this tip, or whose head this tip leads to (the PR got commits the local branch
        // never fetched): the local objects cannot show that squash, GitHub can. Asked only where git alone would say "not in base".
        async Task<int?> PrOf(string? branch, string tip)
        {
            if (prs.FirstOrDefault(pr => pr.Head == tip) is { Number: > 0 } exact) return exact.Number;
            foreach (var pr in prs.Where(pr => pr.Branch == branch))
                if ((await Git(repoRoot, ct, "merge-base", "--is-ancestor", tip, pr.Head)).Exit == 0 || await Behind0(repoRoot, tip, pr.Head, ct)) return pr.Number;
            return null;
        }
        // Squash-merged: the branch's whole diff (or its upstream's, when the PR got more commits than the local branch saw)
        // has the same patch-id as a commit of the base. Read-only, unlike a trial merge it survives the base moving on.
        async Task<bool> Squashed(string rev, string? upstream)
        {
            if (upstream is not null && (await Git(repoRoot, ct, "merge-base", "--is-ancestor", rev, upstream)).Exit == 0) rev = upstream;
            var mb = (await Git(repoRoot, ct, "merge-base", bas, rev)).Out.Trim();
            if (mb.Length == 0 || await PatchId(repoRoot, ct, "diff", mb, rev) is not { } pid) return false;
            return (await basePids.GetOrAdd(mb, m => BasePatchIds(repoRoot, m, bas, ct))).Contains(pid);
        }

        var rows = await Task.WhenAll(records.Skip(1).Where(r => only is null || string.Equals(Norm(r["worktree"]), only, StringComparison.OrdinalIgnoreCase)).Select(async r =>
        {
            var path = Norm(r["worktree"]);
            var branch = r.TryGetValue("branch", out var b) ? b.Replace("refs/heads/", "") : null;
            var exists = Directory.Exists(path);
            string? unreadable = null;
            var dirty = 0;
            string? dirtyNames = null;
            if (exists)
            {
                var st = await Git(path, ct, "status", "--porcelain=v1", "-uall");
                if (st.Exit == 0)
                {
                    var changed = st.Out.Split('\n', StringSplitOptions.RemoveEmptyEntries);
                    dirty = changed.Length;
                    dirtyNames = NamesOf(changed.Select(l => l.Length > 3 ? l[3..].Trim().Trim('"') : l));
                }
                else unreadable = st.Err.Trim() is { Length: > 0 } m ? m : $"git status: exit {st.Exit}";
            }
            refs.TryGetValue(branch ?? "", out var rf);
            var track = rf?[2] ?? "";
            var (ahead, aheadErr) = branch is null ? (0, null) : await Ahead(repoRoot, branch, rf, ct);
            unreadable ??= aheadErr;
            var isMerged = branch is not null ? merged.Contains(branch)
                : r.TryGetValue("HEAD", out var h) && (await Git(repoRoot, ct, "merge-base", "--is-ancestor", h, bas)).Exit == 0;
            var squash = false;
            if (!isMerged && branch is not null)
            {
                var mt = await Git(repoRoot, ct, "merge-tree", "--write-tree", bas, branch);
                squash = mt.Exit == 0 && mt.Out.Split('\n')[0].Trim() == baseTree || baseOk && await Squashed(branch, string.IsNullOrEmpty(rf?[1]) ? null : rf[1]);
            }
            else if (!isMerged && baseOk && r.GetValueOrDefault("HEAD") is { Length: > 0 } dh) squash = await Squashed(dh, await AnyRemoteRef(repoRoot, dh, ct));   // the PR branch may hold more than the agent saw
            r.TryGetValue("locked", out var reason);
            var pid = PidOf(reason);
            var session = live.FirstOrDefault(s => Inside(s.Cwd, path));
            var head = branch ?? r.GetValueOrDefault("HEAD");
            var (onRemote, confirmed, gone) = !isMerged && !squash && ahead == 0 && unreadable is null && head is not null ? await RemoteHolding(repoRoot, head, liveHeads, ct) : (null, false, null);
            var user = cwds.FirstOrDefault(c => Inside(c.Cwd, path));
            var kept = exists && unreadable is null ? await KeptIgnored(path, ct) : null;
            var pr = !isMerged && !squash && !confirmed && ahead == 0 && unreadable is null && r.GetValueOrDefault("HEAD") is { Length: > 0 } tipSha && prs.Count > 0 ? await PrOf(branch, tipSha) : null;   // a sha: GitHub may no longer know the branch name
            var touched = Activity(path);
            var moved = HeadMoved(path);   // created, committed, checked out: a git status elsewhere does not restart this clock
            int? recent = moved > DateTimeOffset.MinValue && DateTimeOffset.Now - moved < TimeSpan.FromHours(1) ? (int)(DateTimeOffset.Now - moved).TotalMinutes : null;
            var facts = new WtFacts(dirty, ahead, isMerged, squash, track == "gone", reason is not null, pid, pid is { } p && Alive(p),
                exists, r.ContainsKey("prunable"), session?.Name, branch is null, Unreadable: unreadable,
                OnRemote: confirmed ? onRemote : null, InUseBy: user.Pid == 0 ? null : $"{user.Name} (pid {user.Pid})", RecentMin: recent, Kept: kept,
                OnRemoteStale: confirmed ? null : onRemote, RemoteGone: gone, MergedPr: pr, DirtyNames: dirtyNames);
            var (state, why) = Classify(facts);
            if (session?.Status == SessionStatus.Waiting) why = Strings.Get("Wt.Why.Waiting", Enc(session.Name));
            var last = new[] { touched, DateTimeOffset.TryParse(rf?[3], out var cd) ? cd : DateTimeOffset.MinValue }.Max();
            var idle = (int)(DateTimeOffset.Now - last).TotalDays;
            if (state is WtState.Safe or WtState.Check && idle >= 2 && last > DateTimeOffset.MinValue) why += Strings.Get("Wt.Why.Idle", idle);
            return new WorktreeInfo(Norm(repoRoot), path, System.IO.Path.GetFileName(path), branch, r.GetValueOrDefault("HEAD"),
                state, why.Replace("{base}", Enc(bas)), null, last, reason is not null, reason, dirty, ahead, isMerged, squash || pr is not null, track == "gone", Unreadable: unreadable,
                Pushed: confirmed ? onRemote : null, RecentMin: recent, Kept: kept);
        }));
        // worktree-* branches that no worktree checks out (main record included) have no row above: surface them.
        if (only is not null) return rows;
        var orphans = await Task.WhenAll(OrphanBranches(refs.Keys, records.Select(r => r.TryGetValue("branch", out var b) ? b.Replace("refs/heads/", "") : null)).Select(async branch =>
        {
            var rf = refs[branch];
            var track = rf[2];
            var (ahead, aheadErr) = await Ahead(repoRoot, branch, rf, ct);
            var isMerged = merged.Contains(branch);
            var squash = false;
            if (!isMerged)
            {
                var mt = await Git(repoRoot, ct, "merge-tree", "--write-tree", bas, branch);
                squash = mt.Exit == 0 && mt.Out.Split('\n')[0].Trim() == baseTree || baseOk && await Squashed(branch, string.IsNullOrEmpty(rf[1]) ? null : rf[1]);
            }
            // claude -w names the branch worktree-<folder>: when that folder is still listed (it went detached), say whose branch it is.
            var twin = rows.FirstOrDefault(w => "worktree-" + w.Name == branch);
            var (onRemote, confirmed, gone) = !isMerged && !squash && ahead == 0 && aheadErr is null ? await RemoteHolding(repoRoot, branch, liveHeads, ct) : (null, false, null);
            var pr = !isMerged && !squash && !confirmed && ahead == 0 && aheadErr is null && prs.Count > 0 ? await PrOf(branch, (await Git(repoRoot, ct, "rev-parse", branch)).Out.Trim()) : null;
            var (state, why) = Classify(new WtFacts(0, ahead, isMerged, squash, track == "gone", NoWorktree: true, Unreadable: aheadErr,
                OnRemote: confirmed ? onRemote : null, TwinOf: twin?.Name, TwinActive: twin?.State == WtState.Active, OnRemoteStale: confirmed ? null : onRemote, RemoteGone: gone, MergedPr: pr));   // a twin is a branch: no folder, no ignored files
            // Full branch name: the short one is the folder name of the worktree it came from, which may still be live.
            return new WorktreeInfo(Norm(repoRoot), $"{Norm(repoRoot)}/refs/heads/{branch}", branch, branch, null,
                state, why.Replace("{base}", Enc(bas)), null, DateTimeOffset.TryParse(rf[3], out var cd) ? cd : default, false, null, 0, ahead, isMerged, squash, track == "gone", BranchOnly: true, Pushed: confirmed ? onRemote : null);
        }));
        WorktreeInfo[] all = [.. rows, .. orphans];
        SetCleanable(Norm(repoRoot), all.Count(w => w.State is WtState.Safe or WtState.Orphan));
        lastScan[Norm(repoRoot)] = all;
        CachedAt = DateTimeOffset.Now;
        return all;
    }

    // Commits on the branch that no remote has, or the git error (unknown is not 0).
    static async Task<(int Ahead, string? Err)> Ahead(string repoRoot, string branch, string[]? rf, CancellationToken ct)
    {
        if (Regex.Match(rf?[2] ?? "", @"ahead (\d+)") is { Success: true } a) return (int.Parse(a.Groups[1].Value), null);
        if (!string.IsNullOrEmpty(rf?[1]) && rf[2] != "gone") return (0, null);   // upstream gone: count against every remote instead
        var (exit, o, e) = await Git(repoRoot, ct, "rev-list", "--count", branch, "--not", "--remotes");
        return exit == 0 && int.TryParse(o.Trim(), out var n) ? (n, null) : (0, e.Trim() is { Length: > 0 } m ? m : $"git rev-list: exit {exit}");
    }

    // Ignored paths that are not known build output (a .env, an agent log, a dump): "safe" must not mean they go unseen.
    // ponytail: a fixed list of regenerable directories; extend it when a real build dir shows up as kept.
    static readonly Regex Regenerable = new(@"(^|/)(bin|obj|target|node_modules|dist|build|out|\.build|\.next|\.nuxt|\.cache|__pycache__|\.venv|venv|\.vs|\.idea|coverage|TestResults|\.gradle|\.dart_tool|\.turbo|\.parcel-cache|\.pytest_cache|\.mypy_cache|\.swiftpm|src-tauri/gen|src-tauri/server|\.nuke/temp)/$|(^|/)\.DS_Store$|\.pyc$|\.cache\.json$", RegexOptions.Compiled);

    // "Composer.razor, LiveSession.cs +5": the first files by name, so the reason says what is at stake.
    internal static string? NamesOf(IEnumerable<string> paths)
    {
        var names = paths.Select(p => System.IO.Path.GetFileName(p.Split(" -> ")[^1].TrimEnd('/')) is { Length: > 0 } n ? n : p).Distinct().ToList();
        return names.Count == 0 ? null : string.Join(", ", names.Take(2)) + (names.Count > 2 ? $" +{names.Count - 2}" : "");
    }

    static async Task<string?> KeptIgnored(string path, CancellationToken ct)
    {
        var (exit, o, _) = await Git(path, ct, "ls-files", "--others", "--ignored", "--exclude-standard", "--directory");
        if (exit != 0) return null;
        var kept = KeptOf(o);
        return kept.Count == 0 ? null : kept.Count <= 2 ? string.Join(", ", kept) : $"{kept[0]}, {kept[1]} +{kept.Count - 2}";
    }

    internal static List<string> KeptOf(string lsOutput) =>
        [.. lsOutput.Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries).Where(l => !Regenerable.IsMatch(l))];

    // A remote branch that contains `rev`, and whether origin confirmed that branch still exists. A remote-tracking ref is a
    // local cache: a branch deleted on the server (merged PR) keeps its ref until a prune, so it proves nothing by itself.
    // Gone: the only refs left point at branches origin no longer has (a merged PR whose branch was deleted).
    static async Task<(string? Ref, bool Confirmed, string? Gone)> RemoteHolding(string repoRoot, string rev, IReadOnlySet<string>? live, CancellationToken ct)
    {
        var refs = await RemoteRefs(repoRoot, rev, ct);
        if (live is null) return (refs.FirstOrDefault(), false, null);   // origin unreachable: the ref is all we have, say so
        return refs.FirstOrDefault(live.Contains) is { } r ? (r, true, null) : (null, false, refs.FirstOrDefault());
    }

    static async Task<List<string>> RemoteRefs(string repoRoot, string rev, CancellationToken ct)
    {
        var (exit, o, _) = await Git(repoRoot, ct, "for-each-ref", "--contains", rev, "--format=%(refname:short)", "refs/remotes");
        return exit != 0 ? [] : [.. o.Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries).Where(r => !r.EndsWith("/HEAD"))];
    }

    static async Task<string?> AnyRemoteRef(string repoRoot, string rev, CancellationToken ct) => (await RemoteRefs(repoRoot, rev, ct)).FirstOrDefault();

    // The branches origin has right now (origin/<name>), or null when it cannot be asked (offline, no origin). Cached 2 min:
    // a cleanup run re-scans each row before it acts.
    static readonly ConcurrentDictionary<string, (DateTimeOffset At, IReadOnlySet<string>? Heads)> liveCache = new(StringComparer.OrdinalIgnoreCase);
    static async Task<IReadOnlySet<string>?> LiveHeads(string repoRoot, CancellationToken ct)
    {
        if (liveCache.TryGetValue(repoRoot, out var c) && DateTimeOffset.Now - c.At < TimeSpan.FromMinutes(2)) return c.Heads;
        using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct);
        cts.CancelAfter(TimeSpan.FromSeconds(10));
        IReadOnlySet<string>? heads = null;
        try
        {
            var (exit, o, _) = await Git(repoRoot, cts.Token, "ls-remote", "--heads", "origin");
            if (exit == 0) heads = o.Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(l => l.Split('\t')).Where(p => p.Length == 2 && p[1].StartsWith("refs/heads/"))
                .Select(p => "origin/" + p[1].Trim()["refs/heads/".Length..]).ToHashSet();
        }
        catch (OperationCanceledException) when (!ct.IsCancellationRequested) { }   // too slow: unknown, not empty
        liveCache[repoRoot] = (DateTimeOffset.Now, heads);
        return heads;
    }

    // PRs merged into `bas` (gh, optional): none when gh is missing, not logged in, or the repo is not on GitHub. Cached 10 min.
    internal sealed record MergedPrInfo(int Number, string Branch, string Head);
    static readonly ConcurrentDictionary<string, (DateTimeOffset At, List<MergedPrInfo> Prs)> prCache = new(StringComparer.OrdinalIgnoreCase);
    static async Task<List<MergedPrInfo>> MergedPrs(string repoRoot, string bas, CancellationToken ct)
    {
        var key = repoRoot + "|" + bas;
        if (prCache.TryGetValue(key, out var c) && DateTimeOffset.Now - c.At < TimeSpan.FromMinutes(10)) return c.Prs;
        var list = new List<MergedPrInfo>();
        if (await Gh(repoRoot, ct, "pr", "list", "--state", "merged", "--base", bas, "--limit", "300", "--json", "number,headRefName,headRefOid") is { } json)
            try
            {
                foreach (var e in JsonDocument.Parse(json).RootElement.EnumerateArray())
                    list.Add(new(e.GetProperty("number").GetInt32(), e.GetProperty("headRefName").GetString() ?? "", e.GetProperty("headRefOid").GetString() ?? ""));
            }
            catch (JsonException) { }
        prCache[key] = (DateTimeOffset.Now, list);
        return list;
    }

    // GitHub's compare: tip is an ancestor of head when head is 0 commits behind it. Immutable, so cached for good.
    static readonly ConcurrentDictionary<string, bool> behindCache = new();
    static async Task<bool> Behind0(string repoRoot, string tip, string head, CancellationToken ct)
    {
        var key = tip + "..." + head;
        if (behindCache.TryGetValue(key, out var b)) return b;
        var o = await Gh(repoRoot, ct, "api", $"repos/{{owner}}/{{repo}}/compare/{tip}...{head}", "--jq", ".behind_by");
        if (o is null) return false;   // unknown is not merged: no cache, ask again next scan
        return behindCache[key] = o.Trim() == "0";
    }

    static async Task<string?> Gh(string dir, CancellationToken ct, params string[] args)
    {
        var psi = new ProcessStartInfo("gh") { RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false, CreateNoWindow = true,
            WorkingDirectory = dir, StandardOutputEncoding = Encoding.UTF8 };
        foreach (var a in args) psi.ArgumentList.Add(a);
        psi.Environment["GH_PROMPT_DISABLED"] = "1";
        using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct);
        cts.CancelAfter(TimeSpan.FromSeconds(15));
        try
        {
            using var p = Process.Start(psi)!;
            var o = p.StandardOutput.ReadToEndAsync(cts.Token);
            _ = p.StandardError.ReadToEndAsync(cts.Token);
            try { await p.WaitForExitAsync(cts.Token); }
            catch (OperationCanceledException) { try { p.Kill(true); } catch (Exception) { } if (ct.IsCancellationRequested) throw; return null; }
            return p.ExitCode == 0 ? await o : null;
        }
        catch (System.ComponentModel.Win32Exception) { return null; }   // no gh: GitHub is simply not consulted
    }

    static async Task<string?> PatchId(string dir, CancellationToken ct, params string[] diffArgs)
    {
        var (exit, diff, _) = await Git(dir, ct, diffArgs);
        if (exit != 0 || diff.Length == 0) return null;
        var (e2, o, _) = await Git(dir, ct, ["patch-id", "--stable"], diff);
        return e2 == 0 && o.Split(' ', 2)[0].Trim() is { Length: > 0 } id ? id : null;
    }

    // ponytail: the last 2000 commits of the base since the merge-base; a squash older than that reads as not merged (safe side).
    static async Task<HashSet<string>> BasePatchIds(string dir, string mergeBase, string bas, CancellationToken ct)
    {
        var (exit, log, _) = await Git(dir, ct, "log", "-p", "--no-merges", "-n", "2000", mergeBase + ".." + bas);
        if (exit != 0 || log.Length == 0) return [];
        var (e2, o, _) = await Git(dir, ct, ["patch-id", "--stable"], log);
        return e2 != 0 ? [] : o.Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(l => l.Split(' ')[0]).ToHashSet();
    }

    // Working directory of every other process (lsof), cached 5 s: a shell, an editor or a claude CLI inside a worktree
    // means it is in use even when no lock says so. No lsof (Windows): nothing is known, the recency rule still holds.
    static (DateTimeOffset At, (int Pid, string Name, string Cwd)[] List) procCache;
    static (int Pid, string Name, string Cwd)[] ProcessCwds()
    {
        if (DateTimeOffset.Now - procCache.At < TimeSpan.FromSeconds(5)) return procCache.List;
        var list = new List<(int, string, string)>();
        try
        {
            var psi = new ProcessStartInfo("lsof", "-n -P -w -d cwd -Fpcn") { RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false };
            using var p = Process.Start(psi)!;
            var o = p.StandardOutput.ReadToEnd();
            p.WaitForExit(3000);
            list = ParseLsof(o, Environment.ProcessId);
        }
        catch (Exception) { }
        procCache = (DateTimeOffset.Now, [.. list]);
        return procCache.List;
    }

    internal static List<(int Pid, string Name, string Cwd)> ParseLsof(string o, int self)
    {
        var list = new List<(int, string, string)>();
        int pid = 0; string name = "";
        foreach (var l in o.Split('\n'))
        {
            if (l.Length < 2) continue;
            switch (l[0])
            {
                case 'p': pid = int.TryParse(l[1..], out var n) ? n : 0; name = ""; break;
                case 'c': name = l[1..]; break;
                case 'n' when pid != 0 && pid != self && l[1] == '/' && name is not ("lsof" or "git"): list.Add((pid, name, l[1..])); break;   // our own probes are not users
            }
        }
        return list;
    }

    static string Ok((int Exit, string Out, string Err) r) => r.Exit == 0 ? r.Out : throw new InvalidOperationException(r.Err.Trim());

    static async Task<string> BaseBranch(string repo, Dictionary<string, string> main, CancellationToken ct)
    {
        if ((await Git(repo, ct, "symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD")).Out.Trim() is { Length: > 0 } o)
            return o.StartsWith("origin/") ? o[7..] : o;
        foreach (var b in new[] { "main", "master" })
            if ((await Git(repo, ct, "rev-parse", "--verify", "-q", "refs/heads/" + b)).Exit == 0) return b;
        return main.TryGetValue("branch", out var mb) ? mb.Replace("refs/heads/", "") : "HEAD";
    }

    // Newest of the worktree's index (gitdir from the .git file); MinValue when unknown.
    static DateTimeOffset Activity(string path)
    {
        try
        {
            var dotgit = System.IO.Path.Combine(path, ".git");
            if (!File.Exists(dotgit)) return DateTimeOffset.MinValue;
            var gitdir = File.ReadAllText(dotgit).Replace("gitdir:", "").Trim();
            var index = System.IO.Path.Combine(System.IO.Path.GetFullPath(gitdir, path), "index");
            return File.Exists(index) ? File.GetLastWriteTime(index) : DateTimeOffset.MinValue;
        }
        catch (Exception) { return DateTimeOffset.MinValue; }
    }

    // When HEAD last moved in this worktree (its reflog): worktree add, commit, checkout, reset. MinValue when unknown.
    static DateTimeOffset HeadMoved(string path)
    {
        try
        {
            var dotgit = System.IO.Path.Combine(path, ".git");
            if (!File.Exists(dotgit)) return DateTimeOffset.MinValue;
            var gitdir = System.IO.Path.GetFullPath(File.ReadAllText(dotgit).Replace("gitdir:", "").Trim(), path);
            var log = System.IO.Path.Combine(gitdir, "logs", "HEAD");
            return File.Exists(log) ? File.GetLastWriteTime(log) : DateTimeOffset.MinValue;
        }
        catch (Exception) { return DateTimeOffset.MinValue; }
    }

    public Task<long> SizeAsync(string path, CancellationToken ct) => Task.Run(() =>
    {
        if (!Directory.Exists(path)) return 0L;
        var opts = new EnumerationOptions { RecurseSubdirectories = true, IgnoreInaccessible = true, AttributesToSkip = FileAttributes.ReparsePoint };
        long sum = 0;
        foreach (var f in new DirectoryInfo(path).EnumerateFiles("*", opts))
        {
            ct.ThrowIfCancellationRequested();
            sum += f.Length;
        }
        return sum;
    }, ct);

    // Safe and Orphan rows, plus a Check row whose only problem is a lock left by a dead pid (gets an unlock).
    // Anything else is ignored: the main worktree, dirty, unpushed and active rows never reach a plan.
    // keptOk: the user chose "Delete anyway" on a row held only by ignored files that are not build output.
    public static bool Cleanable(WorktreeInfo w, bool keptOk = false) => w.State is WtState.Safe or WtState.Orphan
        || w.State == WtState.Check && w.Locked && PidOf(w.LockReason) is not null && Classify(FactsOf(w) with { Locked = false }).State == WtState.Safe
        || keptOk && w.Kept is not null && w.State == WtState.Check
            && Classify(FactsOf(w) with { Kept = null, Locked = w.Locked && PidOf(w.LockReason) is null }).State == WtState.Safe;

    // Local branches created by `claude -w` (worktree-*) that no worktree has checked out.
    internal static IReadOnlyList<string> OrphanBranches(IEnumerable<string> heads, IEnumerable<string?> checkedOut)
    {
        var used = checkedOut.OfType<string>().ToHashSet(StringComparer.OrdinalIgnoreCase);
        return heads.Where(h => h.StartsWith("worktree-", StringComparison.Ordinal) && !used.Contains(h)).Order(StringComparer.Ordinal).ToList();
    }

    static WtFacts FactsOf(WorktreeInfo w) => new(w.Dirty, w.Unpushed, w.Merged, w.SquashMerged, w.UpstreamGone, w.Locked,
        PidOf(w.LockReason), false, w.State != WtState.Orphan, w.State == WtState.Orphan, null, w.Branch is null, Unreadable: w.Unreadable,
        OnRemote: w.Pushed, RecentMin: w.RecentMin, Kept: w.Kept);

    public IReadOnlyList<CleanupStep> Plan(IEnumerable<WorktreeInfo> rows, IReadOnlySet<string>? keptOk = null)
    {
        var steps = new List<CleanupStep>();
        foreach (var g in rows.Where(w => Cleanable(w, keptOk?.Contains(w.Path) == true)).GroupBy(w => w.Repo, StringComparer.OrdinalIgnoreCase))
        {
            var repo = g.Key;
            string Rel(string p) => p.StartsWith(repo + "/", StringComparison.OrdinalIgnoreCase) ? p[(repo.Length + 1)..] : p;
            CleanupStep Step(bool fatal, params string[] a) => new(repo, "git " + string.Join(' ', a.Select(x => x.StartsWith(repo) ? Rel(x) : x)), a, fatal);
            foreach (var w in g.Where(w => w.BranchOnly)) steps.Add(Step(false, "branch", "-d", w.Branch!));   // no folder: just the branch
            var wts = g.Where(w => !w.BranchOnly).ToList();
            foreach (var w in wts.Where(w => w.Locked && w.State == WtState.Orphan)) steps.Add(Step(true, "worktree", "unlock", w.Path));
            foreach (var w in wts.Where(w => w.State != WtState.Orphan))
            {
                if (w.Locked) steps.Add(Step(true, "worktree", "unlock", w.Path));
                steps.Add(Step(true, "worktree", "remove", w.Path));
                if (w.Branch is { } b && w.Merged) steps.Add(Step(false, "branch", "-d", b));   // squash or only pushed: the branch is kept
            }
            if (wts.Any(w => w.State == WtState.Orphan))
            {
                steps.Add(Step(true, "worktree", "prune", "-v"));
                foreach (var w in wts.Where(w => w.State == WtState.Orphan && w.Merged && w.Branch is not null)) steps.Add(Step(false, "branch", "-d", w.Branch!));
            }
        }
        // The acknowledgement travels with the steps, so the run re-validates the row under the same terms.
        return keptOk is null ? steps : [.. steps.Select(st => st.GitArgs is ["worktree", _, var p] && keptOk.Contains(p) ? st with { KeptOk = true } : st)];
    }

    // Runs a confirmed plan one command at a time. Each row is re-scanned right before its first command and skipped
    // (exit -1) if it is no longer cleanable; a failed fatal step skips the rest of that row.
    public async IAsyncEnumerable<(CleanupStep step, int exit, string output)> RunAsync(IReadOnlyList<CleanupStep> confirmedPlan,
        [System.Runtime.CompilerServices.EnumeratorCancellation] CancellationToken ct)
    {
        var skip = new HashSet<string>(StringComparer.OrdinalIgnoreCase);   // paths and branches of skipped rows
        var checkedPaths = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        WorktreeInfo? row = null;
        foreach (var step in confirmedPlan)
        {
            var a = step.GitArgs;
            if (a.Any(x => x is "--force" or "-f" or "-D" or "--delete-force")) { yield return (step, -1, Strings.Get("Wt.Run.Forbidden")); continue; }
            var path = a is ["worktree", "unlock" or "remove", var p] ? p : null;
            var target = path ?? (a is ["branch", "-d", var b] ? b : null);
            if (path is not null && checkedPaths.Add(path))
            {
                row = (await ScanAsync(step.Repo, ct, Norm(path))).FirstOrDefault(w => string.Equals(w.Path, path, StringComparison.OrdinalIgnoreCase));
                if (row is null || !Cleanable(row, step.KeptOk))
                {
                    skip.Add(path);
                    if (row?.Branch is { } rb) skip.Add(rb);
                    yield return (step, -1, row is null ? Strings.Get("Wt.Run.Missing") : Strings.Get("Wt.Run.Changed", Label(row.State)));
                    continue;
                }
            }
            if (target is not null && skip.Contains(target)) { yield return (step, -1, Strings.Get("Wt.Run.Skipped")); continue; }
            var (exit, o, e) = await Git(step.Repo, ct, a);
            var output = (o + e).Trim();
            if (exit != 0 && a is ["branch", "-d", ..]) output = Strings.Get("Wt.Run.BranchKept") + "\n" + output;
            if (exit != 0 && step.FailureIsFatal && path is not null)
            {
                skip.Add(path);
                if (row?.Branch is { } rb) skip.Add(rb);
            }
            yield return (step, exit, output);
        }
        foreach (var repo in confirmedPlan.Select(s => s.Repo).Distinct()) await ScanAsync(repo, ct);   // refresh CleanableCount
    }

    public async Task<int> PushAsync(WorktreeInfo w) =>
        w.Branch is null ? -1 : (await Git(w.Path, CancellationToken.None, "push", "-u", "origin", w.Branch)).Exit;

    // True when .claude/worktrees/ is ignored by git in this repo (otherwise `git add .` embeds the worktrees).
    public async Task<bool> IgnoresWorktreesAsync(string repo, CancellationToken ct) =>
        (await Git(repo, ct, "check-ignore", "-q", ".claude/worktrees/x")).Exit == 0;

    // How both pages name a worktree: its branch (what the work is), else its folder (a detached HEAD).
    public static string DisplayName(WorktreeInfo w) => w.Branch ?? w.Name;

    // A DOM id for a worktree row, stable across scans: the Overview links /worktrees?path=… to it.
    public static string Anchor(string path) => "wt-" + Convert.ToHexString(System.Security.Cryptography.SHA1.HashData(System.Text.Encoding.UTF8.GetBytes(path)))[..12].ToLowerInvariant();

    public static string Label(WtState s) => s switch
    {
        WtState.Safe => Strings.Get("Wt.State.Safe"), WtState.Orphan => Strings.Get("Wt.State.Orphan"), WtState.Active => Strings.Get("Wt.State.Active"), _ => Strings.Get("Wt.State.Check"),
    };

    // Report §3, first match wins. Why is HTML (<b>) with a {base} placeholder filled by ScanAsync.
    // A Safe verdict touched under an hour ago is held back: a fresh agent worktree is clean and "merged" while the agent works.
    public static (WtState State, string Why) Classify(WtFacts f)
    {
        var c = ClassifyCore(f);
        if (f.RecentMin is not { } m) return c;
        // A stale lock is cleanable once unlocked: when recency is what still holds it back, the reason must say recency.
        var held = c.State == WtState.Safe || c.State == WtState.Check && f.Locked && !f.PidAlive && ClassifyCore(f with { Locked = false }).State == WtState.Safe;
        return held ? (WtState.Check, Strings.Get("Wt.Why.Recent", Math.Max(1, m))) : c;
    }

    static (WtState State, string Why) ClassifyCore(WtFacts f)
    {
        if (f.ActiveSessionName is { } s) return (WtState.Active, Strings.Get("Wt.Why.Running", Enc(s)));
        if (f.InUseBy is { } u) return (WtState.Active, Strings.Get("Wt.Why.InUse", Enc(u)));
        if (f.Locked && f.LockPid is { } pid && f.PidAlive) return (WtState.Active, Strings.Get("Wt.Why.External", pid));
        if (f.Unreadable is { } err) return (WtState.Check, Strings.Get("Wt.Why.Unreadable", Enc(err)));
        if (f.NoWorktree && f.MergedPr is { } nb) return (WtState.Check, Strings.Get("Wt.Why.MergedPrBranch", nb));   // -d refuses a squashed branch: named, not offered
        if (f.NoWorktree && !f.Merged && f.SquashMerged) return (WtState.Check, Strings.Get("Wt.Why.SquashBranch"));   // the folder went, the content is in the base
        if (f.NoWorktree && f.TwinOf is { } t)
            return f.TwinActive ? (WtState.Active, Strings.Get("Wt.Why.TwinActive", Enc(t)))   // nothing to decide while its folder is in use
                : f.Merged ? (WtState.Orphan, Strings.Get("Wt.Why.TwinMerged", Enc(t)))
                : (WtState.Check, f.OnRemote is { } tr ? Strings.Get("Wt.Why.Pushed", Enc(tr)) : f.OnRemoteStale is { } ts ? Strings.Get("Wt.Why.Unconfirmed", Enc(ts)) : f.RemoteGone is { } tg ? Strings.Get("Wt.Why.RemoteGone", Enc(tg)) : Strings.Get("Wt.Why.BranchUnmerged"));
        if (f.NoWorktree) return f.Merged ? (WtState.Orphan, Strings.Get("Wt.Why.BranchMerged"))
            : (WtState.Check, f.OnRemote is { } br ? Strings.Get("Wt.Why.Pushed", Enc(br)) : f.OnRemoteStale is { } bs ? Strings.Get("Wt.Why.Unconfirmed", Enc(bs)) : f.RemoteGone is { } bg ? Strings.Get("Wt.Why.RemoteGone", Enc(bg)) : Strings.Get("Wt.Why.BranchUnmerged"));
        if (!f.Exists || f.Prunable) return (WtState.Orphan, Strings.Get("Wt.Why.Deleted"));
        // Dirty files and unpushed commits both say so: the commits are the worse loss (a stash does not save them), never dropped.
        if (f.Dirty > 0) return (WtState.Check,
            // first, so no clamp hides it: the leftover edits are the decision when the commits themselves are already safe
            (f.Merged ? Strings.Get("Wt.Why.DirtyMerged") : f.MergedPr is { } dp ? Strings.Get("Wt.Why.DirtyPr", dp) : f.SquashMerged ? Strings.Get("Wt.Why.DirtySquashed") : "")
            + (f.Ahead > 0 && !f.Merged && !f.SquashMerged ? Strings.Plural(f.Ahead, "Wt.Why.Ahead") + " · " : "") + Strings.Plural(f.Dirty, "Wt.Why.Dirty")
            + (f.DirtyNames is { } dn ? ": " + Enc(dn) : ""));
        if (f.Ahead > 0 && !f.Merged && !f.SquashMerged) return (WtState.Check, Strings.Plural(f.Ahead, "Wt.Why.Ahead"));
        if (f.Locked && f.LockPid is { } dead) return (WtState.Check, Strings.Get("Wt.Why.StaleLock", dead));
        if (f.Locked) return (WtState.Check, Strings.Get("Wt.Why.Locked"));
        if (f.Kept is { } k) return (WtState.Check, Strings.Get("Wt.Why.Kept", Enc(k)));   // removing the folder would delete them
        if (f.MergedPr is { } n) return (WtState.Safe, Strings.Get(f.Detached ? "Wt.Why.DetachedPr" : "Wt.Why.MergedPr", n));
        if (f.Detached && !f.Merged && f.SquashMerged) return (WtState.Safe, Strings.Get("Wt.Why.DetachedSquashed"));   // no branch to keep
        if (f.Detached && !f.Merged && !f.SquashMerged) return f.OnRemote is { } d ? (WtState.Safe, Strings.Get("Wt.Why.DetachedPushed", Enc(d)))
            : f.OnRemoteStale is { } ds ? (WtState.Check, Strings.Get("Wt.Why.Unconfirmed", Enc(ds)))
            : f.RemoteGone is { } dg ? (WtState.Check, Strings.Get("Wt.Why.DetachedGone", Enc(dg))) : (WtState.Check, Strings.Get("Wt.Why.Detached"));
        if (f.Merged && f.UpstreamGone) return (WtState.Safe, Strings.Get("Wt.Why.MergedGone"));
        if (f.Merged) return (WtState.Safe, Strings.Get("Wt.Why.Merged"));
        if (f.SquashMerged) return (WtState.Safe, Strings.Get("Wt.Why.Squashed"));
        if (f.OnRemote is { } o) return (WtState.Safe, Strings.Get("Wt.Why.Pushed", Enc(o)));   // the folder goes, the branch stays (Plan)
        if (f.OnRemoteStale is { } os) return (WtState.Check, Strings.Get("Wt.Why.Unconfirmed", Enc(os)));
        if (f.RemoteGone is { } og) return (WtState.Check, Strings.Get("Wt.Why.RemoteGone", Enc(og)));
        return (WtState.Check, Strings.Get("Wt.Why.NotMerged"));
    }

    static int? PidOf(string? lockReason) => lockReason is not null && Regex.Match(lockReason, @"pid (\d+)") is { Success: true } m ? int.Parse(m.Groups[1].Value) : null;

    static bool Alive(int pid)
    {
        try { using var p = Process.GetProcessById(pid); return !p.HasExited; }
        catch (Exception) { return false; }
    }

    static bool Inside(string cwd, string path)
    {
        var c = Norm(cwd);
        return string.Equals(c, path, StringComparison.OrdinalIgnoreCase) || c.StartsWith(path + "/", StringComparison.OrdinalIgnoreCase);
    }

    static string Norm(string p) => System.IO.Path.TrimEndingDirectorySeparator(System.IO.Path.GetFullPath(p)).Replace('\\', '/');
    static string Enc(string s) => HtmlEncoder.Default.Encode(s);

    static readonly SemaphoreSlim gate = new(Environment.ProcessorCount);   // caps concurrent git processes across every scan

    static Task<(int Exit, string Out, string Err)> Git(string dir, CancellationToken ct, params string[] args) => Git(dir, ct, (IEnumerable<string>)args);

    static async Task<(int Exit, string Out, string Err)> Git(string dir, CancellationToken ct, IEnumerable<string> args, string? input = null)
    {
        var psi = new ProcessStartInfo("git") { RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false, CreateNoWindow = true,
            StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8, RedirectStandardInput = input is not null };
        psi.ArgumentList.Add("-C");
        psi.ArgumentList.Add(dir);
        foreach (var a in args) psi.ArgumentList.Add(a);
        psi.Environment["GIT_OPTIONAL_LOCKS"] = "0";
        psi.Environment["GIT_TERMINAL_PROMPT"] = "0";
        psi.Environment["GIT_SSH_COMMAND"] = Environment.GetEnvironmentVariable("GIT_SSH_COMMAND") ?? "ssh -o BatchMode=yes -o ConnectTimeout=8";   // ls-remote never prompts
        await gate.WaitAsync(ct);
        try
        {
            Process p;
            try { p = Process.Start(psi)!; }
            catch (System.ComponentModel.Win32Exception ex) { throw new InvalidOperationException(Strings.Get("Wt.GitMissing", ex.Message), ex); }   // git not on PATH
            using var _ = p;
            var o = p.StandardOutput.ReadToEndAsync(ct);
            var e = p.StandardError.ReadToEndAsync(ct);
            if (input is not null) { await p.StandardInput.WriteAsync(input.AsMemory(), ct); p.StandardInput.Close(); }   // after the readers start: no pipe deadlock
            try { await p.WaitForExitAsync(ct); }
            catch (OperationCanceledException) { try { p.Kill(true); } catch (Exception) { } throw; }
            return (p.ExitCode, await o, await e);
        }
        finally { gate.Release(); }
    }

    internal static void Check()
    {
        static void Is(WtFacts f, WtState s, string what) => SelfCheck.Assert(Classify(f).State == s, $"Classify {what} : {Classify(f).State}, attendu {s}");
        Is(new(Merged: true, ActiveSessionName: "x"), WtState.Active, "active session in the app");
        Is(new(Locked: true, LockPid: 42, PidAlive: true, Merged: true), WtState.Active, "verrou pid vivant");
        Is(new(Exists: false, Merged: true), WtState.Orphan, "dossier absent");
        Is(new(Prunable: true), WtState.Orphan, "prunable");
        Is(new(Dirty: 3, Merged: true), WtState.Check, "sale");
        Is(new(Ahead: 4), WtState.Check, "unpushed, unmerged");
        Is(new(Locked: true, LockPid: 42, Merged: true), WtState.Check, "verrou pid mort");
        Is(new(Merged: true), WtState.Safe, "clean merged");
        Is(new(Ahead: 2, SquashMerged: true), WtState.Safe, "squash");
        Is(new(Merged: true, UpstreamGone: true), WtState.Safe, "remote branch deleted");
        Is(new(Merged: true, Ahead: 0), WtState.Safe, "frais");
        Is(new(Detached: true), WtState.Check, "detached outside base");
        Is(new(), WtState.Check, "inconnu");
        Is(new(NoWorktree: true, Merged: true), WtState.Orphan, "merged branch without worktree");
        Is(new(NoWorktree: true, Ahead: 2), WtState.Check, "unmerged branch without worktree");
        Is(new(NoWorktree: true, Ahead: 2, SquashMerged: true), WtState.Check, "squash-merged branch without worktree");
        Is(new(Merged: true, Unreadable: "fatal: detected dubious ownership"), WtState.Check, "git failed on a merged worktree");
        Is(new(Detached: true, OnRemote: "origin/x"), WtState.Safe, "detached, commits on a remote branch");
        Is(new(OnRemote: "origin/x"), WtState.Safe, "clean branch, every commit pushed");
        Is(new(OnRemote: "origin/x", Dirty: 1), WtState.Check, "pushed but dirty");
        Is(new(Merged: true, RecentMin: 5), WtState.Check, "fresh agent worktree: clean, merged, touched 5 min ago");
        Is(new(Merged: true, InUseBy: "zsh (pid 9)"), WtState.Active, "a shell sits inside");
        Is(new(Ahead: 3, RecentMin: 5), WtState.Check, "recent and unpushed stays Check");
        Is(new(NoWorktree: true, Merged: true, TwinOf: "agent-a"), WtState.Orphan, "branch of a folder that went detached");
        Is(new(NoWorktree: true, Merged: true, TwinOf: "agent-a", TwinActive: true), WtState.Active, "branch of a folder a session uses");
        Is(new(NoWorktree: true, OnRemote: "origin/x"), WtState.Check, "unmerged branch without worktree, pushed");
        Is(new(Merged: true, Kept: ".env"), WtState.Check, "merged but holds an ignored .env");
        Is(new(Detached: true, OnRemoteStale: "origin/x"), WtState.Check, "detached on a ref origin could not confirm");
        {
            var kept = new WorktreeInfo("C:/r", "C:/r/k", "k", "bk", "h", WtState.Check, "", null, default, false, null, 0, 0, true, false, false, Kept: ".env");
            SelfCheck.Assert(!Cleanable(kept) && Cleanable(kept, keptOk: true), "Cleanable: an ignored .env needs the explicit Delete anyway");
            SelfCheck.Assert(!Cleanable(kept with { Dirty = 1 }, keptOk: true), "Cleanable: Delete anyway never covers uncommitted work");
        }
        Is(new(OnRemoteStale: "origin/x"), WtState.Check, "clean branch on an unconfirmed ref");
        SelfCheck.Assert(Classify(new(Detached: true, RemoteGone: "origin/x")).Why == Strings.Get("Wt.Why.DetachedGone", "origin/x"), "detached on a deleted branch names it");
        Is(new(RemoteGone: "origin/x"), WtState.Check, "clean branch whose remote branch origin deleted");
        Is(new(RemoteGone: "origin/x", MergedPr: 52), WtState.Safe, "deleted remote branch, but GitHub merged its PR");
        Is(new(Detached: true, MergedPr: 718), WtState.Safe, "detached HEAD that is a merged PR's head");
        Is(new(MergedPr: 52, Dirty: 1), WtState.Check, "merged PR but uncommitted files");
        Is(new(NoWorktree: true, MergedPr: 52), WtState.Check, "branch-only row of a merged PR: -d would refuse");
        SelfCheck.Assert(Classify(new(NoWorktree: true, SquashMerged: true)).Why == Strings.Get("Wt.Why.SquashBranch"), "a squash-merged branch with no folder says so");
        SelfCheck.Assert(NamesOf(["src/a.cs", "b.razor", "c/d.js"]) == "a.cs, b.razor +1" && NamesOf(["old -> new.cs"]) == "new.cs", "NamesOf");
        Is(new(Detached: true, SquashMerged: true), WtState.Safe, "detached HEAD whose content is squash-merged");
        SelfCheck.Assert(Classify(new(Merged: true, Locked: true, LockPid: 1, RecentMin: 4)).Why == Strings.Get("Wt.Why.Recent", 4), "a stale lock held back by recency says recency");
        Is(new(Detached: true, OnRemote: "origin/x", Kept: "a.jsonl"), WtState.Check, "pushed detached HEAD with an ignored log");
        SelfCheck.Assert(string.Join(',', KeptOf("bin/\nnode_modules/\n.env\nsrc/obj/\n.DS_Store\ndesktop/src-tauri/gen/\n.claude/decision-events.jsonl\nx.cache.json\n")) == ".env,.claude/decision-events.jsonl",
            "KeptOf drops build output, keeps the rest");
        SelfCheck.Assert(ParseLsof("p1\nczsh\nn/a/b\np2\ncme\nn/self\np3\ncx\nn ignored\np4\nclsof\nn/a/b\n", 2) is [(1, "zsh", "/a/b")], "ParseLsof skips self, its own probes and non-paths");
        Is(new(NoWorktree: true, Merged: true, Unreadable: "x"), WtState.Check, "git failed on a merged branch");
        SelfCheck.Assert(!Cleanable(new("C:/r", "C:/r/u", "u", "bu", "h", WtState.Check, "", null, default, true, "claude session u (pid 7)", 0, 0, true, false, false, Unreadable: "x")),
            "Cleanable: an unreadable stale-lock row never gets an unlock + remove");
        SelfCheck.Assert(string.Join(',', OrphanBranches(["main", "worktree-a", "worktree-b", "feat/x"], ["main", "worktree-b", null])) == "worktree-a", "OrphanBranches");

        WorktreeInfo Row(string name, WtFacts f, string? branch = "b") => new("C:/r", "C:/r/.claude/worktrees/" + name, name, branch is null ? null : branch + name, "h",
            Classify(f).State, "", null, default, f.Locked, f.Locked ? $"claude session {name} (pid {f.LockPid})" : null, f.Dirty, f.Ahead, f.Merged, f.SquashMerged, f.UpstreamGone);
        var rows = new[]
        {
            Row("safe", new(Merged: true)), Row("squash", new(Ahead: 2, SquashMerged: true)), Row("gone", new(Merged: true, UpstreamGone: true)),
            Row("stale", new(Locked: true, LockPid: 7, Merged: true)), Row("orph", new(Exists: false, Merged: true, Locked: true, LockPid: 8)),
            Row("dirty", new(Dirty: 1, Merged: true)), Row("ahead", new(Ahead: 1)), Row("live", new(Locked: true, LockPid: 9, PidAlive: true, Merged: true)),
            Row("act", new(ActiveSessionName: "s")), Row("det", new(Detached: true), null), Row("detm", new(Detached: true, Merged: true), null), Row("unk", new()),
            new WorktreeInfo("C:/r", "C:/r/refs/heads/worktree-gone", "gone2", "worktree-gone", null, WtState.Orphan, "", null, default, false, null, 0, 0, true, false, false, BranchOnly: true),
        };
        var plan = new WorktreeService(null!).Plan(rows);
        var all = string.Join('\n', plan.Select(s => string.Join(' ', s.GitArgs)));
        SelfCheck.Assert(plan.All(s => !s.GitArgs.Any(a => a is "--force" or "-f" or "-D" || a.StartsWith("--force"))), "Plan without --force / -f / -D");
        SelfCheck.Assert(!plan.Any(s => s.GitArgs[^1].EndsWith("/dirty") || s.GitArgs[^1].EndsWith("/ahead") || s.GitArgs[^1].EndsWith("/live")
            || s.GitArgs[^1].EndsWith("/act") || s.GitArgs[^1].EndsWith("/det") || s.GitArgs[^1].EndsWith("/unk")), "Plan: only Safe, Orphan and stale lock");
        SelfCheck.Assert(all.Contains("worktree unlock C:/r/.claude/worktrees/stale") && all.Contains("worktree remove C:/r/.claude/worktrees/stale"), "Plan stale lock: unlock then remove");
        SelfCheck.Assert(all.Contains("worktree remove C:/r/.claude/worktrees/squash") && !all.Contains("branch -d bsquash"), "Plan squash: branch kept");
        SelfCheck.Assert(all.Contains("branch -d bsafe") && all.Contains("worktree prune") && all.Contains("worktree unlock C:/r/.claude/worktrees/orph"), "Plan : branch -d, prune, unlock orphelin");
        SelfCheck.Assert(all.Contains("branch -d worktree-gone") && !all.Contains("remove C:/r/refs/heads/worktree-gone") && !all.Contains("unlock C:/r/refs"), "Plan: branch without worktree = a single branch -d");
        SelfCheck.Assert(all.Contains("worktree remove C:/r/.claude/worktrees/detm"), "Plan: merged detached removed");
        SelfCheck.Assert(plan.First(s => s.GitArgs[0] == "worktree" && s.GitArgs[1] == "remove").Display == "git worktree remove .claude/worktrees/safe", "Plan: relative path displayed");
        var svc = new WorktreeService(null!); var fired = 0; svc.Changed += () => fired++;
        svc.SetCleanable("C:/r", 2);
        SelfCheck.Assert(fired == 1 && svc.CleanableCount == 2, "CleanableCount: Changed on first value");
        svc.SetCleanable("C:/r", 2);
        SelfCheck.Assert(fired == 1, "CleanableCount: no Changed if unchanged");
        svc.SetCleanable("C:/r", 0);
        SelfCheck.Assert(fired == 2 && svc.CleanableCount == 0, "CleanableCount: Changed when the count drops");

        var tmp = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "cc-ui-wt-" + Guid.NewGuid().ToString("N"));
        try
        {
            var repo = System.IO.Path.Combine(tmp, "repo"); Directory.CreateDirectory(repo);
            static void G(string dir, params string[] a)
            {
                var psi = new ProcessStartInfo("git") { WorkingDirectory = dir, RedirectStandardOutput = true, RedirectStandardError = true };
                foreach (var x in a) psi.ArgumentList.Add(x);
                using var p = Process.Start(psi)!; p.StandardOutput.ReadToEnd(); p.StandardError.ReadToEnd(); p.WaitForExit();
                SelfCheck.Assert(p.ExitCode == 0, "git " + string.Join(' ', a));
            }
            G(repo, "init", "-q"); G(repo, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "i");
            G(repo, "worktree", "add", "-q", ".claude/worktrees/demo", "-b", "demo");
            var wt = System.IO.Path.Combine(repo, ".claude", "worktrees", "demo");
            var file = System.IO.Path.Combine(tmp, "repos.json");
            var sm = new SessionManager(); sm.Open(new PastSession(Guid.NewGuid().ToString(), wt, "demo", "t", null, DateTimeOffset.Now, null));
            WorktreeService Svc(SessionManager m) => new(m) { ReposFile = file, repos = [] };
            IReadOnlyList<string> Disc(WorktreeService w) => w.DiscoverReposAsync(CancellationToken.None).GetAwaiter().GetResult();
            var root = System.IO.Path.GetFileName(tmp);   // macOS temp is a symlink: match on the unique folder name
            G(repo, "branch", "worktree-foo");
            G(repo, "branch", "feat-x");
            G(repo, "switch", "-q", "-c", "worktree-bar"); G(repo, "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "--allow-empty", "-m", "b"); G(repo, "switch", "-q", "-");
            var scan = Svc(new SessionManager()).ScanAsync(repo, CancellationToken.None).GetAwaiter().GetResult().Where(w => w.BranchOnly).ToList();
            SelfCheck.Assert(scan.Count == 2 && scan.Any(w => w.Branch == "worktree-foo" && w.State == WtState.Orphan) && scan.Any(w => w.Branch == "worktree-bar" && w.State == WtState.Check),
                "Scan: worktree-* branches without worktree (foo orphan, bar to check, feat-x ignored)");
            SelfCheck.Assert(scan.All(w => w.Name == w.Branch), "Scan: a branch without worktree is named by its full branch");
            var demo = Svc(new SessionManager()).ScanAsync(repo, CancellationToken.None).GetAwaiter().GetResult().Single(w => !w.BranchOnly).Path;
            var one = Svc(new SessionManager()).ScanAsync(repo, CancellationToken.None, demo).GetAwaiter().GetResult();
            SelfCheck.Assert(one.Count == 1 && one[0].Path == demo, "Scan only: the one worktree, no branch rows");
            G(repo, "worktree", "remove", ".claude/worktrees/demo");
            SelfCheck.Assert(Disc(Svc(sm)).Any(r => r.Contains(root)), "Discover: repo of a -w session whose worktree was deleted");
            SelfCheck.Assert(Disc(Svc(new SessionManager())).Any(r => r.Contains(root)), "Discover: repo remembered after restart");
            foreach (var f in new DirectoryInfo(repo).EnumerateFiles("*", SearchOption.AllDirectories)) f.Attributes = FileAttributes.Normal;   // object files are read-only: Windows refuses to delete them
            Directory.Delete(repo, true);
            SelfCheck.Assert(!Disc(Svc(new SessionManager())).Any(r => r.Contains(root)) && !File.ReadAllText(file).Contains("cc-ui-wt-"), "Discover: vanished repo forgotten");
            File.WriteAllText(file, "not json");
            SelfCheck.Assert(Disc(Svc(new SessionManager())).Count == 0, "Discover: corrupt repos.json ignored");
            var plain = System.IO.Path.Combine(tmp, "plain"); Directory.CreateDirectory(plain);   // exists, but git can't resolve a root there
            File.WriteAllText(file, JsonSerializer.Serialize(new[] { Norm(plain) }));
            Disc(Svc(new SessionManager()));
            SelfCheck.Assert(File.ReadAllText(file).Contains("plain"), "Discover: existing folder without a git root stays remembered");
            var gone = System.IO.Path.Combine(tmp, "gone");   // never created; a live session still points at it (trailing slash)
            File.WriteAllText(file, JsonSerializer.Serialize(new[] { Norm(gone) }));
            var smGone = new SessionManager(); smGone.Open(new PastSession(Guid.NewGuid().ToString(), gone + "/", "g", "t", null, DateTimeOffset.Now, null));
            Disc(Svc(smGone));
            SelfCheck.Assert(File.ReadAllText(file).Contains("gone"), "Discover: live session cwd compared normalized");
        }
        finally { try { Directory.Delete(tmp, true); } catch (Exception) { } }
    }
}
