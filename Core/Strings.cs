using System.Collections;
using System.Globalization;
using System.Resources;
using Microsoft.Extensions.Localization;
using Microsoft.Extensions.Logging.Abstractions;
using Microsoft.Extensions.Options;

namespace ClaudeCodeUI;

// UI text: Resources/Strings.resx (English, neutral) and Strings.fr.resx. Components inject IStringLocalizer<Strings>;
// Core code, static helpers included, reads the same resources here. Both follow CultureInfo.CurrentUICulture.
public sealed class Strings
{
    public static readonly string[] Cultures = ["en", "fr"];

    static readonly ResourceManager Rm = new("ClaudeCodeUI.Resources.Strings", typeof(Strings).Assembly);

    public static string Get(string key) => Rm.GetString(key, CultureInfo.CurrentUICulture) ?? key;

    public static string Get(string key, params object?[] args) => string.Format(CultureInfo.CurrentCulture, Get(key), args);

    // <key>1 for one, <key>N otherwise; <key>0 when it exists ("0 session" is singular in French, "0 sessions" in English).
    public static string Plural(int n, string key) =>
        Get(key + (n == 1 ? "1" : n == 0 && Rm.GetString(key + "0", CultureInfo.CurrentUICulture) is not null ? "0" : "N"), n);

    // A localized name for a CLI id (permission mode, effort level), else the id itself.
    public static string Name(string prefix, string? id) => id is null ? "" : Rm.GetString(prefix + id, CultureInfo.CurrentUICulture) ?? id;

    // Shortcut hints are written "Ctrl K" / "Alt N"; macOS shows ⌘ / ⌥ (app.js accepts metaKey). The server runs on the user's machine.
    public static string Kbd(string hint) => OperatingSystem.IsMacOS() ? hint.Replace("Ctrl", "⌘").Replace("Alt", "⌥") : hint;

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Strings: " + what);
        static HashSet<string> Keys(string culture) =>
            [.. Rm.GetResourceSet(CultureInfo.GetCultureInfo(culture), true, false)!.Cast<DictionaryEntry>().Select(e => (string)e.Key)];
        var en = Keys("");   // the neutral resx
        var fr = Keys("fr");
        Ok(en.Count > 100 && en.SetEquals(fr), $"fr and neutral resx differ: {string.Join(", ", en.Except(fr).Concat(fr.Except(en)))}");

        // French typography: no-break space before ? ! : ; » and after « (no wrap), typographic apostrophe.
        var badFr = Rm.GetResourceSet(CultureInfo.GetCultureInfo("fr"), true, false)!.Cast<DictionaryEntry>()
            .Where(e => System.Text.RegularExpressions.Regex.IsMatch(System.Text.RegularExpressions.Regex.Replace((string)e.Value!, "<code>.*?</code>|`[^`]*`|<[^>]*>", ""), " [?!:;»]|« |'"))
            .Select(e => (string)e.Key);
        Ok(!badFr.Any(), $"fr typography (no-break space, ’): {string.Join(", ", badFr)}");

        // IStringLocalizer<Strings> must find the same resources (ResourcesPath + type name).
        var l = new ResourceManagerStringLocalizerFactory(Options.Create(new LocalizationOptions { ResourcesPath = "Resources" }), NullLoggerFactory.Instance)
            .Create(typeof(Strings));
        var saved = CultureInfo.CurrentUICulture;
        try
        {
            CultureInfo.CurrentUICulture = CultureInfo.GetCultureInfo("en");
            Ok(l["Rail.NewSession"] is { ResourceNotFound: false, Value: "New session" } && Get("Wt.State.Safe") == "Safe", "en lookup");
            Ok(Plural(0, "Ov.Sessions") == "0 sessions" && Plural(1, "Ov.Sessions") == "1 session" && Plural(0, "Tools") == "0 tools", "en plural");
            Ok(Name("Mode.Name.", "acceptEdits") == "Accept edits" && Name("Mode.Name.", "bypassPermissions") == "bypassPermissions", "mode name");
            CultureInfo.CurrentUICulture = CultureInfo.GetCultureInfo("fr-BE");   // parent fallback
            Ok(l["Rail.NewSession"] is { ResourceNotFound: false, Value: "Nouvelle session" } && Get("Wt.State.Safe") == "Sûr", "fr lookup");
            Ok(Fmt.Tokens(999_600) == "1M" && Fmt.Tokens(999_400) == "999k", "Fmt.Tokens rounds to 1M");
            Ok(Plural(0, "Ov.Sessions") == "0 session" && Plural(2, "Ov.Sessions") == "2 sessions", "fr plural");
        }
        finally { CultureInfo.CurrentUICulture = saved; }
    }
}
