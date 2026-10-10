using System.ComponentModel;
using System.Diagnostics;
using System.Text;

namespace ClaudeCodeUI;

// The OS's own folder dialog, opened by the server: it is always local (loopback-only, token-gated), so its screen is the user's.
public static class FolderPicker
{
    // The picked folder, null on cancel or when ct fires (the component went away: the dialog closes with it).
    // Throws PlatformNotSupportedException when no dialog tool is installed (a bare Linux without zenity or kdialog).
    public static async Task<string?> Pick(string? start, string title, CancellationToken ct)
    {
        start = start is { Length: > 0 } && Path.IsPathFullyQualified(start) && Directory.Exists(start) ? start
            : Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        var os = OperatingSystem.IsMacOS() ? "mac" : OperatingSystem.IsWindows() ? "win" : "linux";
        foreach (var (exe, args) in Commands(os, start, title))
        {
            var psi = new ProcessStartInfo(exe) { RedirectStandardOutput = true, RedirectStandardError = true, CreateNoWindow = true, StandardOutputEncoding = Encoding.UTF8 };
            foreach (var a in args) psi.ArgumentList.Add(a);
            // PowerShell reads both from the environment: nothing user-typed is ever parsed as script.
            psi.Environment["CCUI_START"] = start;
            psi.Environment["CCUI_TITLE"] = title;
            Process p;
            try { p = Process.Start(psi)!; }
            catch (Win32Exception) { continue; }   // not installed: the next tool
            using (p)
            {
                var output = p.StandardOutput.ReadToEndAsync(CancellationToken.None);
                _ = p.StandardError.ReadToEndAsync(CancellationToken.None);   // drained, so a chatty GTK never blocks on a full pipe
                try { await p.WaitForExitAsync(ct); }
                catch (OperationCanceledException) { try { p.Kill(true); } catch (InvalidOperationException) { } return null; }
                return Parse(p.ExitCode, await output);
            }
        }
        throw new PlatformNotSupportedException("no folder dialog");
    }

    // Candidate tools, in order; pure so the self-check sees the exact arguments.
    internal static (string Exe, string[] Args)[] Commands(string os, string start, string title) => os switch
    {
        // Cancel exits 1 ("User canceled. (-128)" on stderr).
        "mac" => [("osascript", ["-e", $"POSIX path of (choose folder with prompt \"{Esc(title)}\" default location POSIX file \"{Esc(start)}\")"])],
        "win" => [("powershell", ["-NoProfile", "-STA", "-Command",
            "[Console]::OutputEncoding = [Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; " +
            "$d = New-Object System.Windows.Forms.FolderBrowserDialog; $d.Description = $env:CCUI_TITLE; $d.SelectedPath = $env:CCUI_START; " +
            "if ($d.ShowDialog() -eq 'OK') { $d.SelectedPath } else { exit 1 }"])],
        // The trailing / makes zenity open inside start rather than select it in its parent.
        _ => [("zenity", ["--file-selection", "--directory", "--title=" + title, "--filename=" + Path.TrimEndingDirectorySeparator(start) + "/"]),
              ("kdialog", ["--getexistingdirectory", start, "--title", title])],
    };

    // An AppleScript string literal: only \ and " are special.
    static string Esc(string s) => s.Replace(@"\", @"\\").Replace("\"", "\\\"");

    // stdout's path without its newline or trailing slash (osascript ends folders with /); null on cancel.
    internal static string? Parse(int exitCode, string stdout) =>
        exitCode == 0 && stdout.Trim('\r', '\n') is { Length: > 0 } p ? Path.TrimEndingDirectorySeparator(p) : null;

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "FolderPicker: " + what);
        Ok(Commands("mac", "/Users/a \"b\"", "Choisir")[0].Args[1] == "POSIX path of (choose folder with prompt \"Choisir\" default location POSIX file \"/Users/a \\\"b\\\"\")", "osascript quotes escaped");
        Ok(Commands("linux", "/home/a/", "T") is [(_, [.., "--filename=/home/a/"]), ("kdialog", _)], "zenity opens inside start, kdialog fallback");
        Ok(!Commands("win", "C:\\x", "T")[0].Args[3].Contains("C:\\x"), "PowerShell gets the path from the environment");
        Ok(Parse(0, "/Users/a/repo/\n") == "/Users/a/repo" && Parse(0, "/\n") == "/", "Parse trims, keeps root");
        Ok(Parse(1, "") is null && Parse(0, "\n") is null, "cancel is null");
    }
}
