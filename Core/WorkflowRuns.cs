using System.Collections.Concurrent;
using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;

namespace ClaudeCodeUI;

public enum WfState { Waiting, Running, Done, Failed, Unknown }   // Unknown: no end recorded (the transcript stops mid-run)

public sealed record WfAgent(string Label, string Phase, WfState State, string? Model, long Tokens, bool Approx, int ToolCalls,
                             TimeSpan? Duration, string? AgentId, string? Result, string? RawState = null);   // Approx: rebuilt from agent-*.jsonl

public sealed record WfPhase(string Title, string? Detail, int Total, int Done, int Running, int Failed)
{
    public WfState State => Failed > 0 ? WfState.Failed : Running > 0 || Done > 0 && Done < Total ? WfState.Running
        : Total > 0 && Done == Total ? WfState.Done : WfState.Waiting;
}

public sealed record WorkflowRun(string Name, string? Description, string? RunDir, IReadOnlyList<WfPhase> Phases, IReadOnlyList<WfAgent> Agents,
                                 long Tokens, int ToolCalls, TimeSpan Elapsed, WfState State, string? RawStatus, string? Model, bool Live)
{
    public int Done => Agents.Count(a => a.State == WfState.Done);
}

// An ultracode Workflow run (tool "Workflow"), shaped like the CLI's own view. Sources, best first (all observed on claude
// 2.1.295/296, see --probe-cli workflow): <session>/workflows/<runId>.json, written at completion only; the live snapshot
// system/task_progress.workflow_progress (ToolItem.Progress); while running with no snapshot (a transcript opened from
// disk), subagents/workflows/<runId>/journal.jsonl + agent-<id>.jsonl, read incrementally. The run folder comes from the
// launch receipt (tool_use_result.transcriptDir, or the "Transcript dir:" line), never built from ids.
public static class WorkflowRuns
{
    public const string Tool = "Workflow";

    // Per ToolItem instance (a record: a new one on every change), kept 500 ms: the thread re-renders on every streamed
    // delta and each card or panel asks twice per render, which would otherwise stat and tail the run folder each time.
    static readonly System.Runtime.CompilerServices.ConditionalWeakTable<ToolItem, Tuple<DateTimeOffset, WorkflowRun>> recent = new();

    public static WorkflowRun Of(ToolItem t)
    {
        var now = DateTimeOffset.Now;
        if (recent.TryGetValue(t, out var c) && now - c.Item1 < TimeSpan.FromMilliseconds(500)) return c.Item2;
        var run = Read(t, now);
        recent.AddOrUpdate(t, Tuple.Create(now, run));
        return run;
    }

    static WorkflowRun Read(ToolItem t, DateTimeOffset now)
    {
        var receipt = t.Structured ?? default;
        var script = Events.Str(t.Input, "script");
        var dir = Inside(Events.Str(receipt, "transcriptDir") ?? Line(t.ResultText, "Transcript dir:"));
        var final = dir is null ? null : Final(dir);

        var status = final?.Status ?? t.TaskStatus;   // the .json, else the task-notification
        var state = status switch
        {
            "completed" => WfState.Done,
            { } => WfState.Failed,   // never observed: the raw word is shown
            null => t.State switch
            {
                ToolState.Running => WfState.Running,
                ToolState.Waiting => WfState.Waiting,
                ToolState.Done => WfState.Done,
                ToolState.Error when t.ResultText is null => WfState.Unknown,   // launched, then the transcript or the process stopped
                _ => WfState.Failed,   // the tool call itself failed or was denied
            },
        };

        List<WfAgent> agents;
        if ((final?.Progress ?? t.Progress) is { ValueKind: JsonValueKind.Array } snap) agents = [.. Agents(snap, now)];
        else if (dir is not null && Journal(dir, state is not (WfState.Running or WfState.Unknown), now) is { } j) agents = j;
        else agents = [];

        // Declared phases first (meta.phases), then any the run named that the script did not declare.
        var declared = final?.Phases ?? MetaPhases(script);
        var titles = declared.Select(p => p.Title)
            .Concat(final?.Progress is { } fp ? PhaseTitles(fp) : t.Progress is { } tp ? PhaseTitles(tp) : [])
            .Concat(agents.Select(a => a.Phase)).Where(x => x.Length > 0).Distinct().ToList();
        var phases = titles.Select(title =>
        {
            var of = agents.Where(a => a.Phase == title).ToList();
            return new WfPhase(title, declared.FirstOrDefault(p => p.Title == title).Detail, of.Count,
                of.Count(a => a.State == WfState.Done), of.Count(a => a.State == WfState.Running), of.Count(a => a.State is WfState.Failed or WfState.Unknown));
        }).ToList();

        var written = state == WfState.Unknown && dir is not null ? LastWrite(dir) : null;
        var live = state == WfState.Running || written is { } w && now - w < TimeSpan.FromMinutes(2);
        // An unfinished run ends, as far as anyone knows, at the last write in its folder.
        var end = live ? now : written ?? t.EndedAt ?? now;
        var elapsed = final?.Duration ?? end - t.StartedAt;
        return new(Events.Str(receipt, "workflowName") ?? Meta(script, "name") ?? Tool.ToLowerInvariant(),
            Events.Str(receipt, "summary") ?? Meta(script, "description"), dir, phases, agents,
            final?.Tokens ?? (t.Tokens > 0 ? t.Tokens : agents.Sum(a => a.Tokens)),
            final?.ToolCalls ?? (t.SubToolUses > 0 ? t.SubToolUses : agents.Sum(a => a.ToolCalls)),
            elapsed < TimeSpan.Zero ? TimeSpan.Zero : elapsed,
            live && state == WfState.Unknown ? WfState.Running : state,   // still being written: it reads as running, not as "no end"
            status is "completed" ? null : status,
            final?.Model, live);
    }

