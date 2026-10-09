using System.Text.Json;

namespace ClaudeCodeUI;

public enum DiffKind { Ctx, Add, Del, Hunk, More }
public record DiffLine(DiffKind Kind, int? N, string Text);   // N: old number for Del, new number otherwise; More: occurrences not shown

// Diff lines for Edit/Write/MultiEdit: before approval (from the tool input) or after (structuredPatch).
// No LCS: the edited block is cut to whole lines, common leading/trailing lines become context.
public static class EditDiff
{
    const int Context = 2;
    const long MaxRead = 2_000_000;   // bytes: a bigger file is diffed without line numbers
    const int MaxOccurrences = 20;    // replace_all: each hunk rescans the whole file, so only the first ones are previewed

    public static IReadOnlyList<DiffLine> FromInput(string tool, JsonElement input)
    {
        var path = Events.Str(input, "file_path") ?? "";
        var file = Read(path);
        switch (tool)
        {
            case "Write":
                var content = Norm(Events.Str(input, "content") ?? "");
                if (file is null) return File.Exists(path) ? Edits(null, [("", content)]) : Created(content);   // unreadable: not a new file
                return Edits(file, [(file, content)]);
            case "MultiEdit" when Events.Prop(input, "edits") is { ValueKind: JsonValueKind.Array } edits:
                return Edits(file, edits.EnumerateArray().Select(e => (Norm(Events.Str(e, "old_string") ?? ""), Norm(Events.Str(e, "new_string") ?? ""))));
            default:
                var (old, @new) = (Norm(Events.Str(input, "old_string") ?? ""), Norm(Events.Str(input, "new_string") ?? ""));
                if (Events.Prop(input, "replace_all") is not { ValueKind: JsonValueKind.True } || file is null || old.Length == 0) return Edits(file, [(old, @new)]);
                var count = Math.Max(1, file.Split(old).Length - 1);
                var res = Edits(file, Enumerable.Repeat((old, @new), Math.Min(count, MaxOccurrences)), true);   // one hunk per occurrence
                if (count > MaxOccurrences) res.Add(new(DiffKind.More, count - MaxOccurrences, ""));
                return res;
        }
    }

    // tool_use_result.structuredPatch: [{oldStart,oldLines,newStart,newLines,lines:["-a","+b"," c"]}]
    public static IReadOnlyList<DiffLine> FromPatch(JsonElement structuredPatch)
    {
        var res = new List<DiffLine>();
        if (structuredPatch.ValueKind != JsonValueKind.Array) return res;
        foreach (var h in structuredPatch.EnumerateArray())
        {
            int o = Int(h, "oldStart"), n = Int(h, "newStart");
            res.Add(new(DiffKind.Hunk, null, $"@@ -{o},{Int(h, "oldLines")} +{n},{Int(h, "newLines")} @@"));
            if (Events.Prop(h, "lines") is not { ValueKind: JsonValueKind.Array } lines) continue;
            foreach (var l in lines.EnumerateArray().Select(x => x.GetString() ?? ""))
                switch (l.Length > 0 ? l[0] : ' ')
                {
                    case '-': res.Add(new(DiffKind.Del, o++, l[1..])); break;
                    case '+': res.Add(new(DiffKind.Add, n++, l[1..])); break;
                    case '\\': break;   // "\ No newline at end of file"
                    default: res.Add(new(DiffKind.Ctx, n++, l.Length > 0 ? l[1..] : "")); o++; break;
                }
        }
        return res;
    }

    // A new file: every line added.
    public static IReadOnlyList<DiffLine> Created(string content)
    {
        var lines = Lines(Norm(content));
        return [new(DiffKind.Hunk, null, $"@@ -0,0 +1,{lines.Length} @@"), .. lines.Select((l, i) => new DiffLine(DiffKind.Add, i + 1, l))];
    }

