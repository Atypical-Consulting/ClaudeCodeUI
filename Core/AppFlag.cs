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
