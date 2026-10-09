namespace ClaudeCodeUI;

// The Tauri desktop shell (desktop/src-tauri) reads our stdout: after the startup URL, this one line asks it to check
// for updates. No IPC into the webview, so the page (which renders model output) gets no handle on the shell.
public static class Shell
{
    public const string CheckForUpdatesLine = "ccui:check-for-updates";

    // Set by Program in desktop mode (--desktop-port); plain `dotnet run` has no shell to ask.
    public static bool Desktop { get; set; }

    public static string Version { get; } = typeof(Shell).Assembly.GetName().Version?.ToString(3) ?? "";

    public static void CheckForUpdates() => Console.WriteLine(CheckForUpdatesLine);
}
