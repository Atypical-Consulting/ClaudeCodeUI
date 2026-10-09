## Persistance des sessions : c'est en place

J'ai ajouté `SessionStore` et branché le chargement dans `Home.razor`. Les **4 tests** passent ; voici ce qui a changé et ce qu'il reste à décider.

### Ce qui a changé

| Fichier | Changement | Lignes |
|:--|:--|--:|
| `SessionStore.cs` | nouveau, lecture/écriture JSON | +38 |
| `Components/Pages/Home.razor` | charge au démarrage, sauvegarde après chaque événement | +9 −2 |
| `.gitignore` | ignore `.claude-ui/` | +1 |

La sauvegarde est *débouncée* à 500 ms pour ne pas écrire à chaque token :

```csharp
readonly Timer saveTimer;

void ScheduleSave() =>
    saveTimer.Change(dueTime: 500, period: Timeout.Infinite);

async Task SaveNow()
{
    var json = JsonSerializer.Serialize(entries, JsonOpts);
    await File.WriteAllTextAsync(SessionStore.PathFor(cwd), json); // atomique ? voir plus bas
}
```

Pour vérifier de ton côté :

```bash
dotnet test --filter SessionStore
# Réussi : 4, Échec : 0, Ignoré : 0
```

> [!WARNING]
> `File.WriteAllTextAsync` n'est pas atomique : un crash pendant l'écriture laisse un JSON tronqué. Écrire dans un fichier temporaire puis faire `File.Move(tmp, path, overwrite: true)` règle le problème.

### Format sur disque

```json
{
  "version": 1,
  "cwd": "C:\repo\POC\ClaudeCodeUI",
  "mode": "default",
  "entries": [
    { "kind": "user", "body": "Ajoute la persistance des sessions…" },
    { "kind": "tool", "title": "Read  ClaudeSession.cs" }
  ]
}
```

### À décider

- [x] Un fichier par dossier de travail
- [x] Rechargement automatique au démarrage
- [ ] Garder l'historique de **plusieurs** sessions par dossier ?
  - soit un fichier par session (`session-<id>.json`)
  - soit un tableau dans `sessions.json`
- [ ] Limite de taille : couper au-delà de 5 Mo ?

> Les sorties d'outils volumineuses (lectures de fichiers) représentent ~80 % du poids. On peut ne garder que leur titre.

Le diff complet est dans l'inspecteur ; voir aussi la [doc System.Text.Json](https://learn.microsoft.com/dotnet/standard/serialization/system-text-json/overview).

---

Je m'arrête là. Tu veux que je rende l'écriture atomique maintenant ?
