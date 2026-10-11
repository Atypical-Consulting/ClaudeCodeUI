using System.Text.Json;

namespace ClaudeCodeUI;

public record Bookmark(string Session, string Uuid, string Gist, DateTimeOffset At);

// Replies the user bookmarked (Thread), by session id and message uuid, kept in a JSON file next to the app's other state.
// UI only: the CLI has no bookmarks over stream-json. ponytail: one small file rewritten whole, fine for hundreds of entries.
public static class Bookmarks
{
    internal static string File = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ClaudeCodeUI", "bookmarks.json");
    static readonly Lock gate = new();
    static List<Bookmark>? cache;   // read once: the context panel asks on every render
    public static event Action? Changed;

    public static IReadOnlyList<Bookmark> All()
    {
        lock (gate)
        {
            try { cache ??= JsonSerializer.Deserialize<List<Bookmark>>(System.IO.File.ReadAllText(File)) ?? []; }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException or JsonException) { cache = []; }
            return [.. cache];
        }
    }

    public static IReadOnlyList<Bookmark> Of(string session) => [.. All().Where(b => b.Session == session)];

    // Adds the reply, or removes it when already bookmarked; true when it is bookmarked afterwards.
    public static bool Toggle(string session, string uuid, string gist)
    {
        lock (gate)
        {
            var all = All().ToList();
            var on = all.RemoveAll(b => b.Session == session && b.Uuid == uuid) == 0;
            if (on) all.Add(new(session, uuid, gist, DateTimeOffset.Now));
            Directory.CreateDirectory(Path.GetDirectoryName(File)!);
            System.IO.File.WriteAllText(File, JsonSerializer.Serialize(all));
            cache = all;
            Changed?.Invoke();
            return on;
        }
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Bookmarks: " + what);
        var saved = File;
        File = Path.Combine(Path.GetTempPath(), "ccui-check-" + Guid.NewGuid().ToString("N"), "bookmarks.json");
        cache = null;
        try
        {
            Ok(All().Count == 0, "none without a file");
            Ok(Toggle("s1", "u1", "first") && Toggle("s2", "u2", "other") && Of("s1") is [{ Uuid: "u1", Gist: "first" }], "added, per session");
            Ok(!Toggle("s1", "u1", "first") && Of("s1").Count == 0 && All() is [{ Session: "s2" }], "toggled off");
            System.IO.File.WriteAllText(File, "not json");
            cache = null;
            Ok(All().Count == 0, "unreadable file: none");
        }
        finally
        {
            if (Directory.Exists(Path.GetDirectoryName(File))) Directory.Delete(Path.GetDirectoryName(File)!, true);
            File = saved;
            cache = null;
        }
    }
}