    // The last write in the run folder (journal or an agent transcript). Within 2 minutes, an unfinished run may still be
    // going (another claude holds the session): keep polling.
    // ponytail: a time heuristic, only used to keep polling; replace with the CLI's own liveness signal if one appears.
    static DateTimeOffset? LastWrite(string dir)
    {
        try { return new DirectoryInfo(dir).EnumerateFiles().Select(f => (DateTimeOffset?)new DateTimeOffset(f.LastWriteTimeUtc)).Max(); }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { return null; }
    }

    // ---------- path safety: the run folder comes from CLI output ----------

    static readonly ConcurrentDictionary<string, string?> inside = new();

    // The folder, symlinks resolved, when it lies inside the transcripts root (~/.claude/projects); else null.
    internal static string? Inside(string? path, string? root = null) =>
        string.IsNullOrWhiteSpace(path) ? null : inside.GetOrAdd((root ?? "") + "\n" + path, _ =>
        {
            try
            {
                var full = TranscriptStore.Real(path);
                var r = Path.TrimEndingDirectorySeparator(TranscriptStore.Real(root ?? TranscriptStore.Root)) + Path.DirectorySeparatorChar;
                return full.StartsWith(r, OperatingSystem.IsLinux() ? StringComparison.Ordinal : StringComparison.OrdinalIgnoreCase) ? full : null;
            }
            catch (Exception) { return null; }
        });

    static readonly Regex SafeId = new("^[A-Za-z0-9_-]{1,80}$");

    // agent-<id>.jsonl of the run, when the id is a plain token.
    public static string? AgentFile(string? dir, string? agentId) =>
        dir is not null && agentId is not null && SafeId.IsMatch(agentId) ? Path.Combine(dir, $"agent-{agentId}.jsonl") : null;

    static string? Line(string? text, string prefix) =>
        text?.Split('\n').FirstOrDefault(l => l.StartsWith(prefix, StringComparison.Ordinal)) is { } l ? l[prefix.Length..].Trim() : null;

    // ---------- snapshot (live progress, or workflowProgress of the final .json) ----------

    static IEnumerable<string> PhaseTitles(JsonElement snap) =>
        snap.EnumerateArray().Where(x => Events.Str(x, "type") == "workflow_phase").Select(x => Events.Str(x, "title") ?? "");

    internal static IEnumerable<WfAgent> Agents(JsonElement snap, DateTimeOffset now) =>
        snap.EnumerateArray().Where(x => Events.Str(x, "type") == "workflow_agent").Select(a =>
        {
            var started = Num(a, "startedAt");
            var raw = Events.Str(a, "state");
            // Seen: "start" (queued until it has a startedAt) and "done". Anything else is shown with its own word.
            var state = raw switch { "done" => WfState.Done, "start" => started is null ? WfState.Waiting : WfState.Running, _ => WfState.Failed };
            TimeSpan? d = Num(a, "durationMs") is { } ms ? TimeSpan.FromMilliseconds(ms)
                : state == WfState.Running && started is { } s ? now - DateTimeOffset.FromUnixTimeMilliseconds(s) : null;
            return new WfAgent(Events.Str(a, "label") ?? "", Events.Str(a, "phaseTitle") ?? "", state, Events.Str(a, "model"),
                Num(a, "tokens") ?? 0, false, (int)(Num(a, "toolCalls") ?? 0), d, Events.Str(a, "agentId"), Events.Str(a, "resultPreview"),
                state == WfState.Failed ? raw : null);
        });

