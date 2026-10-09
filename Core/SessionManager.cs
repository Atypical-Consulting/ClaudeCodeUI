using System.Collections.Immutable;

namespace ClaudeCodeUI;

// All live sessions of the app (singleton). Kills every claude process on shutdown.
public sealed class SessionManager : IAsyncDisposable
{
    ImmutableList<LiveSession> all = [];
    readonly Lock openGate = new();

    public IReadOnlyList<LiveSession> All => all;   // creation order, never re-sorted
    public LiveSession? Get(string id) => all.FirstOrDefault(s => s.Id == id);

    // Every pending permission of every session, in arrival order.
    public IEnumerable<(LiveSession S, PendingPermission P)> DecisionQueue =>
        all.SelectMany(s => s.Pending.Select(p => (s, p))).OrderBy(x => x.p.At);

    // The latest quota received, all sessions included.
    public RateLimitEvt? Limits => all.Where(s => s.Limits is not null).MaxBy(s => s.LimitsAt)?.Limits;

    public event Action? Changed;

    public LiveSession Start(string cwd, string mode, string name, string? worktree, string? model, string? effort)
    {
        var s = Add(new LiveSession(Guid.NewGuid().ToString(), name, cwd, mode, worktree, model, effort));
        try { s.EnsureProcess(); } catch (Exception) { }   // a failed start leaves the session Crashed with the reason
        _ = s.RefreshGit();
        return s;
    }

    // History loaded now; the --resume process starts on the first Send or Restart.
    // Locked: two circuits opening the same past session at once would add it twice (duplicate @key in the Rail).
    public LiveSession Open(PastSession p)
    {
        lock (openGate)
        {
            if (Get(p.Id) is { } live) return live;
            var s = new LiveSession(p.Id, p.Title, p.Cwd, "default", resumable: true) { Items = [.. TranscriptStore.Load(p.Id)], CostUsd = p.CostUsd ?? 0 };
            s.ToolCount = s.Items.OfType<ToolItem>().Count();
            _ = s.RefreshGit();
            return Add(s);
        }
    }

    public async Task Stop(string id)
    {
        if (Get(id) is { } s) await s.DisposeAsync();
    }

    LiveSession Add(LiveSession s)
    {
        ImmutableInterlocked.Update(ref all, l => l.Add(s));
        s.Changed += () => Changed?.Invoke();
        Changed?.Invoke();
        return s;
    }

    public async ValueTask DisposeAsync()
    {
        foreach (var s in all)   // one failure must not leave the other claude trees running
            try { await s.DisposeAsync(); }
            catch (Exception ex) { Console.Error.WriteLine($"[{s.Id}] shutdown: {ex.Message}"); }
    }
}
