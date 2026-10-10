using System.Globalization;

namespace ClaudeCodeUI;

// Numbers and units follow the UI culture (fr "1,5 Mo", en "1.5 MB"); costs stay "$0.0391" (invariant).
public static class Fmt
{
    static CultureInfo C => CultureInfo.CurrentCulture;

    public static string Dur(TimeSpan t) =>
        t.TotalSeconds < 60 ? Strings.Get("Fmt.Seconds", t.TotalSeconds.ToString("0.#", C))
        : t.TotalHours >= 1 ? Strings.Get("Fmt.HoursMinutes", (int)t.TotalHours, t.Minutes.ToString("00"))
        : t.Seconds == 0 ? Strings.Get("Fmt.Minutes", t.Minutes) : Strings.Get("Fmt.MinutesSeconds", t.Minutes, t.Seconds.ToString("00"));

    public static string Size(long bytes) => bytes switch
    {
        >= 1L << 30 => Strings.Get("Fmt.GB", (bytes / (double)(1L << 30)).ToString("0.00", C)),
        >= 1L << 20 => Strings.Get("Fmt.MB", (bytes / (double)(1L << 20)).ToString("0.#", C)),
        >= 1L << 10 => Strings.Get("Fmt.KB", bytes >> 10),
        _ => Strings.Get("Fmt.Bytes", bytes),
    };

    public static string Pct(double fraction) => Strings.Get("Fmt.Pct", Math.Round(fraction * 100));   // 0..1

    public static string Tokens(long n) => n switch
    {
        >= 999_500 =>$"{(n / 1_000_000.0).ToString("0.#", C)}M",
        >= 1_000 => $"{Math.Round(n / 1_000.0)}k",
        _ => n.ToString(),
    };

    public static string Cost(decimal usd, int decimals) => "$" + usd.ToString("F" + decimals, CultureInfo.InvariantCulture);

    public static string Time(DateTimeOffset t) => t.ToLocalTime().ToString("HH:mm", CultureInfo.InvariantCulture);

    public static string Enc(string? s) => System.Text.Encodings.Web.HtmlEncoder.Default.Encode(s ?? "");

    public static string Home(string path)
    {
        var home = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile).Replace('\\', '/');
        return path.StartsWith(home, StringComparison.OrdinalIgnoreCase) ? "~" + path[home.Length..] : path;
    }
}
