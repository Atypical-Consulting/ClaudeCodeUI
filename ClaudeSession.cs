using System.Collections.Concurrent;
using System.Diagnostics;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

public sealed class ClaudeRequestException(string message) : Exception(message);

// One long-lived Claude Code CLI process speaking the Agent SDK stream-json protocol
// (same flags the SDK / VS Code extension use). Every stdout line is passed to onEvent;
// control_response lines also complete the matching Request().
public sealed class ClaudeSession : IAsyncDisposable
{
    static readonly JsonElement Empty = JsonDocument.Parse("{}").RootElement;

    readonly Process proc;
    readonly ProcessJob? job;
    readonly Task reader;
    readonly SemaphoreSlim writeLock = new(1, 1);
    readonly ConcurrentDictionary<string, TaskCompletionSource<JsonElement>> requests = new();
    readonly Action<string>? trace;
    readonly Stopwatch clock = new();
    volatile bool disposing;

    // trace (optional): timestamped boot timeline — spawn, writes, first stdout line, live stderr, late answers.
    public ClaudeSession(string cwd, IReadOnlyList<string> args, Func<JsonElement, Task> onEvent, Action<int, string> onExit, Action<string>? trace = null)
    {
        this.trace = trace;
        var psi = new ProcessStartInfo("claude")
        {
            WorkingDirectory = cwd,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            StandardOutputEncoding = Encoding.UTF8,
            StandardInputEncoding = new UTF8Encoding(false),
        };
        foreach (var a in new[] { "--output-format", "stream-json", "--verbose", "--input-format", "stream-json", "--permission-prompt-tool=stdio" })
            psi.ArgumentList.Add(a);
        foreach (var a in args) psi.ArgumentList.Add(a);

        ProcessJob.Prepare(psi);
        clock.Start();
        proc = Process.Start(psi)!;
        job = ProcessJob.Attach(proc);
        Trace($"spawn pid={proc.Id}");
        // Read stderr line by line so what the CLI prints while it is slow shows up live, not only after exit.
        // The readers drop the ExecutionContext of the circuit that started claude: the renders their events trigger
        // take the app-wide culture (Program.cs) instead of the language that circuit had, possibly switched since.
        using var noFlow = ExecutionContext.SuppressFlow();
        var errText = new StringBuilder();
        var stderr = Task.Run(async () =>
        {
            while (await proc.StandardError.ReadLineAsync() is { } l)
            {
                lock (errText) errText.AppendLine(l);
                Trace($"stderr {l}");
            }
        });
        reader = Task.Run(async () =>
        {
            var first = true;
            while (await proc.StandardOutput.ReadLineAsync() is { } line)
            {
                JsonElement evt;
                try { evt = JsonDocument.Parse(line).RootElement.Clone(); }
                catch (JsonException) { continue; }
                if (first) { first = false; Trace($"first stdout {Str(evt, "type")}"); }
                if (Str(evt, "type") == "control_response") Complete(evt.GetProperty("response"));
                try { await onEvent(evt); }
                catch (Exception ex) { Console.Error.WriteLine($"claude event handler failed: {ex}"); }
            }
            await proc.WaitForExitAsync();
            job?.Dispose();   // external kill or crash: take the MCP servers down with it, before onExit
            foreach (var r in requests.Values) r.TrySetException(new ClaudeRequestException(Strings.Get("Session.ProcessStopped")));
            await stderr;
            if (!disposing) onExit(proc.ExitCode, $"claude exited {proc.ExitCode} {errText}".Trim());
        });
    }

    void Complete(JsonElement r)
    {
        if (Str(r, "request_id") is not { } id) return;
        if (!requests.TryRemove(id, out var tcs)) { Trace($"late control_response {id}"); return; }
        if (Str(r, "subtype") == "error") tcs.TrySetException(new ClaudeRequestException(Str(r, "error") ?? Strings.Get("Session.UnknownError")));
        else tcs.TrySetResult(r.TryGetProperty("response", out var v) ? v : Empty);
    }

    // Sends a control_request and waits (15 s by default) for its control_response payload.
    public async Task<JsonElement> Request(string subtype, JsonObject? fields = null, int seconds = 15)
    {
        var id = Guid.NewGuid().ToString("N");
        var tcs = new TaskCompletionSource<JsonElement>(TaskCreationOptions.RunContinuationsAsynchronously);
        requests[id] = tcs;
        var req = new JsonObject { ["subtype"] = subtype };
        if (fields is not null)
            foreach (var (k, v) in fields) req[k] = v?.DeepClone();
        try
        {
            await Write(new JsonObject { ["type"] = "control_request", ["request_id"] = id, ["request"] = req });
            return await tcs.Task.WaitAsync(TimeSpan.FromSeconds(seconds));
        }
        finally { requests.TryRemove(id, out _); }
    }

    public Task SendUser(string text) => Write(new JsonObject
    {
        ["type"] = "user",
        ["message"] = new JsonObject { ["role"] = "user", ["content"] = text },
    });

    // Answer a "can_use_tool" control_request.
    public Task Respond(string requestId, bool allow, JsonElement input, JsonNode? updatedPermissions = null)
    {
        var res = allow
            ? new JsonObject { ["behavior"] = "allow", ["updatedInput"] = JsonNode.Parse(input.GetRawText()) }
            : new JsonObject { ["behavior"] = "deny", ["message"] = "Refusé par l’utilisateur" };
        if (allow && updatedPermissions is not null) res["updatedPermissions"] = updatedPermissions.DeepClone();
        return Write(new JsonObject
        {
            ["type"] = "control_response",
            ["response"] = new JsonObject { ["subtype"] = "success", ["request_id"] = requestId, ["response"] = res },
        });
    }

    public Task Interrupt() => Request("interrupt");

    async Task Write(JsonNode msg)
    {
        await writeLock.WaitAsync();
        try
        {
            if (trace is not null)
                Trace($"write {msg["type"]}" + (msg["request"]?["subtype"] is { } st ? $"/{st}" : ""));
            await proc.StandardInput.WriteLineAsync(msg.ToJsonString());
            await proc.StandardInput.FlushAsync();
        }
        finally { writeLock.Release(); }
    }

    void Trace(string what) => trace?.Invoke(TraceLine(clock.ElapsedMilliseconds, what));

    internal static string TraceLine(long ms, string what) => $"+{ms} ms {what}";

    // Same lookup as Process.Start("claude") with UseShellExecute=false: on Windows CreateProcess only appends ".exe".
    public static bool OnPath() =>
        (Environment.GetEnvironmentVariable("PATH") ?? "").Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries)
            .Any(d => File.Exists(Path.Combine(d.Trim('"'), OperatingSystem.IsWindows() ? "claude.exe" : "claude")));

    static string? Str(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Object && e.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() : null;

    public async ValueTask DisposeAsync()
    {
        disposing = true;
        try { proc.StandardInput.Close(); } catch { }
        if (!proc.WaitForExit(2000)) try { proc.Kill(true); } catch { }
        job?.Dispose();
        await reader.ConfigureAwait(false);
        proc.Dispose();
    }
}
