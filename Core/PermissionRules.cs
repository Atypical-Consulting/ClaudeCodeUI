using System.Globalization;
using System.Text;
using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

// One row of list_permission_rules (claude 2.1.296). Editability: persistent = in a settings file (user, project, local),
// session = in memory until the session ends, readonly = policy, flag, command and the rest. Text: the CLI's plain
// reading ("Any Bash command starting with " + "npm run"), absent where the terminal shows none.
public sealed record PermissionRule(string Behavior, string Source, string Rule, (string Prefix, string Emphasis, string Suffix)? Text,
                                    string Editability, bool NotInEffect)
{
    public bool Editable => Editability == "persistent" && !NotInEffect && PermissionRules.Files.Contains(Source);
}

public sealed record PermissionState(IReadOnlyList<PermissionRule> Rules, IReadOnlyList<(string Path, string Source)> Dirs,
                                     string? Cwd, bool ManagedOnly, IReadOnlyList<string> Errors);

// The /permissions equivalent. The CLI lists rules (list_permission_rules) but writes none over stream-json: update_settings
// takes outputStyle only, apply_flag_settings is a session layer. So add and remove edit the settings file the CLI reads,
// which it watches and reloads (verified on 2.1.296: a rule written to settings.local.json is listed 3 s later).
public static class PermissionRules
{
    public static readonly string[] Behaviors = ["allow", "ask", "deny"];
    public static readonly string[] Files = ["localSettings", "projectSettings", "userSettings"];

    static readonly JsonSerializerOptions Write = new() { WriteIndented = true, Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping };

    public static PermissionState Parse(JsonElement response)
    {
        var st = Events.Prop(response, "state") ?? default;
        static IEnumerable<JsonElement> Arr(JsonElement e, string n) => Events.Prop(e, n) is { ValueKind: JsonValueKind.Array } a ? a.EnumerateArray() : [];
        return new(
            [.. Arr(st, "rules").Select(r => new PermissionRule(Events.Str(r, "behavior") ?? "", Events.Str(r, "source") ?? "", Events.Str(r, "rule") ?? "",
                Events.Prop(r, "description") is { } d ? (Events.Str(d, "prefix") ?? "", Events.Str(d, "emphasis") ?? "", Events.Str(d, "suffix") ?? "") : null,
                Events.Str(r, "editability") ?? "readonly", Events.Prop(r, "notInEffect") is { ValueKind: JsonValueKind.True }))],
            [.. Arr(st, "workspaceDirectories").Select(d => (Events.Str(d, "path") ?? "", Events.Str(d, "source") ?? ""))],
            Events.Str(st, "originalCwd"), Events.Prop(st, "managedOnly") is { ValueKind: JsonValueKind.True },
            [.. Arr(st, "errors").Select(x => string.Join(" · ", new[] { Events.Str(x, "file"), Events.Str(x, "path"), Events.Str(x, "message") }.Where(s => !string.IsNullOrEmpty(s))))]);
    }

    // Where the CLI reads each file (2.1.296): user = the config dir, project = cwd, local = the canonical git root (the
    // main checkout for a worktree; cwd outside a repo or when the root is the home folder).
    public static string? FileOf(string source, string cwd, string? gitRoot, string configDir) => source switch
    {
        "userSettings" => Path.Combine(configDir, "settings.json"),
        "projectSettings" => Path.Combine(cwd, ".claude", "settings.json"),
        "localSettings" => Path.Combine(gitRoot is { Length: > 0 } g && g != Environment.GetFolderPath(Environment.SpecialFolder.UserProfile) ? g : cwd,
                                        ".claude", "settings.local.json"),
        _ => null,
    };

    // A rule is a tool name (Bash, mcp__srv__*) optionally followed by its content in parentheses: Bash(npm run:*).
    public static bool Valid(string rule) =>
        System.Text.RegularExpressions.Regex.IsMatch(rule, @"^[A-Za-z0-9_\-:.*]+(\(.+\))?$", System.Text.RegularExpressions.RegexOptions.Singleline);

    // The file's text with `rule` added to (or removed from) permissions.<behavior>, every other key kept. Throws on a file
    // that is not strict JSON: the CLI skips such a file, and rewriting it would drop what the user wrote.
    public static string Edit(string? json, string behavior, string rule, bool add)
    {
        var root = string.IsNullOrWhiteSpace(json) ? new JsonObject() : JsonNode.Parse(json) as JsonObject ?? throw new JsonException("not a JSON object");
        var perms = root["permissions"] as JsonObject ?? (JsonObject)(root["permissions"] = new JsonObject());
        var list = perms[behavior] as JsonArray ?? (JsonArray)(perms[behavior] = new JsonArray());
        var at = list.Select((n, i) => (n, i)).Where(x => x.n?.GetValueKind() == JsonValueKind.String && x.n.GetValue<string>() == rule).Select(x => x.i).ToList();
        if (add && at.Count == 0) list.Add(rule);
        if (!add) foreach (var i in Enumerable.Reverse(at)) list.RemoveAt(i);
        return root.ToJsonString(Write) + "\n";
    }

