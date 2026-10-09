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
    bool Locked, string? LockReason, int Dirty, int Unpushed, bool Merged, bool SquashMerged, bool UpstreamGone);

public sealed record CleanupStep(string Repo, string Display, string[] GitArgs, bool FailureIsFatal);

// Facts collected by git for one worktree; Classify turns them into a state.
public sealed record WtFacts(int Dirty = 0, int Ahead = 0, bool Merged = false, bool SquashMerged = false, bool UpstreamGone = false,
    bool Locked = false, int? LockPid = null, bool PidAlive = false, bool Exists = true, bool Prunable = false,
    string? ActiveSessionName = null, bool Detached = false);

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

    // Repo roots: cwds of the live sessions, plus the newest transcript folders of the last 30 days (not under %TEMP%).
    // Cached; refresh = true rescans.
    public async Task<IReadOnlyList<string>> DiscoverReposAsync(CancellationToken ct) => await DiscoverReposAsync(false, ct);

    // Roots resolved from live sessions are remembered here (and in ReposFile) until the repo vanishes from disk.
    internal string ReposFile = System.IO.Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ClaudeCodeUI", "repos.json");
    readonly object rememberedGate = new();
    HashSet<string>? remembered;

    public async Task<IReadOnlyList<string>> DiscoverReposAsync(bool refresh, CancellationToken ct)
    {
        var live = Sessions.All.Select(s => StripWorktree(s.Cwd)).ToHashSet(StringComparer.OrdinalIgnoreCase);
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
            if (live.Contains(cwd)) liveRoots.TryAdd(r, 0);
        });
        lock (rememberedGate)
        {
            var before = remembered!.ToArray();
            remembered.UnionWith(liveRoots.Keys.Select(Norm));
            foreach (var d in dead.Keys) if (!live.Contains(d)) remembered.Remove(d);
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
        var dir = System.IO.Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".claude", "projects");
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
    public async Task<IReadOnlyList<WorktreeInfo>> ScanAsync(string repoRoot, CancellationToken ct)
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
        var merged = (await Git(repoRoot, ct, ["for-each-ref", "refs/heads", "--merged", bas, .. hasRemoteBase ? ["--merged", "origin/" + bas] : Array.Empty<string>(), "--format=%(refname:short)"]))
            .Out.Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries).ToHashSet();
        var refs = (await Git(repoRoot, ct, "for-each-ref", "refs/heads", "--format=%(refname:short)%00%(upstream:short)%00%(upstream:track,nobracket)%00%(committerdate:iso-strict)"))
            .Out.Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(l => l.TrimEnd('\r').Split('\0')).Where(p => p.Length == 4).ToDictionary(p => p[0]);
        var live = Sessions.All.Where(s => s.HasProcess).ToList();

        var rows = await Task.WhenAll(records.Skip(1).Select(async r =>
        {
            var path = Norm(r["worktree"]);
            var branch = r.TryGetValue("branch", out var b) ? b.Replace("refs/heads/", "") : null;
            var exists = Directory.Exists(path);
            var dirty = exists ? (await Git(path, ct, "status", "--porcelain=v1", "-uall")).Out.Split('\n', StringSplitOptions.RemoveEmptyEntries).Length : 0;
            refs.TryGetValue(branch ?? "", out var rf);
            var track = rf?[2] ?? "";
            var ahead = Regex.Match(track, @"ahead (\d+)") is { Success: true } a ? int.Parse(a.Groups[1].Value)
                : branch is not null && string.IsNullOrEmpty(rf?[1]) ? int.Parse((await Git(repoRoot, ct, "rev-list", "--count", branch, "--not", "--remotes")).Out.Trim() is { Length: > 0 } n ? n : "0")
                : 0;
            var isMerged = branch is not null ? merged.Contains(branch)
                : r.TryGetValue("HEAD", out var h) && (await Git(repoRoot, ct, "merge-base", "--is-ancestor", h, bas)).Exit == 0;
            var squash = false;
            if (!isMerged && branch is not null)
            {
                var mt = await Git(repoRoot, ct, "merge-tree", "--write-tree", bas, branch);
                squash = mt.Exit == 0 && mt.Out.Split('\n')[0].Trim() == baseTree;
            }
            r.TryGetValue("locked", out var reason);
            var pid = PidOf(reason);
            var session = live.FirstOrDefault(s => Inside(s.Cwd, path));
            var facts = new WtFacts(dirty, ahead, isMerged, squash, track == "gone", reason is not null, pid, pid is { } p && Alive(p),
                exists, r.ContainsKey("prunable"), session?.Name, branch is null);
            var (state, why) = Classify(facts);
            if (session?.Status == SessionStatus.Waiting) why = $"Session <b>{Enc(session.Name)}</b> attend une décision";
            var last = new[] { Activity(path), DateTimeOffset.TryParse(rf?[3], out var cd) ? cd : DateTimeOffset.MinValue }.Max();
            var idle = (int)(DateTimeOffset.Now - last).TotalDays;
            if (state is WtState.Safe or WtState.Check && idle >= 2 && last > DateTimeOffset.MinValue) why += $", inactif depuis {idle} j";
            return new WorktreeInfo(Norm(repoRoot), path, System.IO.Path.GetFileName(path), branch, r.GetValueOrDefault("HEAD"),
                state, why.Replace("{base}", Enc(bas)), null, last, reason is not null, reason, dirty, ahead, isMerged, squash, track == "gone");
        }));
        SetCleanable(Norm(repoRoot), rows.Count(w => w.State is WtState.Safe or WtState.Orphan));
        return rows;
    }

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
    public static bool Cleanable(WorktreeInfo w) => w.State is WtState.Safe or WtState.Orphan
        || w.State == WtState.Check && w.Locked && PidOf(w.LockReason) is not null && Classify(FactsOf(w) with { Locked = false }).State == WtState.Safe;

    static WtFacts FactsOf(WorktreeInfo w) => new(w.Dirty, w.Unpushed, w.Merged, w.SquashMerged, w.UpstreamGone, w.Locked,
        PidOf(w.LockReason), false, w.State != WtState.Orphan, w.State == WtState.Orphan, null, w.Branch is null);

    public IReadOnlyList<CleanupStep> Plan(IEnumerable<WorktreeInfo> rows)
    {
        var steps = new List<CleanupStep>();
        foreach (var g in rows.Where(Cleanable).GroupBy(w => w.Repo, StringComparer.OrdinalIgnoreCase))
        {
            var repo = g.Key;
            string Rel(string p) => p.StartsWith(repo + "/", StringComparison.OrdinalIgnoreCase) ? p[(repo.Length + 1)..] : p;
            CleanupStep Step(bool fatal, params string[] a) => new(repo, "git " + string.Join(' ', a.Select(x => x.StartsWith(repo) ? Rel(x) : x)), a, fatal);
            foreach (var w in g.Where(w => w.Locked && w.State == WtState.Orphan)) steps.Add(Step(true, "worktree", "unlock", w.Path));
            foreach (var w in g.Where(w => w.State != WtState.Orphan))
            {
                if (w.Locked) steps.Add(Step(true, "worktree", "unlock", w.Path));
                steps.Add(Step(true, "worktree", "remove", w.Path));
                if (w.Branch is { } b && !(w.SquashMerged && !w.Merged)) steps.Add(Step(false, "branch", "-d", b));   // squash: -d would refuse, branch kept
            }
            if (g.Any(w => w.State == WtState.Orphan))
            {
                steps.Add(Step(true, "worktree", "prune", "-v"));
                foreach (var w in g.Where(w => w.State == WtState.Orphan && w.Merged && w.Branch is not null)) steps.Add(Step(false, "branch", "-d", w.Branch!));
            }
        }
        return steps;
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
            if (a.Any(x => x is "--force" or "-f" or "-D" or "--delete-force")) { yield return (step, -1, "refusé : option interdite"); continue; }
            var path = a is ["worktree", "unlock" or "remove", var p] ? p : null;
            var target = path ?? (a is ["branch", "-d", var b] ? b : null);
            if (path is not null && checkedPaths.Add(path))
            {
                row = (await ScanAsync(step.Repo, ct)).FirstOrDefault(w => string.Equals(w.Path, path, StringComparison.OrdinalIgnoreCase));
                if (row is null || !Cleanable(row))
                {
                    skip.Add(path);
                    if (row?.Branch is { } rb) skip.Add(rb);
                    yield return (step, -1, row is null ? "ignoré : worktree introuvable" : $"ignoré : l'état a changé ({Label(row.State)})");
                    continue;
                }
            }
            if (target is not null && skip.Contains(target)) { yield return (step, -1, "ignoré"); continue; }
            var (exit, o, e) = await Git(step.Repo, ct, a);
            var output = (o + e).Trim();
            if (exit != 0 && a is ["branch", "-d", ..]) output = "branche gardée (git a refusé)\n" + output;
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

    public static string Label(WtState s) => s switch
    {
        WtState.Safe => "Sûr", WtState.Orphan => "Orphelin", WtState.Active => "Actif", _ => "À vérifier",
    };

    // Report §3, first match wins. Why is HTML (<b>) with a {base} placeholder filled by ScanAsync.
    public static (WtState State, string Why) Classify(WtFacts f)
    {
        if (f.ActiveSessionName is { } s) return (WtState.Active, $"Session <b>{Enc(s)}</b> en cours");
        if (f.Locked && f.LockPid is { } pid && f.PidAlive) return (WtState.Active, $"Session claude externe (pid {pid})");
        if (!f.Exists || f.Prunable) return (WtState.Orphan, "Dossier supprimé à la main, git le référence encore");
        if (f.Dirty > 0) return (WtState.Check, f.Dirty == 1 ? "<b>1 fichier</b> modifié non commité" : $"<b>{f.Dirty} fichiers</b> modifiés non commités");
        if (f.Ahead > 0 && !f.Merged && !f.SquashMerged) return (WtState.Check, f.Ahead == 1 ? "<b>1 commit</b> jamais poussé" : $"<b>{f.Ahead} commits</b> jamais poussés");
        if (f.Locked && f.LockPid is { } dead) return (WtState.Check, $"Verrou périmé (pid {dead}), sinon sûr");
        if (f.Locked) return (WtState.Check, "Verrouillé sans pid connu");
        if (f.Detached && !f.Merged) return (WtState.Check, "HEAD détachée, commits hors branche");
        if (f.Merged && f.UpstreamGone) return (WtState.Safe, "Mergé dans <b>{base}</b>, branche distante supprimée");
        if (f.Merged) return (WtState.Safe, "Déjà dans <b>{base}</b>, aucun changement local");
        if (f.SquashMerged) return (WtState.Safe, "Contenu déjà dans <b>{base}</b> (squash), branche gardée");
        return (WtState.Check, "Pas encore dans <b>{base}</b>, rien à pousser");
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

    static Task<(int Exit, string Out, string Err)> Git(string dir, CancellationToken ct, params string[] args) => Git(dir, ct, (IEnumerable<string>)args);

    static async Task<(int Exit, string Out, string Err)> Git(string dir, CancellationToken ct, IEnumerable<string> args)
    {
        var psi = new ProcessStartInfo("git") { RedirectStandardOutput = true, RedirectStandardError = true, UseShellExecute = false, CreateNoWindow = true,
            StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8 };
        psi.ArgumentList.Add("-C");
        psi.ArgumentList.Add(dir);
        foreach (var a in args) psi.ArgumentList.Add(a);
        psi.Environment["GIT_OPTIONAL_LOCKS"] = "0";
        psi.Environment["GIT_TERMINAL_PROMPT"] = "0";
        using var p = Process.Start(psi)!;
        var o = p.StandardOutput.ReadToEndAsync(ct);
        var e = p.StandardError.ReadToEndAsync(ct);
        try { await p.WaitForExitAsync(ct); }
        catch (OperationCanceledException) { try { p.Kill(true); } catch (Exception) { } throw; }
        return (p.ExitCode, await o, await e);
    }

    internal static void Check()
    {
        static void Is(WtFacts f, WtState s, string what) => SelfCheck.Assert(Classify(f).State == s, $"Classify {what} : {Classify(f).State}, attendu {s}");
        Is(new(Merged: true, ActiveSessionName: "x"), WtState.Active, "session active dans l'app");
        Is(new(Locked: true, LockPid: 42, PidAlive: true, Merged: true), WtState.Active, "verrou pid vivant");
        Is(new(Exists: false, Merged: true), WtState.Orphan, "dossier absent");
        Is(new(Prunable: true), WtState.Orphan, "prunable");
        Is(new(Dirty: 3, Merged: true), WtState.Check, "sale");
        Is(new(Ahead: 4), WtState.Check, "non poussé non mergé");
        Is(new(Locked: true, LockPid: 42, Merged: true), WtState.Check, "verrou pid mort");
        Is(new(Merged: true), WtState.Safe, "mergé propre");
        Is(new(Ahead: 2, SquashMerged: true), WtState.Safe, "squash");
        Is(new(Merged: true, UpstreamGone: true), WtState.Safe, "branche distante supprimée");
        Is(new(Merged: true, Ahead: 0), WtState.Safe, "frais");
        Is(new(Detached: true), WtState.Check, "détachée hors base");
        Is(new(), WtState.Check, "inconnu");

        WorktreeInfo Row(string name, WtFacts f, string? branch = "b") => new("C:/r", "C:/r/.claude/worktrees/" + name, name, branch is null ? null : branch + name, "h",
            Classify(f).State, "", null, default, f.Locked, f.Locked ? $"claude session {name} (pid {f.LockPid})" : null, f.Dirty, f.Ahead, f.Merged, f.SquashMerged, f.UpstreamGone);
        var rows = new[]
        {
            Row("safe", new(Merged: true)), Row("squash", new(Ahead: 2, SquashMerged: true)), Row("gone", new(Merged: true, UpstreamGone: true)),
            Row("stale", new(Locked: true, LockPid: 7, Merged: true)), Row("orph", new(Exists: false, Merged: true, Locked: true, LockPid: 8)),
            Row("dirty", new(Dirty: 1, Merged: true)), Row("ahead", new(Ahead: 1)), Row("live", new(Locked: true, LockPid: 9, PidAlive: true, Merged: true)),
            Row("act", new(ActiveSessionName: "s")), Row("det", new(Detached: true), null), Row("detm", new(Detached: true, Merged: true), null), Row("unk", new()),
        };
        var plan = new WorktreeService(null!).Plan(rows);
        var all = string.Join('\n', plan.Select(s => string.Join(' ', s.GitArgs)));
        SelfCheck.Assert(plan.All(s => !s.GitArgs.Any(a => a is "--force" or "-f" or "-D" || a.StartsWith("--force"))), "Plan sans --force / -f / -D");
        SelfCheck.Assert(!plan.Any(s => s.GitArgs[^1].EndsWith("/dirty") || s.GitArgs[^1].EndsWith("/ahead") || s.GitArgs[^1].EndsWith("/live")
            || s.GitArgs[^1].EndsWith("/act") || s.GitArgs[^1].EndsWith("/det") || s.GitArgs[^1].EndsWith("/unk")), "Plan : seulement Sûr, Orphelin et verrou périmé");
        SelfCheck.Assert(all.Contains("worktree unlock C:/r/.claude/worktrees/stale") && all.Contains("worktree remove C:/r/.claude/worktrees/stale"), "Plan verrou périmé : unlock puis remove");
        SelfCheck.Assert(all.Contains("worktree remove C:/r/.claude/worktrees/squash") && !all.Contains("branch -d bsquash"), "Plan squash : branche gardée");
        SelfCheck.Assert(all.Contains("branch -d bsafe") && all.Contains("worktree prune") && all.Contains("worktree unlock C:/r/.claude/worktrees/orph"), "Plan : branch -d, prune, unlock orphelin");
        SelfCheck.Assert(all.Contains("worktree remove C:/r/.claude/worktrees/detm"), "Plan : détachée mergée supprimée");
        SelfCheck.Assert(plan.First(s => s.GitArgs[0] == "worktree" && s.GitArgs[1] == "remove").Display == "git worktree remove .claude/worktrees/safe", "Plan : chemin relatif affiché");
        var svc = new WorktreeService(null!); var fired = 0; svc.Changed += () => fired++;
        svc.SetCleanable("C:/r", 2);
        SelfCheck.Assert(fired == 1 && svc.CleanableCount == 2, "CleanableCount : Changed à la première valeur");
        svc.SetCleanable("C:/r", 2);
        SelfCheck.Assert(fired == 1, "CleanableCount : pas de Changed si inchangé");
        svc.SetCleanable("C:/r", 0);
        SelfCheck.Assert(fired == 2 && svc.CleanableCount == 0, "CleanableCount : Changed quand le compte baisse");

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
            G(repo, "worktree", "remove", ".claude/worktrees/demo");
            SelfCheck.Assert(Disc(Svc(sm)).Any(r => r.Contains(root)), "Discover : repo d'une session -w dont le worktree a été supprimé");
            SelfCheck.Assert(Disc(Svc(new SessionManager())).Any(r => r.Contains(root)), "Discover : dépôt mémorisé après redémarrage");
            Directory.Delete(repo, true);
            SelfCheck.Assert(!Disc(Svc(new SessionManager())).Any(r => r.Contains(root)) && !File.ReadAllText(file).Contains("cc-ui-wt-"), "Discover : dépôt disparu oublié");
            File.WriteAllText(file, "not json");
            SelfCheck.Assert(Disc(Svc(new SessionManager())).Count == 0, "Discover : repos.json corrompu ignoré");
        }
        finally { try { Directory.Delete(tmp, true); } catch (Exception) { } }
    }
}
