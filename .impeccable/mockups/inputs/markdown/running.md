La session vit entièrement dans `Home.razor` : la liste `entries` est en mémoire et disparaît au rechargement. Je propose un petit **SessionStore** statique, sans dépendance :

```csharp
static class SessionStore
{
    static string PathFor(string cwd) =>
        Path.Combine(cwd, ".claude-ui", "session.json");

    // Load / Save sérialisent List<Entry> avec System.Text.Json
    public static List<Entry> Load(string cwd) =>
        File.Exists(PathFor(cwd))
            ? JsonSerializer.Deserialize<List<Entry>>(File.ReadAllText(PathFor(cwd))) ?? []
            : [];
}
```

### Ensuite

1. Charger au démarrage de la page, sauvegarder après chaque événement.
2. Ignorer `.claude-ui/` dans git
