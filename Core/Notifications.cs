namespace ClaudeCodeUI;

// Desktop notifications (toggle on the Appearance page). MainLayout watches every session and, for a change worth one,
// asks app.js whether the user is looking. The browser then shows a Web Notification; the desktop shell, whose page has
// no IPC, gets a line on the server's stdout that main.rs posts natively (and turns a click into focus + navigation).
public static class Notifications
{
    public enum Kind { None, Decision, Done }

    public static bool ViaShell { get; set; }   // set by Program.cs under --desktop-port

    // Desktop: the server binds port 0, so the page's origin, and the localStorage that goes with it, changes with every
    // launch. The toggle lives in a file instead, next to the app's other state (WorktreeService's repos.json).
    internal static string OnFile = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "ClaudeCodeUI", "notify-on");

    public static bool ShellOn
    {
        get => File.Exists(OnFile);
        set
        {
            try
            {
                if (!value) { if (File.Exists(OnFile)) File.Delete(OnFile); return; }
                Directory.CreateDirectory(Path.GetDirectoryName(OnFile)!);
                File.WriteAllText(OnFile, "");
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException) { }   // the getter shows what stuck
        }
    }

    // Between two observations of a session (before = null: first seen). A decision is a can_use_tool, whatever the tool
    // (permission, AskUserQuestion, ExitPlanMode: --probe-cli notify); a turn is done when a new result leaves it Idle
    // (a queued message keeps it Running). An interrupt is the user's own doing.
    public static Kind Between(SessionStatus? before, DateTimeOffset? resultBefore, SessionStatus status, DateTimeOffset? result, string? subtype) =>
        status == SessionStatus.Waiting && before != SessionStatus.Waiting ? Kind.Decision
        : before is not null && status == SessionStatus.Idle && result != resultBefore && subtype != "interrompu" ? Kind.Done
        : Kind.None;

    // subtype: the result's (success, error_max_turns, error_during_execution…).
    public static string Body(Kind kind, string? tool, TimeSpan? turn, string? subtype = null) => kind == Kind.Done
        ? Strings.Get(subtype?.StartsWith("error") == true ? "Notify.Failed" : "Notify.Done", Fmt.Dur(turn ?? TimeSpan.Zero))
        : tool switch
        {
            "AskUserQuestion" => Strings.Get("Notify.Question"),
            "ExitPlanMode" => Strings.Get("Notify.Plan"),
            null => Strings.Get("Notify.Decision"),
            _ => Strings.Get("Notify.Permission", tool),
        };

    // One stdout line per notification for the desktop shell: tag, session id (a click opens it), title, body, tab-separated.
    public static string ShellLine(string id, string title, string body) => $"ccui-notify\t{Flat(id)}\t{Flat(title)}\t{Flat(body)}";

    public static void ToShell(string id, string title, string body) => Console.Out.WriteLine(ShellLine(id, title, body));

    static string Flat(string s) => s.ReplaceLineEndings(" ").Replace('\t', ' ');

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Notifications: " + what);
        DateTimeOffset t1 = DateTimeOffset.UnixEpoch, t2 = t1.AddSeconds(5);
        Ok(Between(SessionStatus.Running, null, SessionStatus.Waiting, null, null) == Kind.Decision, "running → waiting");
        Ok(Between(null, null, SessionStatus.Waiting, null, null) == Kind.Decision, "first seen waiting");
        Ok(Between(SessionStatus.Waiting, null, SessionStatus.Waiting, null, null) == Kind.None, "still waiting");
        Ok(Between(SessionStatus.Running, t1, SessionStatus.Idle, t2, "success") == Kind.Done, "turn done");
        Ok(Between(SessionStatus.Waiting, t1, SessionStatus.Idle, t2, "error_during_execution") == Kind.Done, "turn ended on an error");
        Ok(Between(SessionStatus.Running, t1, SessionStatus.Idle, t2, "interrompu") == Kind.None, "interrupt");
        Ok(Between(SessionStatus.Running, t1, SessionStatus.Running, t2, "success") == Kind.None, "queued turn runs next");
        Ok(Between(SessionStatus.Idle, t2, SessionStatus.Idle, t2, "success") == Kind.None, "no new result");
        Ok(Between(null, null, SessionStatus.Idle, t2, "success") == Kind.None, "first seen idle");
        Ok(Body(Kind.Decision, "Bash", null) == "Attend votre autorisation : Bash" && Body(Kind.Decision, "ExitPlanMode", null) == "Plan prêt à valider"
           && Body(Kind.Done, null, TimeSpan.FromSeconds(2.4), "success") == "Tour terminé en 2,4 s"
           && Body(Kind.Done, null, TimeSpan.FromSeconds(2.4), "error_max_turns") == "Tour arrêté sur une erreur après 2,4 s", "body text");
        Ok(ShellLine("s-1", "a\tb", "x\r\ny\nz") == "ccui-notify\ts-1\ta b\tx y z", "shell line is one line, four fields");

        var saved = OnFile;
        OnFile = Path.Combine(Path.GetTempPath(), "ccui-check-" + Guid.NewGuid().ToString("N"), "notify-on");
        try
        {
            Ok(!ShellOn, "desktop toggle off by default");
            ShellOn = true;
            Ok(ShellOn, "desktop toggle remembered on");
            ShellOn = false;
            ShellOn = false;
            Ok(!ShellOn, "desktop toggle back off (twice is fine)");
        }
        finally
        {
            if (Directory.Exists(Path.GetDirectoryName(OnFile))) Directory.Delete(Path.GetDirectoryName(OnFile)!, true);
            OnFile = saved;
        }
    }
}
