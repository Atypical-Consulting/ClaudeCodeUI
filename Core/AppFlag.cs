namespace ClaudeCodeUI;

// An on/off app setting, kept as an empty file next to the app's other state (WorktreeService's repos.json): the desktop
// server binds port 0, so the page's origin, and the localStorage that goes with it, changes with every launch.
public sealed class AppFlag(string name)
{
    public static readonly AppFlag Thinking = new("thinking-summaries");   // claude starts with --thinking-display summarized
    public static readonly AppFlag Timestamps = new("timestamps");         // the thread shows reply times and day dividers

    internal string Path = System.IO.Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ClaudeCodeUI", name);

    public bool On
    {
        get => File.Exists(Path);
        set
        {
            try
            {
                if (!value) { if (File.Exists(Path)) File.Delete(Path); return; }
                Directory.CreateDirectory(System.IO.Path.GetDirectoryName(Path)!);
                File.WriteAllText(Path, "");
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }   // the getter shows what stuck
        }
    }
}

// A choice among a few values, kept the same way (the file holds the value). Changed lets an open page follow a change
// made elsewhere, e.g. the palette's theme command while Settings is on screen.
public sealed class AppPref(string name, string[] values)
{
    public static readonly AppPref Theme = new("theme", ["graphite", "encre", "ristretto", "mousse", "contraste"]);
    public static readonly AppPref CodeSize = new("code-size", ["12px", "13px", "14px", "15px"]);
    // The splitters' widths (app.js): every pixel between the bounds, so a hand-edited file cannot break the grid.
    public static readonly AppPref RailWidth = new("rail-width", Px(200, 400));
    public static readonly AppPref InspectorWidth = new("inspector-width", Px(320, 720));

    static string[] Px(int min, int max) => [.. Enumerable.Range(min, max - min + 1).Select(i => i + "px")];

    public static event Action? Changed;

    internal string Path = System.IO.Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ClaudeCodeUI", name);
    public readonly string[] Values = values;

    // null: never chosen (or unreadable), the page falls back to what the browser remembers.
    public string? Value
    {
        get
        {
            try { var v = File.ReadAllText(Path).Trim(); return Values.Contains(v) ? v : null; }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return null; }
        }
        set
        {
            if (value is null || !Values.Contains(value) || value == Value) return;
            try
            {
                Directory.CreateDirectory(System.IO.Path.GetDirectoryName(Path)!);
                File.WriteAllText(Path, value);
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { return; }
            Changed?.Invoke();
        }
    }

    internal static void Check()
    {
        var p = new AppPref("x", ["a", "b"]) { Path = System.IO.Path.Combine(System.IO.Path.GetTempPath(), "ccui-check-" + Guid.NewGuid().ToString("N"), "x") };
        var fired = 0;
        void Count() => fired++;
        Changed += Count;
        try
        {
            SelfCheck.Assert(p.Value is null, "AppPref: unset reads null");
            p.Value = "b";
            SelfCheck.Assert(p.Value == "b" && fired == 1, "AppPref: a value sticks and notifies");
            p.Value = "b"; p.Value = "z"; p.Value = null;
            SelfCheck.Assert(p.Value == "b" && fired == 1, "AppPref: same, unknown or null value changes nothing");
            File.WriteAllText(p.Path, "evil");
            SelfCheck.Assert(p.Value is null, "AppPref: a value outside the list reads null");
        }
        finally { Changed -= Count; Directory.Delete(System.IO.Path.GetDirectoryName(p.Path)!, true); }
    }
}