    // Successive replacements on one file. Each hunk is located in the text as already edited by the previous ones
    // (new numbering); its old start is shifted back by the lines added so far. When the file is missing or the
    // text is not found (already applied, or edited since), the block is shown without numbers.
    // replaceAll: each edit searches after the previous replacement.
    static List<DiffLine> Edits(string? file, IEnumerable<(string Old, string New)> edits, bool replaceAll = false)
    {
        var res = new List<DiffLine>();
        var text = file;
        int shift = 0, from = 0;
        foreach (var (old, @new) in edits)
        {
            if (!replaceAll) from = 0;
            var pos = text is null ? -1 : old.Length == 0 ? 0 : text.IndexOf(old, from, StringComparison.Ordinal);
            if (pos < 0 && text is not null && @new.Length > 0 && text.IndexOf(@new, from, StringComparison.Ordinal) is >= 0 and var p)
            {
                text = text[..p] + old + text[(p + @new.Length)..];   // already applied: rebuild the text before the edit
                pos = p;
            }
            if (pos < 0 || text is null)
            {
                var (o, n) = (Lines(old), Lines(@new));
                res.AddRange(o.Select(l => new DiffLine(DiffKind.Del, null, l)));
                res.AddRange(n.Select(l => new DiffLine(DiffKind.Add, null, l)));
                continue;
            }

            // Whole lines around the replaced text, before and after.
            var all = Lines(text);
            int end = pos + old.Length;
            int blockStart = pos == 0 ? 0 : text.LastIndexOf('\n', pos - 1) + 1;
            int blockEnd = text.IndexOf('\n', end) is >= 0 and var nl ? nl : text.Length;
            var edited = text[..pos] + @new + text[end..];
            var oldBlock = Lines(text[blockStart..blockEnd]);
            var newBlock = Lines(edited[blockStart..(blockEnd + @new.Length - old.Length)]);
            int first = text[..blockStart].Count(c => c == '\n'), last = first + oldBlock.Length - 1;

            int pre = 0;
            while (pre < oldBlock.Length && pre < newBlock.Length && oldBlock[pre] == newBlock[pre]) pre++;
            int post = 0;
            while (post < oldBlock.Length - pre && post < newBlock.Length - pre && oldBlock[^(post + 1)] == newBlock[^(post + 1)]) post++;

            int delFrom = first + pre, delTo = last - post;                 // inclusive, 0-based, old text
            int ctxFrom = Math.Max(0, delFrom - Context);
            int addCount = newBlock.Length - pre - post;
            int ctxAfter = Math.Min(Context, all.Length - 1 - delTo);
            int oldLen = (delTo - ctxFrom + 1) + ctxAfter, newLen = (delFrom - ctxFrom) + addCount + ctxAfter;
            res.Add(new(DiffKind.Hunk, null, $"@@ -{ctxFrom + 1 - shift},{oldLen} +{ctxFrom + 1},{newLen} @@"));
            for (int i = ctxFrom; i < delFrom; i++) res.Add(new(DiffKind.Ctx, i + 1, all[i]));
            for (int i = delFrom; i <= delTo; i++) res.Add(new(DiffKind.Del, i + 1 - shift, all[i]));
            for (int i = 0; i < addCount; i++) res.Add(new(DiffKind.Add, delFrom + i + 1, newBlock[pre + i]));
            int delta = addCount - (delTo - delFrom + 1);
            for (int i = 1; i <= ctxAfter; i++) res.Add(new(DiffKind.Ctx, delTo + i + 1 + delta, all[delTo + i]));
            shift += delta;
            text = edited;
            from = pos + @new.Length;
        }
        return res;
    }

    static string? Read(string path)
    {
        try
        {
            if (path.Length == 0 || new FileInfo(path) is not { Exists: true } fi || fi.Length > MaxRead) return null;
            // Devices and FIFOs (/dev/zero, a pipe) report length 0 and would block or never end: never open them.
            // A regular empty file reads as "" anyway.
            return fi.Length == 0 ? "" : Norm(File.ReadAllText(path));
        }
        catch (IOException) { return null; }
        catch (ArgumentException) { return null; }
        catch (UnauthorizedAccessException) { return null; }
    }

    static string Norm(string s) => s.Replace("\r\n", "\n");
    static string[] Lines(string s) => s.Length == 0 ? [] : (s.EndsWith('\n') ? s[..^1] : s).Split('\n');
    static int Int(JsonElement e, string name) => Events.Prop(e, name) is { ValueKind: JsonValueKind.Number } n ? n.GetInt32() : 0;

