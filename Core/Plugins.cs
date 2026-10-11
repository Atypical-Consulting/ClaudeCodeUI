using System.Diagnostics;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

// `claude plugin … --json` (2.1.296): the CLI owns installs, marketplaces, settings and caches, and the stream-json
// protocol has no request for any of it. We run the command in the session's folder (project and local scopes are
// per folder) and read its answer; reload_plugins then applies the change to the running session.
public static class Plugins
{
    // One install as `plugin list --json` reports it. Errors / Notes: the CLI's own lines for a load failure, a demoted
    // dependency, a shadowed folder… already worded for a person. A plugin installed in two scopes is two rows.
    public record Row(string Id, string Version, string Scope, bool Enabled, string[] Errors, string[] Notes, bool HasOptions)
    {
        public string Name => Id.Split('@')[0];
    }
    public record Listed(string Id, string? Description, string Marketplace, long Installs);
    public record Market(string Name, string Where);
    // A failed install/update whose marketplace runs a command first carries it: it runs only once a person has seen it,
    // with --accept-command <sha> (bound to that command and that catalog, so a changed one is refused again).
    public record Outcome(bool Ok, string Message, string? Command = null, string? CommandSha = null);
    // One userConfig option of `plugin configure --json`. Value: the current text ("" for a sensitive one, never echoed).
    public record Option(string Key, string Type, string Title, string? Description, bool Required, bool Sensitive, string Value, string[] Choices, bool Set);

    public static readonly string[] Scopes = ["user", "project", "local"];

    public static async Task<(List<Row> Installed, List<Listed> Available)> List(string cwd)
    {
        var (code, output, err) = await Run(cwd, ["list", "--json", "--available"]);
        if (code != 0) throw new InvalidOperationException(Line(err, output));
        return ParseList(JsonDocument.Parse(output).RootElement);
    }

    internal static (List<Row>, List<Listed>) ParseList(JsonElement r) => (
        Events.Prop(r, "installed") is { ValueKind: JsonValueKind.Array } i
            ? [.. i.EnumerateArray().Select(p => new Row(Events.Str(p, "id") ?? "", Events.Str(p, "version") ?? "", Events.Str(p, "scope") ?? "",
                Events.Bool(p, "enabled"), Texts(p, "errors"), Texts(p, "notes"), Events.Bool(p, "hasUserConfig")))]
            : [],
        Events.Prop(r, "available") is { ValueKind: JsonValueKind.Array } a
            ? [.. a.EnumerateArray().Select(p => new Listed(Events.Str(p, "pluginId") ?? "", Events.Str(p, "description"), Events.Str(p, "marketplaceName") ?? "", Events.Long(p, "installCount")))]
            : []);

    public static async Task<List<Market>> Markets(string cwd)
    {
        var (code, output, err) = await Run(cwd, ["marketplace", "list", "--json"]);
        if (code != 0) throw new InvalidOperationException(Line(err, output));
        return ParseMarkets(JsonDocument.Parse(output).RootElement);
    }

    // github → owner/repo, git/url → the URL, directory/file → the path.
    internal static List<Market> ParseMarkets(JsonElement r) => r.ValueKind == JsonValueKind.Array
        ? [.. r.EnumerateArray().Select(m => new Market(Events.Str(m, "name") ?? "", Events.Str(m, "repo") ?? Events.Str(m, "url") ?? Events.Str(m, "path") ?? Events.Str(m, "source") ?? ""))]
        : [];

    // install / uninstall / enable / disable / update / marketplace add|update|remove: one result line, the last on stdout.
    public static async Task<Outcome> Act(string cwd, params string[] args)
    {
        var (_, output, err) = await Run(cwd, [.. args, "--json"]);
        return ParseOutcome(output) ?? new(false, Line(err, output));
    }

