using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

// The default model of new sessions: "model" in the user settings file, which the CLI's update_settings refuses to write
// (it takes effortLevel only). Every other key is kept; a file that does not parse is left alone (the error says why).
public static class UserSettings
{
    static string File => Path.Combine(Environment.GetEnvironmentVariable("CLAUDE_CONFIG_DIR") is { Length: > 0 } d ? d
        : Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".claude"), "settings.json");

    public static void SetModel(string model)
    {
        var json = System.IO.File.Exists(File) ? System.IO.File.ReadAllText(File) : "{}";
        Directory.CreateDirectory(Path.GetDirectoryName(File)!);
        var tmp = File + ".ccui-tmp";
        System.IO.File.WriteAllText(tmp, WithModel(json, model));
        System.IO.File.Move(tmp, File, overwrite: true);   // never a half-written settings file
    }

    // "default" removes the key: the CLI's own default applies again.
    internal static string WithModel(string json, string model)
    {
        var o = JsonNode.Parse(json) as JsonObject ?? throw new JsonException("settings.json is not an object");
        if (model == "default") o.Remove("model");
        else o["model"] = model;
        return o.ToJsonString(new JsonSerializerOptions { WriteIndented = true });
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "UserSettings: " + what);
        var set = JsonNode.Parse(WithModel("""{"effortLevel":"xhigh","model":"opus[1m]","modelSettings":{"claude-fable-5-1":{"effortLevel":"high"}}}""", "sonnet"))!;
        Ok((string?)set["model"] == "sonnet" && (string?)set["modelSettings"]!["claude-fable-5-1"]!["effortLevel"] == "high" && (string?)set["effortLevel"] == "xhigh", "model set, other keys kept");
        Ok(JsonNode.Parse(WithModel("""{"model":"opus"}""", "default"))!.AsObject().Count == 0, "default removes the key");
        try { WithModel("[1]", "x"); Ok(false, "array refused"); } catch (JsonException) { }
    }
}
