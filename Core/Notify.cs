namespace ClaudeCodeUI;

// Desktop notifications (toggle on the Appearance page). MainLayout watches every session and, for a change worth one,
// asks app.js whether the window is in the background or another session is on screen. The browser then shows a Web
// Notification; the desktop shell, whose page has no IPC, gets a line on the server's stdout that main.rs posts natively.
public static class Notify
{
    public enum Kind { None, Decision, Done }

    public static bool ViaShell { get; set; }   // set by Program.cs under --desktop-port

    // Between two observations of a session (before = null: first seen). A decision is a can_use_tool, whatever the tool
    // (permission, AskUserQuestion, ExitPlanMode: --probe-cli notify); a turn is done when a new result leaves it Idle
    // (a queued message keeps it Running). An interrupt is the user's own doing.
    public static Kind Between(SessionStatus? before, DateTimeOffset? resultBefore, SessionStatus status, DateTimeOffset? result, string? subtype) =>
        status == SessionStatus.Waiting && before != SessionStatus.Waiting ? Kind.Decision
        : before is not null && status == SessionStatus.Idle && result != resultBefore && subtype != "interrompu" ? Kind.Done
        : Kind.None;

    public static string Body(Kind kind, string? tool, TimeSpan? turn) => kind == Kind.Done
        ? Strings.Get("Notify.Done", Fmt.Dur(turn ?? TimeSpan.Zero))
        : tool switch
        {
            "AskUserQuestion" => Strings.Get("Notify.Question"),
            "ExitPlanMode" => Strings.Get("Notify.Plan"),
            null => Strings.Get("Notify.Decision"),
            _ => Strings.Get("Notify.Permission", tool),
        };

    // One stdout line per notification for the desktop shell: tag, title, body, tab-separated.
    public static string ShellLine(string title, string body) => $"ccui-notify\t{Flat(title)}\t{Flat(body)}";

    public static void ToShell(string title, string body) => Console.Out.WriteLine(ShellLine(title, body));

    static string Flat(string s) => s.ReplaceLineEndings(" ").Replace('\t', ' ');

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Notify: " + what);
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
           && Body(Kind.Done, null, TimeSpan.FromSeconds(2.4)) == "Tour terminé en 2,4 s", "body text");
        Ok(ShellLine("a\tb", "x\r\ny\nz") == "ccui-notify\ta b\tx y z", "shell line is one line, three fields");
    }
}