    internal static Outcome? ParseOutcome(string output)
    {
        foreach (var line in output.Split('\n').Reverse())
        {
            if (!line.TrimStart().StartsWith('{')) continue;
            try
            {
                var r = JsonDocument.Parse(line).RootElement;
                if (Events.Str(r, "outcome") is not { } o) continue;
                var shown = Events.Prop(r, "shownCommand");
                var cmd = shown is { } c ? Events.Prop(c, "command") : null;
                return new(o == "ok", Events.Str(r, "message") ?? o,
                    cmd is { ValueKind: JsonValueKind.Array } ca ? string.Join(" ", ca.EnumerateArray().Select(x => x.ToString())) : cmd?.ToString(),
                    shown is { } sc ? Events.Str(sc, "sha256") : null);
            }
            catch (JsonException) { }
        }
        return null;
    }

    public static async Task<List<Option>> Options(string cwd, string id)
    {
        var (code, output, err) = await Run(cwd, ["configure", id, "--json"]);
        if (code != 0) throw new InvalidOperationException(Line(err, output));
        return ParseOptions(JsonDocument.Parse(output).RootElement);
    }

    internal static List<Option> ParseOptions(JsonElement r)
    {
        var set = Events.Names(r, "configured");
        var inputs = Events.Prop(r, "inputs");
        var choices = Events.Prop(r, "choices");
        return Events.Prop(r, "schema") is { ValueKind: JsonValueKind.Object } schema
            ? [.. schema.EnumerateObject().Select(o => new Option(o.Name, Events.Str(o.Value, "type") ?? "string", Events.Str(o.Value, "title") ?? o.Name,
                Events.Str(o.Value, "description"), Events.Bool(o.Value, "required"), Events.Bool(o.Value, "sensitive"),
                inputs is { } iv ? Events.Str(iv, o.Name) ?? "" : "", choices is { } cv ? Events.Names(cv, o.Name) : [], set.Contains(o.Name)))]
            : [];
    }

    // --values-stdin: a JSON object of single-line strings, the options left out keep their values. null = saved, else
    // the CLI's refusal (an out-of-range number, an unknown option…).
    public static async Task<string?> Save(string cwd, string id, IReadOnlyDictionary<string, string> values)
    {
        var json = new JsonObject();
        foreach (var (k, v) in values) json[k] = v.ReplaceLineEndings(" ");
        var (code, output, err) = await Run(cwd, ["configure", id, "--values-stdin", "--json"], json.ToJsonString());
        if (code == 0) return null;
        try { return Events.Prop(JsonDocument.Parse(output).RootElement, "refused") is { } rf ? Events.Str(rf, "message") : Line(err, output); }
        catch (JsonException) { return Line(err, output); }
    }

    static string[] Texts(JsonElement e, string name) => Events.Prop(e, name) is { ValueKind: JsonValueKind.Array } ? Events.Names(e, name) : [];

    // The CLI's message for a failure without a JSON line: its last non-empty line, stderr first.
    static string Line(string err, string output) =>
        (err + "\n" + output).Split('\n').Select(l => l.Trim()).LastOrDefault(l => l.Length > 0) ?? "claude plugin";

    // stdin is a pipe, never a TTY: every confirmation the CLI would ask for is refused rather than awaited. Installs clone
    // git repositories: 5 minutes before giving up.
    static async Task<(int Code, string Out, string Err)> Run(string cwd, string[] args, string? stdin = null)
    {
        var psi = new ProcessStartInfo("claude")
        {
            WorkingDirectory = cwd, RedirectStandardInput = true, RedirectStandardOutput = true, RedirectStandardError = true,
            StandardOutputEncoding = Encoding.UTF8, StandardInputEncoding = new UTF8Encoding(false), CreateNoWindow = true,
        };
        psi.ArgumentList.Add("plugin");
        foreach (var a in args) psi.ArgumentList.Add(a);
        using var p = Process.Start(psi)!;
        if (stdin is not null) await p.StandardInput.WriteAsync(stdin);
        p.StandardInput.Close();
        var output = p.StandardOutput.ReadToEndAsync();
        var err = p.StandardError.ReadToEndAsync();
        using var cts = new CancellationTokenSource(TimeSpan.FromMinutes(5));
        try { await p.WaitForExitAsync(cts.Token); }
        catch (OperationCanceledException) { p.Kill(true); throw new TimeoutException("claude plugin " + string.Join(" ", args)); }
        return (p.ExitCode, await output, await err);
    }

