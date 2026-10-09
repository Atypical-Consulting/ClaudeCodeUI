using System.Diagnostics;
using System.Text.Json;

namespace ClaudeCodeUI;

// `dotnet run -- --boot-probe <dir> [seconds]`: spawns claude exactly as a fresh LiveSession would, sends
// initialize and prints the boot timeline. Exit 0 = answered, 1 = timeout or error. Repro for cold-start stalls.
public static class BootProbe
{
    public static async Task<int> Run(string dir, int seconds)
    {
        var args = new LiveSession(Guid.NewGuid().ToString(), "boot-probe", dir, "default").Args(resume: false);
        await using var s = new ClaudeSession(dir, args, _ => Task.CompletedTask, (c, t) => Console.WriteLine($"exited {c}: {t}"), Console.WriteLine);
        var sw = Stopwatch.StartNew();
        try
        {
            var info = await s.Request("initialize", null, seconds);
            Console.WriteLine($"initialize OK in {sw.ElapsedMilliseconds} ms ({Count(info, "models")} models, {Count(info, "commands")} commands)");
            return 0;
        }
        catch (TimeoutException) { Console.WriteLine($"initialize TIMEOUT after {seconds} s"); }
        catch (ClaudeRequestException ex) { Console.WriteLine($"initialize ERROR {ex.Message}"); }
        return 1;

        static int Count(JsonElement e, string n) => Events.Prop(e, n) is { ValueKind: JsonValueKind.Array } x ? x.GetArrayLength() : 0;
    }
}
