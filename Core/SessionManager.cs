using System.Collections.Immutable;

namespace ClaudeCodeUI;

// All live sessions of the app (singleton). Kills every claude process on shutdown.
public sealed class SessionManager : IAsyncDisposable
{
    ImmutableList<LiveSession> all = [];
    readonly Lock openGate = new();

    public IReadOnlyList<LiveSession> All => all;   // creation order, never re-sorted
    public LiveSession? Get(string id) => all.FirstOrDefault(s => s.Id == id);

    // Sessions with a process (or a crash to show). A transcript only viewed stays Exited, under Recent, until its first Send.
    public IReadOnlyList<LiveSession> Active => [.. all.Where(s => s.Status != SessionStatus.Exited)];
    public bool IsActive(string id) => Get(id) is { Status: not SessionStatus.Exited };

    // Held by a claude process this app did not start (a terminal, another window).
    public bool RunningElsewhere(string id) => Get(id)?.HasProcess != true && TranscriptStore.HeldByClaude().Contains(id);

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
            var items = TranscriptStore.Load(p.Id);
            // The transcript's mode, if the UI offers it (bypassPermissions / dontAsk fall back to default).
            var mode = p.Mode is { } m && LiveSession.Modes.Contains(m) ? m : "default";
            var s = new LiveSession(p.Id, p.Title, p.Cwd, mode, resumable: true)
            {
                Items = [.. items], CostUsd = p.CostUsd ?? 0, CostAtOpen = p.CostUsd ?? 0,
                StartedAt = items.OfType<UserItem>().FirstOrDefault()?.At ?? p.LastWrite, LastEventAt = p.LastWrite,
            };
            s.ToolCount = s.Items.OfType<ToolItem>().Count();
            _ = s.RefreshGit();
            return Add(s);
        }
    }

    // A new live session continuing src's conversation under a new id; src is left as it is (--probe-cli fork).
    // History = src's transcript, what the CLI copies, cut after the message `at` for "Fork from here".
    public LiveSession Fork(LiveSession src, string? at = null)
    {
        var s = new LiveSession(Guid.NewGuid().ToString(), Strings.Get("Session.ForkName", src.Name), src.Cwd, src.Mode, model: src.Model, effort: src.Effort)
        {
            ForkOf = src.Id, ForkAt = at, Items = LiveSession.Upto(TranscriptStore.Load(src.Id), at), CostUsd = TranscriptStore.PersistedCost(src.Id),
        };
        s.ToolCount = s.Items.OfType<ToolItem>().Count();
        Add(s);
        try { s.EnsureProcess(); } catch (Exception) { }   // a failed start leaves the session Crashed with the reason
        _ = s.RefreshGit();
        return s;
    }

    // Kills the process and forgets the session: it goes back to Recent, its transcript intact.
    public async Task Stop(string id)
    {
        if (Get(id) is not { } s) return;
        ImmutableInterlocked.Update(ref all, l => l.Remove(s));
        await s.DisposeAsync();
        TranscriptStore.ForgetHeld();
        TranscriptStore.Invalidate();   // its Recent row must show the cost and title of the turns just run
        Changed?.Invoke();
    }

    LiveSession Add(LiveSession s)
    {
        ImmutableInterlocked.Update(ref all, l => l.Add(s));
        // Streamed text alone (Version unchanged) concerns the session page only, not the rail, quota, mascot or queue.
        var seen = -1;
        s.Changed += () =>
        {
            var v = s.Version;
            if (Interlocked.Exchange(ref seen, v) != v) Changed?.Invoke();
        };
        Changed?.Invoke();
        return s;
    }

    public async ValueTask DisposeAsync()
    {
        // in parallel (each may wait 2 s for its process); one failure must not leave the other claude trees running
        await Task.WhenAll(all.Select(async s =>
        {
            try { await s.DisposeAsync(); }
            catch (Exception ex) { Console.Error.WriteLine($"[{s.Id}] shutdown: {ex.Message}"); }
        }));
    }
}
