using System.Diagnostics;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

// One long-lived Claude Code CLI process speaking the Agent SDK stream-json protocol
// (same flags the SDK / VS Code extension use). Every stdout line is passed to onEvent.
public sealed class ClaudeSession : IAsyncDisposable
{
    readonly Process proc;
    readonly Task reader;
    readonly SemaphoreSlim writeLock = new(1, 1);

    public ClaudeSession(string cwd, string permissionMode, Func<JsonElement, Task> onEvent, Action<string> onExit)
    {
        var psi = new ProcessStartInfo("claude")
        {
            WorkingDirectory = cwd,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            StandardOutputEncoding = Encoding.UTF8,
            StandardInputEncoding = new UTF8Encoding(false),
        };
        foreach (var a in new[] { "--output-format", "stream-json", "--verbose", "--input-format", "stream-json", "--permission-prompt-tool=stdio", "--permission-mode", permissionMode })
            psi.ArgumentList.Add(a);

        proc = Process.Start(psi)!;
        var stderr = proc.StandardError.ReadToEndAsync();
        reader = Task.Run(async () =>
        {
            while (await proc.StandardOutput.ReadLineAsync() is { } line)
            {
                JsonElement evt;
                try { evt = JsonDocument.Parse(line).RootElement.Clone(); }
                catch (JsonException) { continue; }
                await onEvent(evt);
            }
            await proc.WaitForExitAsync();
            onExit($"claude exited {proc.ExitCode} {await stderr}".Trim());
        });
    }

    public Task SendUser(string text) => Write(new JsonObject
    {
        ["type"] = "user",
        ["message"] = new JsonObject { ["role"] = "user", ["content"] = text },
    });

    // Answer a "can_use_tool" control_request.
    public Task Respond(string requestId, bool allow, JsonElement input) => Write(new JsonObject
    {
        ["type"] = "control_response",
        ["response"] = new JsonObject
        {
            ["subtype"] = "success",
            ["request_id"] = requestId,
            ["response"] = allow
                ? new JsonObject { ["behavior"] = "allow", ["updatedInput"] = JsonNode.Parse(input.GetRawText()) }
                : new JsonObject { ["behavior"] = "deny", ["message"] = "The user denied this tool use." },
        },
    });

    public Task Interrupt() => Write(new JsonObject
    {
        ["type"] = "control_request",
        ["request_id"] = Guid.NewGuid().ToString("N"),
        ["request"] = new JsonObject { ["subtype"] = "interrupt" },
    });

    async Task Write(JsonNode msg)
    {
        await writeLock.WaitAsync();
        try
        {
            await proc.StandardInput.WriteLineAsync(msg.ToJsonString());
            await proc.StandardInput.FlushAsync();
        }
        finally { writeLock.Release(); }
    }

    public async ValueTask DisposeAsync()
    {
        try { proc.StandardInput.Close(); } catch { }
        if (!proc.WaitForExit(2000)) try { proc.Kill(true); } catch { }
        await reader.ConfigureAwait(false);
        proc.Dispose();
    }
}