    // highlight.js language of a file; "" when unknown.
    public static string Lang(string path) => Path.GetExtension(path).ToLowerInvariant() switch
    {
        ".cs" or ".razor" or ".cshtml" => "csharp",
        ".js" or ".mjs" or ".cjs" => "javascript",
        ".ts" or ".tsx" => "typescript",
        ".json" => "json",
        ".css" => "css",
        ".html" or ".htm" or ".xml" or ".csproj" or ".props" or ".targets" or ".svg" => "xml",
        ".md" => "markdown",
        ".sh" => "bash",
        ".ps1" or ".psm1" => "powershell",
        ".yml" or ".yaml" => "yaml",
        ".py" => "python",
        ".sql" => "sql",
        _ => "",
    };

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "EditDiff: " + what);
        static JsonElement J(string json) => JsonDocument.Parse(json).RootElement.Clone();
        static string Show(IEnumerable<DiffLine> d) => string.Join("|", d.Select(l => $"{l.Kind}{l.N}:{l.Text}"));
        var dir = Path.Combine(Path.GetTempPath(), "claude-ui-check-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(dir);
        try
        {
            var f = Path.Combine(dir, "a.txt").Replace("\\", "\\\\");
            File.WriteAllText(Path.Combine(dir, "a.txt"), "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9\r\n10\r\n");

            var mid = FromInput("Edit", J($$"""{"file_path":"{{f}}","old_string":"5","new_string":"cinq"}"""));
            Ok(Show(mid) == "Hunk:@@ -3,5 +3,5 @@|Ctx3:3|Ctx4:4|Del5:5|Add5:cinq|Ctx6:6|Ctx7:7", "one line replaced in the middle: " + Show(mid));

            var add = FromInput("Edit", J($$"""{"file_path":"{{f}}","old_string":"2\n","new_string":"2\nbis\n"}"""));
            Ok(Show(add) == "Hunk:@@ -1,4 +1,5 @@|Ctx1:1|Ctx2:2|Add3:bis|Ctx4:3|Ctx5:4", "pure addition: " + Show(add));

            var created = FromInput("Write", J($$"""{"file_path":"{{f}}.absent","content":"x\ny\n"}"""));
            Ok(Show(created) == "Hunk:@@ -0,0 +1,2 @@|Add1:x|Add2:y", "Write on a missing file: " + Show(created));

            var multi = FromInput("MultiEdit", J($$"""{"file_path":"{{f}}","edits":[{"old_string":"2","new_string":"deux\ndeux bis"},{"old_string":"9","new_string":"neuf"}]}"""));
            Ok(multi.Count(l => l.Kind == DiffKind.Hunk) == 2 && Show(multi).Contains("Del2:2|Add2:deux|Add3:deux bis") && Show(multi).Contains("Hunk:@@ -7,4 +8,4 @@|Ctx8:7|Ctx9:8|Del9:9|Add10:neuf"),
               "MultiEdit, two hunks: " + Show(multi));

            var every = FromInput("Edit", J($$"""{"file_path":"{{f}}","old_string":"1","new_string":"1x","replace_all":true}"""));
            Ok(every.Count(l => l.Kind == DiffKind.Add) == 2 && Show(every).Contains("Add1:1x") && Show(every).Contains("Add10:1x0"), "replace_all, every occurrence: " + Show(every));

            File.WriteAllText(Path.Combine(dir, "many.txt"), string.Concat(Enumerable.Repeat("x\n", MaxOccurrences + 5)));
            var many = Path.Combine(dir, "many.txt").Replace("\\", "\\\\");
            var capped = FromInput("Edit", J($$"""{"file_path":"{{many}}","old_string":"x","new_string":"y","replace_all":true}"""));
            Ok(capped.Count(l => l.Kind == DiffKind.Hunk) == MaxOccurrences && capped[^1] is { Kind: DiffKind.More, N: 5 }, "replace_all, capped occurrences: " + Show(capped));

            File.WriteAllText(Path.Combine(dir, "big.txt"), "a\n" + new string('z', (int)MaxRead));
            var big = Path.Combine(dir, "big.txt").Replace("\\", "\\\\");
            var bigEdit = FromInput("Edit", J($$"""{"file_path":"{{big}}","old_string":"a","new_string":"b"}"""));
            Ok(Show(bigEdit) == "Del:a|Add:b", "file too large, block without numbers: " + Show(bigEdit));
            var bigWrite = FromInput("Write", J($$"""{"file_path":"{{big}}","content":"c"}"""));
            Ok(Show(bigWrite) == "Add:c", "Write over a file too large is not a creation: " + Show(bigWrite));

            var gone = FromInput("Edit", J("""{"file_path":"C:\\nope\\x.cs","old_string":"a","new_string":"b"}"""));
            Ok(Show(gone) == "Del:a|Add:b", "missing file: " + Show(gone));

            // Protocol capture (s2.jsonl): Edit a.txt hello -> bye. Before approval the same +/- lines as structuredPatch after.
            File.WriteAllText(Path.Combine(dir, "h.txt"), "hello\n");
            var h = Path.Combine(dir, "h.txt").Replace("\\", "\\\\");
            var patch = FromPatch(J("""[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-hello","+bye"]}]"""));
            Ok(Show(patch) == "Hunk:@@ -1,1 +1,1 @@|Del1:hello|Add1:bye", "FromPatch: " + Show(patch));
            var before = FromInput("Edit", J($$"""{"file_path":"{{h}}","old_string":"hello","new_string":"bye"}"""));
            Ok(Show(before) == Show(patch), "FromInput matches structuredPatch: " + Show(before));
            File.WriteAllText(Path.Combine(dir, "h.txt"), "bye\n");   // replay after the edit was applied
            Ok(Show(FromInput("Edit", J($$"""{"file_path":"{{h}}","old_string":"hello","new_string":"bye"}"""))) == Show(patch), "FromInput after the fact");

            var ctxPatch = FromPatch(J("""[{"oldStart":3,"oldLines":3,"newStart":3,"newLines":4,"lines":[" a","-b","+B","+C"," d","\\ No newline at end of file"]}]"""));
            Ok(Show(ctxPatch) == "Hunk:@@ -3,3 +3,4 @@|Ctx3:a|Del4:b|Add4:B|Add5:C|Ctx6:d", "FromPatch with context: " + Show(ctxPatch));
        }
        finally { Directory.Delete(dir, true); }
    }
}
