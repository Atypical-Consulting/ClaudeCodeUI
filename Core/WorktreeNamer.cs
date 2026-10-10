using System.Diagnostics;
using System.Text;

namespace ClaudeCodeUI;

// A worktree name from Haiku, summarizing the first prompt: one-shot `claude -p`, no tools, no settings, no transcript.
public static class WorktreeNamer
{
    public static readonly TimeSpan Timeout = TimeSpan.FromSeconds(8);

    const string Ask = "Give a git branch name (2-4 lowercase English words, kebab-case, no prefix, no punctuation) summarizing this task. Reply with the name only.\n\nTask:\n";

    // Per prompt text: re-renders, the debounce and Start all share one call. The task, not its result, so an in-flight call is shared too.
    static readonly Dictionary<string, Task<string?>> cache = [];

    // Null on failure, timeout, missing claude or cancel; never throws. ct only stops the wait: the call itself is shared, bounded by Timeout.
    public static async Task<string?> Suggest(string prompt, CancellationToken ct = default)
    {
        try { return await Cached(prompt.Trim(), Run).WaitAsync(ct); }
        catch (Exception) { return null; }
    }

    internal static Task<string?> Cached(string key, Func<string, Task<string?>> run)
    {
        lock (cache)
        {
            if (cache.TryGetValue(key, out var t)) return t;
            if (cache.Count >= 32) cache.Clear();   // ponytail: drop all when full, an LRU if prompts get revisited often
            return cache[key] = Task.Run(() => run(key));   // the spawn stays out of the lock
        }
    }

    // The model's reply, made branch-safe; null when nothing usable is left (the caller keeps the heuristic name).
    internal static string? Clean(string? reply) => TranscriptStore.NameFrom(reply ?? "", 4) is { Length: > 0 } n ? n : null;

    static async Task<string?> Run(string prompt)
    {
        if (!ClaudeSession.OnPath()) return null;
        using var cts = new CancellationTokenSource(Timeout);
        // A neutral folder and no setting sources: the user's project must not load its CLAUDE.md, hooks or MCP servers for a name.
        var psi = new ProcessStartInfo("claude")
        {
            WorkingDirectory = Path.GetTempPath(),
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            CreateNoWindow = true,
            StandardOutputEncoding = Encoding.UTF8,
            StandardInputEncoding = new UTF8Encoding(false),
        };
        // --no-session-persistence: otherwise every naming call would show up as a session in the rail.
        foreach (var a in new[] { "-p", "--model", "haiku", "--output-format", "text", "--tools", "", "--max-turns", "1",
                                  "--setting-sources", "", "--strict-mcp-config", "--no-session-persistence" })
            psi.ArgumentList.Add(a);
        Process? p = null;
        try
        {
            p = Process.Start(psi)!;
            // Through stdin, not argv: a prompt can be long and hold anything.
            await p.StandardInput.WriteAsync((Ask + (prompt.Length > 2000 ? prompt[..2000] : prompt)).AsMemory(), cts.Token);
            p.StandardInput.Close();
            var err = p.StandardError.ReadToEndAsync(cts.Token);   // drained so a chatty stderr never blocks the child
            var output = await p.StandardOutput.ReadToEndAsync(cts.Token);
            await p.WaitForExitAsync(cts.Token);
            await err;
            return p.ExitCode == 0 ? Clean(output) : null;
        }
        catch (Exception)
        {
            try { p?.Kill(true); } catch { }
            return null;
        }
        finally { p?.Dispose(); }
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "WorktreeNamer: " + what);
        Ok(Clean("`Persist-Sessions-JSON`\n") == "persist-sessions-json", "Clean");
        Ok(Clean("fix auth token refresh bug now") == "fix-auth-token-refresh" && Clean("  ...  ") is null && Clean(null) is null, "Clean bounds");
        var calls = 0;
        Task<string?> Fake(string k) { calls++; return Task.FromResult<string?>(k); }
        var key = $"cc-ui-check-{Guid.NewGuid()}";
        var first = Cached(key, Fake);
        Ok(first == Cached(key, Fake) && first.Result == key && calls == 1, "Cached calls once per prompt");
    }
}