    static long? Num(JsonElement e, string name) => Events.Prop(e, name) is { ValueKind: JsonValueKind.Number } n ? (long)n.GetDouble() : null;

    // ---------- script meta (export const meta = { name, description, phases: [{ title, detail }] }) ----------

    const string Q = """(?:'((?:\\.|[^'\\])*)'|"((?:\\.|[^"\\])*)"|`((?:\\.|[^`\\])*)`)""";
    static string Unq(Match m, int from) => Regex.Unescape(m.Groups[from].Success ? m.Groups[from].Value : m.Groups[from + 1].Success ? m.Groups[from + 1].Value : m.Groups[from + 2].Value);

    internal static string? Meta(string? script, string key) =>
        script?.IndexOf("meta", StringComparison.Ordinal) is >= 0 and var i && Regex.Match(script[i..], $@"\b{key}\s*:\s*{Q}") is { Success: true } m ? Unq(m, 1) : null;

    // ponytail: a regex over the JS literal, bounded to the meta block's first 8 KB; only a fallback until the .json exists.
    internal static List<(string Title, string? Detail)> MetaPhases(string? script)
    {
        if (script?.IndexOf("meta", StringComparison.Ordinal) is not (>= 0 and var i) || script.IndexOf("phases", i, StringComparison.Ordinal) is not (>= 0 and var p)) return [];
        var block = script[p..Math.Min(script.Length, p + 8192)];
        return [.. Regex.Matches(block, $@"\{{\s*title\s*:\s*{Q}\s*(?:,\s*detail\s*:\s*{Q}\s*)?,?\s*\}}")
            .Select(m => (Unq(m, 1), m.Groups[4].Success || m.Groups[5].Success || m.Groups[6].Success ? Unq(m, 4) : null))];
    }

    // ---------- the final .json (<session>/workflows/<runId>.json) ----------

    sealed record FinalRun(string? Status, TimeSpan? Duration, long? Tokens, int? ToolCalls, List<(string Title, string? Detail)> Phases, JsonElement? Progress, string? Model);
    static readonly ConcurrentDictionary<string, (long Len, DateTime At, FinalRun? Run)> finals = new();

    static FinalRun? Final(string dir)
    {
        // <session>/subagents/workflows/<runId>: the shape both observed paths have. Anything else has no .json to look for.
        var wf = Path.GetDirectoryName(dir);
        if (Path.GetFileName(wf) != "workflows" || Path.GetDirectoryName(wf) is not { } sub || Path.GetFileName(sub) != "subagents") return null;
        var file = Path.Combine(Path.GetDirectoryName(sub)!, "workflows", Path.GetFileName(dir) + ".json");
        var fi = new FileInfo(file);
        if (!fi.Exists) return null;
        if (finals.TryGetValue(file, out var c) && c.Len == fi.Length && c.At == fi.LastWriteTimeUtc) return c.Run;
        FinalRun? run = null;
        try
        {
            using var d = JsonDocument.Parse(File.ReadAllBytes(file));
            var r = d.RootElement;
            run = new(Events.Str(r, "status"), Num(r, "durationMs") is { } ms ? TimeSpan.FromMilliseconds(ms) : null, Num(r, "totalTokens"),
                Num(r, "totalToolCalls") is { } tc ? (int)tc : null,
                Events.Prop(r, "phases") is { ValueKind: JsonValueKind.Array } ps
                    ? [.. ps.EnumerateArray().Select(x => (Events.Str(x, "title") ?? "", Events.Str(x, "detail")))] : [],
                Events.Prop(r, "workflowProgress") is { ValueKind: JsonValueKind.Array } wp ? wp.Clone() : null, Events.Str(r, "defaultModel"));
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or JsonException) { }   // partial write: retried on the next change
        finals[file] = (fi.Length, fi.LastWriteTimeUtc, run);
        return run;
    }

    // ---------- the journal and agent transcripts, while no .json exists ----------

    // Lines appended since the last read. A trailing line without its newline waits for it; a shorter file starts over.
    internal sealed class Tail(string path)
    {
        long offset;
        public bool Reset { get; private set; }

