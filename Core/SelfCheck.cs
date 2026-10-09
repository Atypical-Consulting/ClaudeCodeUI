namespace ClaudeCodeUI;

// `dotnet run -- --self-check`: runs every Check() and exits non-zero on the first failure.
public static class SelfCheck
{
    public static int Run()
    {
        // The asserts below compare the French UI text.
        System.Globalization.CultureInfo.CurrentCulture = System.Globalization.CultureInfo.CurrentUICulture = System.Globalization.CultureInfo.GetCultureInfo("fr");
        try
        {
            Strings.Check();
            ProcessJob.Check();
            Events.Check();
            ApiErrors.Check();
            LiveSession.Check();
            Md.Check();
            EditDiff.Check();
            WorktreeService.Check();
            TranscriptStore.Check();
            ListNav.Check();
            Console.WriteLine("self-check OK");
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"self-check ÉCHEC : {ex.Message}");
            return 1;
        }
    }

    internal static void Assert(bool ok, string what)
    {
        if (!ok) throw new Exception(what);
    }
}
