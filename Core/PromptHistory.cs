namespace ClaudeCodeUI;

// Composer ↑ ↓ recall. Entries are newest first; Index -1 is the draft being typed, kept while browsing.
public sealed class PromptHistory
{
    List<string> entries = [];
    string draft = "";

    public int Index { get; private set; } = -1;
    public int Count => entries.Count;
    // The text typed before the walk, while a past prompt is shown; null when not walking.
    public string? Draft => Index >= 0 ? draft : null;

    // newestFirst: this session's prompts, then the past sessions'. Exact duplicates keep their newest place.
    public void Load(IEnumerable<string> newestFirst)
    {
        entries = [.. newestFirst.Where(s => !string.IsNullOrWhiteSpace(s)).Distinct()];
        Index = -1;
    }

    // step +1 = older (↑), -1 = newer (↓). Null when there is nothing further that way.
    public string? Move(int step, string current)
    {
        var next = Index + step;
        if (next < -1 || next >= entries.Count) return null;
        if (Index == -1) draft = current;
        Index = next;
        return Index == -1 ? draft : entries[Index];
    }

    // The text was edited or sent: it is the draft again.
    public void Reset() => Index = -1;

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "PromptHistory: " + what);
        var h = new PromptHistory();
        Ok(h.Move(1, "x") is null && h.Index == -1, "empty history");
        h.Load(["b", "a", "b", " ", "c"]);
        Ok(h.Count == 3, "dedup + blanks");
        Ok(h.Move(-1, "draft") is null, "↓ at the draft");
        Ok(h.Move(1, "draft") == "b" && h.Move(1, "b") == "a" && h.Move(1, "a") == "c", "↑ walks newest to oldest");
        Ok(h.Move(1, "c") is null && h.Index == 2, "↑ stops at the oldest");
        Ok(h.Draft == "draft", "the draft is kept while walking");
        Ok(h.Move(-1, "c") == "a" && h.Move(-1, "a") == "b" && h.Move(-1, "b") == "draft", "↓ comes back to the draft");
        Ok(h.Draft is null, "no draft outside a walk");
        h.Move(1, "draft 2");
        h.Reset();
        Ok(h.Move(1, "edited") == "b" && h.Move(-1, "b") == "edited", "an edit becomes the draft");

        var t = TranscriptStore.PromptsIn([
            """{"type":"user","message":{"role":"user","content":"first"},"promptId":"p1"}""",
            """{"type":"user","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"ok"}]}}""",
            """{"type":"user","isMeta":true,"message":{"role":"user","content":"meta"}}""",
            """{"type":"user","isSidechain":true,"message":{"role":"user","content":"agent prompt"}}""",
            """{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}""",
            """{"type":"user","message":{"role":"user","content":"<command-name>/compact</command-name>\n<command-args></command-args>"}}""",
            """{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"reply"}]}}""",
            """{"type":"user","message":{"role":"user","content":"second"},"promptId":"p2"}""",
        ]).ToList();
        Ok(t is ["first", "/compact", "second"], "PromptsIn keeps typed prompts: " + string.Join("|", t));
    }
}
