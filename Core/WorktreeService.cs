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

// Scan, classify and safely clean the worktrees of known repos. Never --force, never -D. Body: WP4.
public sealed class WorktreeService(SessionManager sessions)
{
    internal SessionManager Sessions { get; } = sessions;

    public int? CleanableCount { get; private set; }   // Safe + Orphan of the last scan; null before the first

    public Task<IReadOnlyList<string>> DiscoverReposAsync(CancellationToken ct) => Task.FromResult<IReadOnlyList<string>>([]);

    public Task<IReadOnlyList<WorktreeInfo>> ScanAsync(string repoRoot, CancellationToken ct) => Task.FromResult<IReadOnlyList<WorktreeInfo>>([]);

    public Task<long> SizeAsync(string path, CancellationToken ct) => Task.FromResult(0L);

    public IReadOnlyList<CleanupStep> Plan(IEnumerable<WorktreeInfo> rows) => [];

    public async IAsyncEnumerable<(CleanupStep step, int exit, string output)> RunAsync(IReadOnlyList<CleanupStep> confirmedPlan,
        [System.Runtime.CompilerServices.EnumeratorCancellation] CancellationToken ct)
    {
        await Task.CompletedTask;
        yield break;
    }

    public Task<int> PushAsync(WorktreeInfo w) => Task.FromResult(0);

    public static (WtState State, string Why) Classify(WtFacts f) => (WtState.Check, "état inconnu");

    internal static void Check() { }
}
