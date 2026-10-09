namespace ClaudeCodeUI;

public sealed record ApiError(string Title, string Hint, string Raw);

// Maps the CLI's synthetic "API Error: ..." text to a friendly French message; Raw stays untouched.
public static class ApiErrors
{
    // First match wins, most specific first. Case-insensitive substring match on the raw text.
    static readonly (string[] Needles, string Title, string Hint)[] Table =
    [
        (["content filtering"], "La réponse a été bloquée par le filtre de contenu de l'API.", "Reformule la demande ou retire le contenu sensible, puis renvoie le message."),
        (["prompt is too long", "413"], "La conversation dépasse la taille de contexte du modèle.", "Lance `/compact` ou démarre une nouvelle session."),
        (["rate_limit", "429"], "Limite de requêtes atteinte.", "Patiente quelques instants avant de renvoyer le message."),
        (["overloaded", "529"], "L'API Claude est surchargée.", "Réessaie dans un moment."),
        (["authentication", "invalid api key", "oauth", "401", "403"], "L'authentification auprès de l'API a échoué.", "Reconnecte-toi avec `claude /login` dans un terminal."),
        (["timed out", "timeout", "connection error", "ECONNRESET", "fetch failed"], "Impossible de joindre l'API Claude.", "Vérifie la connexion réseau puis renvoie le message."),
        (["500", "api_error", "internal server error"], "L'API Claude a rencontré une erreur interne.", "Réessaie ; si l'erreur persiste, consulte status.anthropic.com."),
    ];

    public static ApiError? Parse(string text)
    {
        if (text is null || !text.TrimStart().StartsWith("API Error", StringComparison.Ordinal)) return null;
        foreach (var (needles, title, hint) in Table)
            if (needles.Any(n => text.Contains(n, StringComparison.OrdinalIgnoreCase))) return new(title, hint, text);
        return new("L'API Claude a renvoyé une erreur.", "Le détail brut est ci-dessous.", text);
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "ApiErrors: " + what);
        const string cf = "API Error: Output blocked by content filtering policy";
        Ok(Parse(cf) is { Title: var t, Raw: cf } && t.Contains("filtre de contenu"), "content filter");
        Ok(Parse("""API Error: 429 {"type":"error","error":{"type":"rate_limit_error"}}""") is { Title: "Limite de requêtes atteinte." }, "rate limit");
        Ok(Parse("API Error: something new") is { Title: "L'API Claude a renvoyé une erreur." }, "generic");
        Ok(Parse("Hello") is null && Parse("") is null, "non-errors");
        Ok(Parse("  API Error: 529 overloaded") is { Title: "L'API Claude est surchargée." }, "leading whitespace");
    }
}