    // Trimmed real answers of claude 2.1.296 (a local marketplace with a "demo" plugin declaring 4 userConfig options).
    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Plugins: " + what);
        var (rows, avail) = ParseList(JsonDocument.Parse("""{"installed":[{"id":"demo@probe-mk","version":"1.0.0","scope":"user","enabled":false,"installPath":"/c/demo/1.0.0","projectEnabled":false,"hasUserConfig":true},{"id":"bad@inline","version":"unknown","scope":"session","enabled":false,"installPath":"/w/bad","errors":["Path not found: /w/bad"],"errorDetails":[{"type":"path-not-found","path":"/w/bad"}]}],"available":[{"pluginId":"demo@probe-mk","name":"demo","description":"Demo plugin","marketplaceName":"probe-mk","version":"1.0.0","source":"./plugins/demo"},{"pluginId":"figma@claude-plugins-official","name":"figma","marketplaceName":"claude-plugins-official","installCount":3952}]}""").RootElement);
        Ok(rows is [{ Name: "demo", Scope: "user", Enabled: false, HasOptions: true, Errors: [] }, { Errors: ["Path not found: /w/bad"], Notes: [] }]
           && avail is [{ Id: "demo@probe-mk", Description: "Demo plugin", Installs: 0 }, { Marketplace: "claude-plugins-official", Installs: 3952 }], "list");
        Ok(ParseMarkets(JsonDocument.Parse("""[{"name":"official","source":"github","repo":"anthropics/claude-plugins-official","installLocation":"/m"},{"name":"probe-mk","source":"directory","path":"/w/mk","installLocation":"/w/mk"}]""").RootElement)
           is [{ Name: "official", Where: "anthropics/claude-plugins-official" }, { Where: "/w/mk" }], "marketplaces");
        Ok(ParseOutcome("""{"command":"enable","outcome":"ok","plugin":"demo@probe-mk","pluginId":"demo@probe-mk","scope":"user","message":"Successfully enabled plugin: demo (scope: user)"}""" + "\n")
           is { Ok: true, Message: "Successfully enabled plugin: demo (scope: user)", Command: null }, "outcome ok");
        Ok(ParseOutcome("""{"command":"disable","outcome":"failed","plugin":"demo@probe-mk","message":"Plugin \"demo@probe-mk\" is already disabled","failureCode":"already_in_goal_state","alreadyInGoalState":true}""")
           is { Ok: false, Message: "Plugin \"demo@probe-mk\" is already disabled" }, "outcome failed");
        Ok(ParseOutcome("""{"command":"install","outcome":"failed","plugin":"x@m","message":"Refused","failureCode":"command_source_refused","shownCommand":{"kind":"command_source","pluginId":"x@m","command":["npx","x"],"sha256":"ab12"}}""")
           is { Ok: false, Command: "npx x", CommandSha: "ab12" } && ParseOutcome("Usage: …") is null, "outcome with a command to accept");
        var opts = ParseOptions(JsonDocument.Parse("""{"pluginId":"demo@probe-mk","displayName":"demo","schema":{"api_url":{"type":"string","title":"API URL","description":"Where to call","required":true},"token":{"type":"string","title":"Token","description":"Secret","sensitive":true},"verbose":{"type":"boolean","title":"Verbose","description":"Log more","default":false}},"inputs":{"api_url":"https://x.test","token":"","verbose":"true"},"choices":{"verbose":["true","false"]},"configured":["api_url","token"],"unconfigured":["verbose"]}""").RootElement);
        Ok(opts is [{ Key: "api_url", Required: true, Value: "https://x.test", Set: true }, { Sensitive: true, Value: "", Set: true }, { Type: "boolean", Choices: ["true", "false"], Set: false }], "configure");
    }
}
