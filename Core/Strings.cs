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

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Strings: " + what);
        static HashSet<string> Keys(string culture) =>
            [.. Rm.GetResourceSet(CultureInfo.GetCultureInfo(culture), true, false)!.Cast<DictionaryEntry>().Select(e => (string)e.Key)];
        var en = Keys("");   // the neutral resx
        var fr = Keys("fr");
        Ok(en.Count > 100 && en.SetEquals(fr), $"fr and neutral resx differ: {string.Join(", ", en.Except(fr).Concat(fr.Except(en)))}");

        // IStringLocalizer<Strings> must find the same resources (ResourcesPath + type name).
        var l = new ResourceManagerStringLocalizerFactory(Options.Create(new LocalizationOptions { ResourcesPath = "Resources" }), NullLoggerFactory.Instance)
            .Create(typeof(Strings));
        var saved = CultureInfo.CurrentUICulture;
        try
        {
            CultureInfo.CurrentUICulture = CultureInfo.GetCultureInfo("en");
            Ok(l["Rail.NewSession"] is { ResourceNotFound: false, Value: "New session" } && Get("Wt.State.Safe") == "Safe", "en lookup");
            CultureInfo.CurrentUICulture = CultureInfo.GetCultureInfo("fr-BE");   // parent fallback
            Ok(l["Rail.NewSession"] is { ResourceNotFound: false, Value: "Nouvelle session" } && Get("Wt.State.Safe") == "Sûr", "fr lookup");
        }
        finally { CultureInfo.CurrentUICulture = saved; }
    }
}