        public List<string> Next()
        {
            Reset = false;
            try
            {
                using var fs = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete);
                if (fs.Length < offset) { offset = 0; Reset = true; }
                if (fs.Length == offset) return [];
                fs.Seek(offset, SeekOrigin.Begin);
                var buf = new byte[fs.Length - offset];
                var n = fs.ReadAtLeast(buf, buf.Length, false);
                if (n == 0) return [];   // truncated between Length and the read: the next call starts over
                var end = Array.LastIndexOf(buf, (byte)'\n', n - 1);
                if (end < 0) return [];
                offset += end + 1;
                return [.. Encoding.UTF8.GetString(buf, 0, end).Split('\n').Select(l => l.TrimEnd('\r')).Where(l => l.Length > 0)];
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException) { return []; }   // missing yet: partial data
        }
    }

    static JsonElement? Parse(string line)
    {
        try { return JsonDocument.Parse(line).RootElement; }
        catch (JsonException) { return null; }
    }

    sealed class JournalState(string dir)
    {
        public readonly Tail T = new(Path.Combine(dir, "journal.jsonl"));
        public readonly List<(string Id, string Label, string Phase)> Started = [];
        public readonly Dictionary<string, string?> Results = [];
        public readonly Dictionary<string, AgentStats> Stats = [];
    }

    // Per agent-<id>.jsonl, folded line by line: never re-read. Tokens follow the last message's context, which is what
    // the CLI shows to within ~1.5k; the sum over messages would be 15-35 times too high.
    sealed class AgentStats(string path)
    {
        readonly Tail t = new(path);
        public DateTimeOffset? First, Last;
        public string? Model;
        public long Tokens;
        readonly HashSet<string> tools = [];
        public int ToolCalls => tools.Count;

        public void Read()
        {
            var lines = t.Next();
            if (t.Reset) { First = Last = null; Model = null; Tokens = 0; tools.Clear(); }
            foreach (var l in lines)
            {
                if (Parse(l) is not { } e) continue;
                if (DateTimeOffset.TryParse(Events.Str(e, "timestamp"), out var at)) { First ??= at; Last = at; }
                if (Events.Str(e, "type") != "assistant" || Events.Prop(e, "message") is not { } m) continue;
                Model = Events.Str(e, "requestedModel") ?? Events.Str(m, "model") ?? Model;
                if (Events.Prop(m, "usage") is { } u)
                    Tokens = (Num(u, "input_tokens") ?? 0) + (Num(u, "cache_creation_input_tokens") ?? 0) + (Num(u, "cache_read_input_tokens") ?? 0) + (Num(u, "output_tokens") ?? 0);
                if (Events.Prop(m, "content") is { ValueKind: JsonValueKind.Array } c)
                    foreach (var b in c.EnumerateArray())
                        if (Events.Str(b, "type") == "tool_use" && Events.Str(b, "id") is { } id) tools.Add(id);
            }
        }
    }

    static readonly ConcurrentDictionary<string, JournalState> journals = new();

    // ended: the run is over, so an agent started without a result never finished.
    internal static List<WfAgent>? Journal(string dir, bool ended, DateTimeOffset now)
    {
        if (!File.Exists(Path.Combine(dir, "journal.jsonl"))) return null;
        var j = journals.GetOrAdd(dir, d => new JournalState(d));
        lock (j)
        {
            var lines = j.T.Next();
            if (j.T.Reset) { j.Started.Clear(); j.Results.Clear(); }
            foreach (var l in lines)
            {
                if (Parse(l) is not { } e || Events.Str(e, "agentId") is not { } id) continue;   // "launched" and unknown types carry none
                switch (Events.Str(e, "type"))
                {
                    case "started" when j.Started.All(s => s.Id != id):
                        j.Started.Add((id, Events.Str(e, "label") ?? "", Events.Str(e, "phase") ?? ""));
                        break;
                    case "result":
                        j.Results[id] = Events.Prop(e, "result") is { } r ? r.ValueKind == JsonValueKind.String ? r.GetString() : r.GetRawText() : null;
                        break;
                }
            }
            return [.. j.Started.Select(s =>
            {
                var st = AgentFile(dir, s.Id) is { } f ? j.Stats.TryGetValue(s.Id, out var x) ? x : j.Stats[s.Id] = new AgentStats(f) : null;
                st?.Read();
                var done = j.Results.ContainsKey(s.Id);
                var state = done ? WfState.Done : ended ? WfState.Unknown : WfState.Running;   // started, then no result: no end recorded
                TimeSpan? d = st?.First is { } a ? (done || ended ? st.Last ?? a : now) - a : null;
                return new WfAgent(s.Label, s.Phase, state, st?.Model, st?.Tokens ?? 0, true, st?.ToolCalls ?? 0, d, s.Id,
                    j.Results.GetValueOrDefault(s.Id));
            })];
        }
    }

    // ---------- one agent's transcript for the inspector: its last text and its tool calls ----------

    // Replays agent-<id>.jsonl through the session reducer, new lines only. Owned by the panel showing the agent.
    public sealed class AgentLog(string file)
    {
        readonly Tail t = new(file);
        LiveSession s = new("", "", "", "default");

        public (string? Text, IReadOnlyList<ToolItem> Tools) Read()
        {
            var lines = t.Next();
            if (t.Reset) s = new("", "", "", "default");
            foreach (var l in lines)
            {
                if (!l.Contains("\"type\":\"user\"") && !l.Contains("\"type\":\"assistant\"")) continue;
                if (Parse(l.Replace("\"toolUseResult\":", "\"tool_use_result\":")) is not { } e) continue;
                try { TranscriptStore.Apply(s, e, DateTimeOffset.TryParse(Events.Str(e, "timestamp"), out var at) ? at : null); }
                catch (Exception ex) { Console.Error.WriteLine($"workflow agent log: line skipped ({ex.Message})"); }
            }
            return (s.Items.OfType<TextItem>().LastOrDefault()?.Markdown, [.. s.Items.OfType<ToolItem>().Where(x => x.ParentToolUseId is null)]);
        }
    }

    internal static void Check()
    {
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "WorkflowRuns: " + what);
        var now = DateTimeOffset.FromUnixTimeMilliseconds(1791626222733);

        // claude 2.1.296 live snapshot (task_progress.workflow_progress), trimmed: pong:a done, pong:b queued.
        var snap = JsonDocument.Parse("""[{"type":"workflow_phase","index":1,"title":"Ping"},{"type":"workflow_agent","index":1,"label":"pong:a","phaseIndex":1,"phaseTitle":"Ping","agentId":"aea012a4f32b49114","model":"claude-haiku-5-5","state":"done","startedAt":1791626220733,"queuedAt":1791626220731,"attempt":1,"tokens":24538,"toolCalls":0,"durationMs":2045,"resultPreview":"pong"},{"type":"workflow_agent","index":2,"label":"pong:b","phaseIndex":1,"phaseTitle":"Ping","model":"claude-haiku-5-5","state":"start","queuedAt":1791626220731}]""").RootElement.Clone();
        Ok(Agents(snap, now).ToList() is [{ Label: "pong:a", Phase: "Ping", State: WfState.Done, Tokens: 24538, Result: "pong", Model: "claude-haiku-5-5" } a, { State: WfState.Waiting, AgentId: null, Duration: null }]
           && a.Duration == TimeSpan.FromMilliseconds(2045), "snapshot agents");

        const string script = "export const meta = { name: 'pong-probe', description: 'Two agents \\'reply\\' pong', phases: [{ title: 'Ping', detail: 'two pong agents' }, { title: \"Check\" }] }\nphase('Ping')";
        Ok(Meta(script, "name") == "pong-probe" && Meta(script, "description") == "Two agents 'reply' pong" && Meta(null, "name") is null, "script meta");
        Ok(MetaPhases(script) is [("Ping", "two pong agents"), ("Check", null)] && MetaPhases("phase('x')") is [], "script meta phases");

        var input = JsonDocument.Parse(JsonSerializer.Serialize(new { script })).RootElement.Clone();
        var live = Of(new ToolItem("w", Tool, input, null) { State = ToolState.Running, StartedAt = now.AddSeconds(-2), Progress = snap });
        Ok(live is { Name: "pong-probe", State: WfState.Running, Live: true, Done: 1, Agents.Count: 2, Phases: [{ Title: "Ping", Total: 2, Done: 1, State: WfState.Running, Detail: "two pong agents" }, { Title: "Check", Total: 0, State: WfState.Waiting }] },
            "live run from the snapshot, declared phases kept");
        Ok(Of(new ToolItem("w", Tool, input, null) { State = ToolState.Error, StartedAt = now }) is { State: WfState.Unknown, Agents: [], Live: false }
           && Of(new ToolItem("w", Tool, input, null) { State = ToolState.Error, ResultText = "boom", StartedAt = now }).State == WfState.Failed, "unfinished vs failed");

        Ok(Inside(Path.Combine(TranscriptStore.Root, "p", "s", "subagents", "workflows", "wf_1")) is not null
           && Inside(Path.Combine(TranscriptStore.Root, "..", "x")) is null && Inside(Path.GetTempPath()) is null && Inside("") is null, "run dir must be under the projects root");
        Ok(AgentFile("/d", "a1b2") == Path.Combine("/d", "agent-a1b2.jsonl") && AgentFile("/d", "../x") is null && AgentFile(null, "a") is null, "agent file ids");
        Ok(Line("Workflow launched in background. Task ID: w0\nTranscript dir: /r/wf_1\nScript file: /s", "Transcript dir:") == "/r/wf_1", "receipt text line");

        // Journal + agent transcript on disk, as 2.1.295 writes them; a partial last line, an unknown type, a truncation.
        var dir = Path.Combine(Path.GetTempPath(), $"cc-ui-wf-{Guid.NewGuid():N}");
        Directory.CreateDirectory(dir);
        try
        {
            var journal = Path.Combine(dir, "journal.jsonl");
            File.WriteAllText(journal, """
                {"type":"launched"}
                {"type":"started","key":"v2:1","agentId":"a1","label":"probe","phase":"Probe"}
                {"type":"weird","agentId":"a1"}
                {"type":"started","key":"v2:2","agentId":"a2","label":"build","phase":"Build"}
                {"type":"result","key":"v2:1","agentId":"a1","result":{"ok":true}}
                {"type":"result","key":"v2:2","agentId":"a2","res
                """.Replace("\r", ""));
            File.WriteAllText(Path.Combine(dir, "agent-a1.jsonl"), """
                {"type":"user","isSidechain":true,"message":{"role":"user","content":"go"},"timestamp":"2026-10-10T09:00:00.000Z"}
                {"type":"assistant","requestedModel":"claude-opus-5-5[1m]","message":{"id":"m1","model":"claude-opus-5-5","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"ls"}}],"usage":{"input_tokens":2,"cache_creation_input_tokens":10,"cache_read_input_tokens":100,"output_tokens":5}},"timestamp":"2026-10-10T09:00:01.000Z"}
                {"type":"user","message":{"role":"user","content":[{"tool_use_id":"t1","type":"tool_result","content":"a.txt"}]},"timestamp":"2026-10-10T09:00:02.000Z"}
                {"type":"assistant","requestedModel":"claude-opus-5-5[1m]","message":{"id":"m2","model":"claude-opus-5-5","content":[{"type":"text","text":"done"}],"usage":{"input_tokens":2,"cache_creation_input_tokens":20,"cache_read_input_tokens":110,"output_tokens":8}},"timestamp":"2026-10-10T09:00:04.500Z"}

                """.Replace("\r", ""));
            var j = Journal(dir, false, now);
            Ok(j is [{ Label: "probe", Phase: "Probe", State: WfState.Done, Result: """{"ok":true}""", Model: "claude-opus-5-5[1m]", Tokens: 140, ToolCalls: 1, Approx: true } p1,
                     { Label: "build", State: WfState.Running, Model: null, Duration: null }]
               && p1.Duration == TimeSpan.FromSeconds(4.5), "journal: started/result, unknown type ignored, partial line waits, agent stats");
            File.AppendAllText(journal, "ult\":\"pong\"}\n");
            Ok(Journal(dir, false, now) is [_, { State: WfState.Done, Result: "pong" }], "journal: the partial line completes");
            File.WriteAllText(journal, """{"type":"started","agentId":"a3","label":"x","phase":"P"}""" + "\n");
            Ok(Journal(dir, true, now) is [{ AgentId: "a3", State: WfState.Unknown, RawState: null }], "journal: truncated file starts over; an ended run has no end for unfinished agents");
            Ok(Journal(Path.Combine(dir, "none"), false, now) is null, "journal: missing file");

            var log = new AgentLog(Path.Combine(dir, "agent-a1.jsonl"));
            Ok(log.Read() is ("done", [{ Name: "Bash", State: ToolState.Done } bash]) && bash.EndedAt - bash.StartedAt == TimeSpan.FromSeconds(1), "agent log: last text and tools");
            Ok(log.Read() is ("done", [_]), "agent log: nothing new, same state");
            Ok(new AgentLog(Path.Combine(dir, "agent-none.jsonl")).Read() is (null, []), "agent log: missing file");
        }
        finally { Directory.Delete(dir, true); }
    }
}