    public static void Save(string path, string behavior, string rule, bool add)
    {
        var text = File.Exists(path) ? File.ReadAllText(path) : null;
        var next = Edit(text, behavior, rule, add);
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);
        File.WriteAllText(path, next);
    }

    // The CLI says rules can carry invisible or control characters by design: shown as \u{…} so two rules never look alike.
    public static string Visible(string s)
    {
        var b = new StringBuilder(s.Length);
        foreach (var c in s)
            if (char.IsControl(c) || CharUnicodeInfo.GetUnicodeCategory(c) == UnicodeCategory.Format) b.Append($"\\u{{{(int)c:X4}}}");
            else b.Append(c);
        return b.ToString();
    }

    // decision_reason and matched_ask_rule "may carry ANSI escapes; sanitize before rendering" (2.1.296 schema): escapes dropped, the rest made visible, lines kept.
    public static string Plain(string s) => string.Join('\n', System.Text.RegularExpressions.Regex.Replace(s, Ansi, "").Split('\n').Select(Visible));
    public const string Ansi = @"\x1B(\[[0-?]*[ -/]*[@-~]|\][^\x07\x1B]*(\x07|\x1B\\)|[@-_])";

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "PermissionRules: " + what);
        // claude 2.1.296, trimmed: a project ask rule, a project deny rule, a user rule without description.
        var st = Parse(JsonDocument.Parse("""{"state":{"rules":[{"behavior":"ask","source":"projectSettings","rule":"Bash(echo:*)","description":{"prefix":"Any Bash command starting with ","emphasis":"echo"},"editability":"persistent"},{"behavior":"deny","source":"projectSettings","rule":"Read(./secret.txt)","editability":"persistent"},{"behavior":"allow","source":"policySettings","rule":"WebSearch","description":{"prefix":"Any use of the ","emphasis":"WebSearch","suffix":" tool"},"editability":"readonly"}],"workspaceDirectories":[{"path":"/x","source":"cliArg"}],"originalCwd":"/w","managedOnly":false}}""").RootElement);
        Ok(st.Rules is [{ Behavior: "ask", Text: ("Any Bash command starting with ", "echo", ""), Editable: true }, { Text: null, Editable: true }, { Editable: false, Text: (_, _, " tool") }]
           && st.Dirs is [("/x", "cliArg")] && st.Cwd == "/w" && st.Errors.Count == 0, "list_permission_rules");
        Ok(Parse(JsonDocument.Parse("""{"state":{"rules":[{"behavior":"allow","source":"localSettings","rule":"Bash","editability":"readonly","notInEffect":true}],"workspaceDirectories":[],"originalCwd":"/w","managedOnly":true,"errors":[{"file":"/w/.claude/settings.json","path":"","message":"Invalid JSON"}]}}""").RootElement)
           is { ManagedOnly: true, Rules: [{ Editable: false }], Errors: ["/w/.claude/settings.json · Invalid JSON"] }, "managed-only and errors");

        var added = Edit("""{"model":"haiku","permissions":{"allow":["Bash(ls:*)"],"defaultMode":"plan"}}""", "deny", "Read(./.env)", true);
        var n = JsonNode.Parse(added)!;
        Ok(n["model"]!.GetValue<string>() == "haiku" && n["permissions"]!["defaultMode"]!.GetValue<string>() == "plan"
           && n["permissions"]!["deny"]![0]!.GetValue<string>() == "Read(./.env)" && n["permissions"]!["allow"]!.AsArray().Count == 1, "add keeps the other keys");
        Ok(Edit(added, "deny", "Read(./.env)", true) == added, "add is idempotent");
        Ok(JsonNode.Parse(Edit(added, "allow", "Bash(ls:*)", false))!["permissions"]!["allow"]!.AsArray().Count == 0, "remove");
        Ok(Edit(null, "allow", "Bash(a<b)", true).Contains("\"Bash(a<b)\""), "new file, no \\u escaping");
        var threw = false;
        try { Edit("{ // comment\n}", "allow", "Bash", true); } catch (JsonException) { threw = true; }
        Ok(threw, "a file the CLI would skip is never rewritten");
        Ok(Valid("Bash(npm run:*)") && Valid("mcp__roseline__*") && Valid("WebFetch(domain:github.com)") && !Valid("") && !Valid("Bash(") && !Valid("two words"), "rule syntax");
        Ok(Visible("Bash(a​b)") == "Bash(a\\u{200B}b)", "invisible characters shown");
        Ok(Plain("\u001b[31mBlocked\u001b[0m by hook\nline\u0007") == "Blocked by hook\nline\\u{0007}", "decision_reason sanitized");
        Ok(FileOf("localSettings", "/r/.claude/worktrees/x", "/r", "/c") == Path.Combine("/r", ".claude", "settings.local.json")
           && FileOf("projectSettings", "/r/.claude/worktrees/x", "/r", "/c") == Path.Combine("/r/.claude/worktrees/x", ".claude", "settings.json")
           && FileOf("localSettings", "/t", null, "/c") == Path.Combine("/t", ".claude", "settings.local.json")
           && FileOf("userSettings", "/t", null, "/c") == Path.Combine("/c", "settings.json") && FileOf("policySettings", "/t", null, "/c") is null, "file locations");
    }
}
