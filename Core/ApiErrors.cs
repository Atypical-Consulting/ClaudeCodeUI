using System.Text.RegularExpressions;

namespace ClaudeCodeUI;

// Kind picks the friendly text (resources ApiError.<Kind>.Title / .Hint), read in the UI culture when shown; Raw stays untouched.
public sealed record ApiError(string Kind, string Raw)
{
    public string Title => Strings.Get($"ApiError.{Kind}.Title");
    public string Hint => Strings.Get($"ApiError.{Kind}.Hint");
}

// Maps the CLI's synthetic "API Error: ..." text to a friendly message.
public static class ApiErrors
{
    // First match wins, most specific first. Case-insensitive substring match on the raw text.
    static readonly (string[] Needles, string Kind)[] Table =
    [
        (["content filtering"], "ContentFilter"),
        (["prompt is too long", "413"], "TooLong"),
        (["rate_limit", "429"], "RateLimit"),
        (["overloaded", "529"], "Overloaded"),
        (["authentication", "invalid api key", "oauth", "401", "403"], "Auth"),
        (["timed out", "timeout", "connection error", "ECONNRESET", "fetch failed"], "Network"),
        (["500", "api_error", "internal server error"], "Server"),
    ];

    public static ApiError? Parse(string text)
    {
        if (text is null || !text.TrimStart().StartsWith("API Error", StringComparison.Ordinal)) return null;
        foreach (var (needles, kind) in Table)
            if (needles.Any(n => Hit(text, n))) return new(kind, text);
        return new("Generic", text);
    }

    // Numeric status codes match as whole tokens ("prompt used 1500 tokens" is not a 500); text needles stay substrings.
    static bool Hit(string text, string n) =>
        n.All(char.IsDigit) ? Regex.IsMatch(text, $@"\b{n}\b") : text.Contains(n, StringComparison.OrdinalIgnoreCase);

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "ApiErrors: " + what);
        const string cf = "API Error: Output blocked by content filtering policy";
        Ok(Parse(cf) is { Title: var t, Raw: cf } && t.Contains("filtre de contenu"), "content filter");
        Ok(Parse("""API Error: 429 {"type":"error","error":{"type":"rate_limit_error"}}""") is { Title: "Limite de requêtes atteinte." }, "rate limit");
        Ok(Parse("API Error: something new") is { Title: "L’API Claude a renvoyé une erreur." }, "generic");
        Ok(Parse("Hello") is null && Parse("") is null, "non-errors");
        Ok(Parse("  API Error: 529 overloaded") is { Title: "L’API Claude est surchargée." }, "leading whitespace");
        Ok(Parse("API Error: prompt used 1500 tokens") is { Title: "L’API Claude a renvoyé une erreur." }, "1500 is not a 500");
        Ok(Parse("API Error: 500 oops") is { Kind: "Server" }, "bare status code still matches");
    }
}
