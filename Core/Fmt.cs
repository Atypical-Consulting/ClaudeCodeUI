using System.Globalization;

namespace ClaudeCodeUI;

// French number formats for the UI; costs stay "$0.0391" (invariant).
public static class Fmt
{
    static readonly CultureInfo Fr = CultureInfo.GetCultureInfo("fr-FR");

    public static string Dur(TimeSpan t) =>
        t.TotalSeconds < 60 ? $"{t.TotalSeconds.ToString("0.#", Fr)} s"
        : t.TotalHours >= 1 ? $"{(int)t.TotalHours} h {t.Minutes:00}"
        : t.Seconds == 0 ? $"{t.Minutes} min" : $"{t.Minutes} min {t.Seconds:00}";

    public static string Size(long bytes) => bytes switch
    {
        >= 1L << 30 => $"{(bytes / (double)(1L << 30)).ToString("0.00", Fr)} Go",
        >= 1L << 20 => $"{(bytes / (double)(1L << 20)).ToString("0.#", Fr)} Mo",
        >= 1L << 10 => $"{bytes >> 10} Ko",
        _ => $"{bytes} o",
    };

    public static string Pct(double fraction) => $"{Math.Round(fraction * 100)} %";   // 0..1

    public static string Tokens(long n) => n switch
    {
        >= 1_000_000 => $"{(n / 1_000_000.0).ToString("0.#", Fr)}M",
        >= 1_000 => $"{Math.Round(n / 1_000.0)}k",
        _ => n.ToString(),
    };

    public static string Cost(decimal usd, int decimals) => "$" + usd.ToString("F" + decimals, CultureInfo.InvariantCulture);

    public static string Time(DateTimeOffset t) => t.ToLocalTime().ToString("HH:mm", Fr);
}
