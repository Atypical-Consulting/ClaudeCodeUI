using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

public abstract record ClaudeEvent;
public record InitEvt(string SessionId, string Cwd, string Model, string PermissionMode, string[] Tools,
                      McpBrief[] Mcp, string[] SlashCommands, string[] Skills, string[] Agents, string[] Plugins,
                      string FastModeState, string? FastModeReason) : ClaudeEvent;
public record McpBrief(string Name, string Status);                                   // connected|failed|needs-auth|pending
public record StatusEvt(string? Status, string? PermissionMode) : ClaudeEvent;
public record ThinkingEvt(int EstimatedTokens) : ClaudeEvent;
public record TextDeltaEvt(string Text, string? ParentToolUseId) : ClaudeEvent;
public record AssistantTextEvt(string MessageId, string Text, string? ParentToolUseId, bool Synthetic, string? Uuid = null) : ClaudeEvent; // Uuid = the line's (stream and .jsonl alike)
public record ToolUseEvt(string Id, string Name, JsonElement Input, string? ParentToolUseId) : ClaudeEvent;
public record ToolResultEvt(string ToolUseId, string Text, bool IsError, JsonElement? Structured) : ClaudeEvent; // Structured = tool_use_result (objects only)
public record UserTextEvt(string Text, DateTimeOffset? At = null, IReadOnlyList<UserImage>? Images = null) : ClaudeEvent;     // At = transcript timestamp
public record PermissionEvt(string RequestId, string Tool, JsonElement Input, string? Description,
                            string? ToolUseId, JsonElement? Suggestions) : ClaudeEvent;
public record ResultEvt(string Subtype, bool IsError, decimal TotalCostUsd, int DurationMs, int NumTurns,
                        long ContextTokens, long? ContextWindow, string? FastModeState, string? FastModeReason,
                        string? TerminalReason = null) : ClaudeEvent;
public record RateLimitEvt(double FiveHour, DateTimeOffset FiveHourReset, double SevenDay, DateTimeOffset SevenDayReset) : ClaudeEvent; // 0..1
// TaskType: local_agent (Agent), local_bash (a Bash/PowerShell run_in_background or a Monitor), local_workflow...
public record TaskStartedEvt(string TaskId, string ToolUseId, string Description, string SubagentType, string? TaskType = null, bool Backgrounded = false) : ClaudeEvent;
public record TaskUpdatedEvt(string TaskId, string? Status, DateTimeOffset? EndedAt) : ClaudeEvent;   // patch.status: running|completed|failed|killed…
public record TaskProgressEvt(string TaskId, long TotalTokens, int ToolUses, int DurationMs) : ClaudeEvent;
public record TaskDoneEvt(string TaskId, string ToolUseId, string Status, string? Result = null, long Tokens = 0, int ToolUses = 0, int DurationMs = 0) : ClaudeEvent;
public record TitleEvt(string Title) : ClaudeEvent;
public record ResetEvt : ClaudeEvent;                                                // /clear: the CLI starts a new conversation (and session id)
public record QueueEvt(string Uuid, string State) : ClaudeEvent;   // command_lifecycle: queued|started|completed|cancelled|discarded|refused

// The single parsing point for CLI stdout lines and transcript (.jsonl) lines.
public static class Events
{
    public static ClaudeEvent? Parse(JsonElement e) => ParseAll(e).FirstOrDefault();

    // A message may carry several content blocks (e.g. tool_results in one user message): one event per block.
    public static IEnumerable<ClaudeEvent> ParseAll(JsonElement e)
    {
        switch (Str(e, "type"))
        {
            case "system":
                if (ParseSystem(e) is { } s) yield return s;
                break;

            case "stream_event":
                if (e.TryGetProperty("event", out var se) && Str(se, "type") == "content_block_delta"
                    && se.TryGetProperty("delta", out var d) && Str(d, "type") == "text_delta")
                    yield return new TextDeltaEvt(Str(d, "text") ?? "", Str(e, "parent_tool_use_id"));
                break;

            case "assistant":
                {
                    // Malformed lines (missing message, id or input) are skipped, never thrown: a throw aborts the whole message.
                    if (Prop(e, "message") is not { } msg || Prop(msg, "content") is not { ValueKind: JsonValueKind.Array } content) break;
                    var parent = Str(e, "parent_tool_use_id");
                    var synthetic = Str(msg, "model") == "<synthetic>";
                    foreach (var c in content.EnumerateArray())
                        switch (Str(c, "type"))
                        {
                            case "text": yield return new AssistantTextEvt(Str(msg, "id") ?? "", Str(c, "text") ?? "", parent, synthetic, Str(e, "uuid")); break;
                            case "tool_use" when Str(c, "id") is { } id && Prop(c, "input") is { } input:
                                yield return new ToolUseEvt(id, Str(c, "name") ?? "", input, parent); break;
                        }
                    break;
                }

            case "user":
                {
                    var parent = Str(e, "parent_tool_use_id");
                    DateTimeOffset? at = Str(e, "timestamp") is { } ts && DateTimeOffset.TryParse(ts, out var t) ? t : null;
                    if (Prop(e, "message") is not { } msg || Prop(msg, "content") is not { } content) break;
                    if (content.ValueKind == JsonValueKind.String)
                    {
                        if (parent is null) yield return Notification(content.GetString()!) ?? (ClaudeEvent)new UserTextEvt(content.GetString()!, at);
                        break;
                    }
                    if (content.ValueKind != JsonValueKind.Array) break;
                    JsonElement? structured = e.TryGetProperty("tool_use_result", out var tur) && tur.ValueKind == JsonValueKind.Object ? Slim(tur) : null;
                    // Image blocks ride on the message's first text, or stand alone in an image-only message.
                    UserImage[] images = parent is null ? [.. content.EnumerateArray().Where(c => Str(c, "type") == "image").Select(Images.Parse)] : [];
                    var texts = 0;
                    foreach (var c in content.EnumerateArray())
                        switch (Str(c, "type"))
                        {
                            case "tool_result" when Str(c, "tool_use_id") is { } toolUseId:
                                yield return new ToolResultEvt(toolUseId, ResultText(c.TryGetProperty("content", out var rc) ? rc : default),
                                    Bool(c, "is_error"), structured);
                                break;
                            case "text" when parent is null:
                                yield return Notification(Str(c, "text") ?? "") ?? (ClaudeEvent)new UserTextEvt(Str(c, "text") ?? "", at, texts++ == 0 ? images : null);
                                break;
                        }
                    if (texts == 0 && images.Length > 0) yield return new UserTextEvt("", at, images);
                    break;
                }

            case "control_request":
                if (e.TryGetProperty("request", out var r) && Str(r, "subtype") == "can_use_tool"
                    && Str(e, "request_id") is { } requestId && Prop(r, "input") is { } permInput)
                    yield return new PermissionEvt(requestId, Str(r, "tool_name") ?? "", permInput,
                        Str(r, "description"), Str(r, "tool_use_id"), Prop(r, "permission_suggestions"));
                break;

            case "result":
                {
                    // Context = the last API call's prompt size; top-level usage sums every iteration of the turn.
                    var usage = Prop(e, "usage");
                    if (usage is { } u && Prop(u, "iterations") is { ValueKind: JsonValueKind.Array } it && it.GetArrayLength() > 0)
                        usage = it[it.GetArrayLength() - 1];
                    long ctx = usage is { } x ? Long(x, "input_tokens") + Long(x, "cache_read_input_tokens") + Long(x, "cache_creation_input_tokens") : 0;
                    long? window = null;
                    if (Prop(e, "modelUsage") is { ValueKind: JsonValueKind.Object } mu)
                        foreach (var m in mu.EnumerateObject())
                            if (Prop(m.Value, "contextWindow") is { ValueKind: JsonValueKind.Number } w) window = w.GetInt64();
                    yield return new ResultEvt(Str(e, "subtype") ?? "", Bool(e, "is_error"),
                        Prop(e, "total_cost_usd") is { ValueKind: JsonValueKind.Number } cost ? cost.GetDecimal() : 0,
                        (int)Long(e, "duration_ms"), (int)Long(e, "num_turns"), ctx, window,
                        Str(e, "fast_mode_state"), Str(e, "fast_mode_disabled_reason"), Str(e, "terminal_reason"));
                    break;
                }

            case "conversation_reset":
                yield return new ResetEvt();
                break;

            case "command_lifecycle":
                if (Str(e, "command_uuid") is { } cu && Str(e, "state") is { } st) yield return new QueueEvt(cu, st);
                break;

            case "rate_limit_event":
                if (Prop(e, "rate_limit_info") is { } info && Prop(info, "unifiedWindows") is { } uw
                    && Prop(uw, "five_hour") is { } h5 && Prop(uw, "seven_day") is { } d7)
                    yield return new RateLimitEvt(Double(h5, "utilization"), DateTimeOffset.FromUnixTimeSeconds(Long(h5, "resetsAt")),
                        Double(d7, "utilization"), DateTimeOffset.FromUnixTimeSeconds(Long(d7, "resetsAt")));
                break;
        }
    }

    static ClaudeEvent? ParseSystem(JsonElement e) => Str(e, "subtype") switch
    {
        "init" => new InitEvt(Str(e, "session_id") ?? "", Str(e, "cwd") ?? "", Str(e, "model") ?? "", Str(e, "permissionMode") ?? "",
            Names(e, "tools"),
            Prop(e, "mcp_servers") is { ValueKind: JsonValueKind.Array } mcp
                ? mcp.EnumerateArray().Select(m => new McpBrief(Str(m, "name") ?? "", Str(m, "status") ?? "")).ToArray() : [],
            Names(e, "slash_commands"), Names(e, "skills"), Names(e, "agents"), Names(e, "plugins"),
            Str(e, "fast_mode_state") ?? "off", Str(e, "fast_mode_disabled_reason")),
        "status" => new StatusEvt(Str(e, "status"), Str(e, "permissionMode")),
        "thinking_tokens" => new ThinkingEvt((int)Long(e, "estimated_tokens")),
        "task_started" => new TaskStartedEvt(Str(e, "task_id") ?? "", Str(e, "tool_use_id") ?? "", Str(e, "description") ?? "", Str(e, "subagent_type") ?? "",
            Str(e, "task_type"), Bool(e, "is_backgrounded")),
        "task_updated" => Prop(e, "patch") is { } patch
            ? new TaskUpdatedEvt(Str(e, "task_id") ?? "", Str(patch, "status"),
                Long(patch, "end_time") is > 0 and < 253402300800000 and var end ? DateTimeOffset.FromUnixTimeMilliseconds(end) : null) : null,   // out of range = unknown: the reducer uses now
        "task_progress" => Prop(e, "usage") is { } u
            ? new TaskProgressEvt(Str(e, "task_id") ?? "", Long(u, "total_tokens"), (int)Long(u, "tool_uses"), (int)Long(u, "duration_ms")) : null,
        "task_notification" => Prop(e, "usage") is { } nu
            ? new TaskDoneEvt(Str(e, "task_id") ?? "", Str(e, "tool_use_id") ?? "", Str(e, "status") ?? "", null, Long(nu, "total_tokens"), (int)Long(nu, "tool_uses"), (int)Long(nu, "duration_ms"))
            : new TaskDoneEvt(Str(e, "task_id") ?? "", Str(e, "tool_use_id") ?? "", Str(e, "status") ?? ""),
        "session_title_changed" => new TitleEvt(Str(e, "title") ?? ""),
        _ => null,
    };

    // A background agent's outcome arrives as a user message: <task-notification><task-id>…<result>…</result><usage>…</usage>.
    internal static TaskDoneEvt? Notification(string text)
    {
        if (!text.StartsWith("<task-notification>")) return null;
        static string? Tag(string t, string n) =>
            System.Text.RegularExpressions.Regex.Match(t, $"<{n}>(.*?)</{n}>", System.Text.RegularExpressions.RegexOptions.Singleline) is { Success: true } m ? m.Groups[1].Value : null;
        var r = text.IndexOf("<result>", StringComparison.Ordinal);
        var re = text.LastIndexOf("</result>", StringComparison.Ordinal);
        var hasResult = r >= 0 && re > r;
        var result = hasResult ? text[(r + 8)..re] : null;
        var rest = hasResult ? text[..r] + text[(re + 9)..] : text;   // tags below must not match inside the result
        return new TaskDoneEvt(Tag(rest, "task-id") ?? "", Tag(rest, "tool-use-id") ?? "", Tag(rest, "status") ?? "", result,
            long.TryParse(Tag(rest, "subagent_tokens"), out var tok) ? tok : 0,
            int.TryParse(Tag(rest, "tool_uses"), out var tu) ? tu : 0,
            int.TryParse(Tag(rest, "duration_ms"), out var d) ? d : 0);
    }

    // tool_use_result minus the whole files the UI never reads (Edit's originalFile, Read's file.content), copied into its
    // own small document: kept on every ToolItem, the original would pin the full stdout line for the session's lifetime.
    static JsonElement Slim(JsonElement tur)
    {
        var o = JsonObject.Create(tur)!;
        o.Remove("originalFile");
        (o["file"] as JsonObject)?.Remove("content");
        return JsonSerializer.SerializeToElement(o);
    }

    // tool_result content is a string or an array of blocks.
    public static string ResultText(JsonElement content) => content.ValueKind switch
    {
        JsonValueKind.String => content.GetString()!,
        JsonValueKind.Array => string.Join("\n", content.EnumerateArray().Select(b => Str(b, "text") ?? $"[{Str(b, "type")}]")),
        JsonValueKind.Undefined or JsonValueKind.Null => "",
        _ => content.ToString(),
    };

    // String arrays, or arrays of objects with a "name" (plugins).
    static string[] Names(JsonElement e, string name) => Prop(e, name) is { ValueKind: JsonValueKind.Array } a
        ? a.EnumerateArray().Select(x => x.ValueKind == JsonValueKind.String ? x.GetString()! : Str(x, "name") ?? "").ToArray()
        : [];

    public static string? Str(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Object && e.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() : null;

    public static JsonElement? Prop(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Object && e.TryGetProperty(name, out var v) && v.ValueKind != JsonValueKind.Null ? v : null;

    static bool Bool(JsonElement e, string name) => Prop(e, name) is { ValueKind: JsonValueKind.True };
    static long Long(JsonElement e, string name) => Prop(e, name) is { ValueKind: JsonValueKind.Number } n ? (long)n.GetDouble() : 0;
    static double Double(JsonElement e, string name) => Prop(e, name) is { ValueKind: JsonValueKind.Number } n ? n.GetDouble() : 0;

    // Trimmed real lines from the protocol captures (s2/s3.jsonl).
    internal static void Check()
    {
        static ClaudeEvent? P(string json) => Parse(JsonDocument.Parse(json).RootElement.Clone());
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Events: " + what);

        var init = P("""{"type":"system","subtype":"init","cwd":"C:\\w","session_id":"s1","tools":["Task","Edit","PowerShell"],"mcp_servers":[{"name":"plugin:github:github","status":"failed","source":"plugin"},{"name":"x","status":"connected"}],"model":"claude-haiku-5-5","permissionMode":"default","slash_commands":["cost"],"skills":["impeccable"],"agents":["Explore"],"plugins":[{"name":"context7","path":"C:\\p","source":"x"}],"fast_mode_state":"off","fast_mode_disabled_reason":"sdk_opt_in_required"}""") as InitEvt;
        Ok(init is { SessionId: "s1", Cwd: @"C:\w", Model: "claude-haiku-5-5", PermissionMode: "default", FastModeReason: "sdk_opt_in_required" }, "init");
        Ok(init!.Tools.Contains("PowerShell") && init.Mcp is [{ Status: "failed" }, { Name: "x" }] && init.Plugins is ["context7"] && init.Skills is ["impeccable"], "init lists");

        Ok(P("""{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"done"}},"parent_tool_use_id":null}""") is TextDeltaEvt { Text: "done", ParentToolUseId: null }, "text_delta");
        Ok(P("""{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"","estimated_tokens":50}}}""") is null, "thinking_delta ignored");
        Ok(P("""{"type":"system","subtype":"hook_started","hook_id":"h"}""") is null, "hook ignored");
        Ok(P("""{"type":"system","subtype":"thinking_tokens","estimated_tokens":250,"estimated_tokens_delta":200}""") is ThinkingEvt { EstimatedTokens: 250 }, "thinking_tokens");
        Ok(P("""{"type":"system","subtype":"status","status":null,"permissionMode":"acceptEdits"}""") is StatusEvt { Status: null, PermissionMode: "acceptEdits" }, "status");

        var tu = P("""{"type":"assistant","message":{"model":"claude-haiku-5-5","id":"msg_1","content":[{"type":"tool_use","id":"toolu_01Y","name":"Write","input":{"file_path":"C:\\w\\b.txt","content":"x"}}]},"parent_tool_use_id":null}""") as ToolUseEvt;
        Ok(tu is { Id: "toolu_01Y", Name: "Write", ParentToolUseId: null } && Str(tu.Input, "file_path") == @"C:\w\b.txt", "tool_use");
        Ok(P("""{"type":"assistant","message":{"model":"<synthetic>","id":"m","content":[{"type":"text","text":"Total cost"}]},"parent_tool_use_id":null}""") is AssistantTextEvt { Synthetic: true, Text: "Total cost", MessageId: "m", Uuid: null }, "synthetic text");
        Ok(P("""{"type":"assistant","message":{"model":"claude-haiku-5-5","id":"msg_2","role":"assistant","content":[{"type":"text","text":"OK"}]},"parent_tool_use_id":null,"session_id":"s","uuid":"66d742fc-2e1b-4c7e-9a43-0f4e3c0b8f11"}""") is AssistantTextEvt { Text: "OK", Uuid: "66d742fc-2e1b-4c7e-9a43-0f4e3c0b8f11" }, "assistant uuid");

        static List<ClaudeEvent> All(string json) => ParseAll(JsonDocument.Parse(json).RootElement.Clone()).ToList();
        Ok(All("""{"type":"assistant"}""") is [] && All("""{"type":"user","message":"x"}""") is []
           && All("""{"type":"control_request","request_id":"r","request":{"subtype":"can_use_tool","tool_name":"Write"}}""") is [], "malformed lines skipped");
        Ok(All("""{"type":"assistant","message":{"id":"m","content":[{"type":"tool_use","id":"t","name":"Read"},{"type":"text","text":"ok"}]}}""") is [AssistantTextEvt { Text: "ok" }], "block without input skipped, next kept");

        var tr = P("""{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_E","type":"tool_result","content":"The file a.txt has been updated."}]},"parent_tool_use_id":null,"tool_use_result":{"filePath":"C:\\w\\a.txt","oldString":"hello","newString":"bye","originalFile":"hello\n","structuredPatch":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-hello","+bye"]}]}}""") as ToolResultEvt;
        Ok(tr is { ToolUseId: "toolu_E", IsError: false, Structured: { } st } && st.GetProperty("structuredPatch")[0].GetProperty("lines")[1].GetString() == "+bye", "tool_result + tool_use_result");
        Ok(tr?.Structured is { } ts && Prop(ts, "originalFile") is null && Str(ts, "filePath") == @"C:\w\a.txt", "tool_use_result drops originalFile");
        var rd = P("""{"type":"user","message":{"role":"user","content":[{"tool_use_id":"r","type":"tool_result","content":"1→x"}]},"tool_use_result":{"type":"text","file":{"filePath":"a","content":"x","numLines":1}}}""") as ToolResultEvt;
        Ok(rd?.Structured is { } rs && Prop(rs.GetProperty("file"), "content") is null && rs.GetProperty("file").GetProperty("numLines").GetInt32() == 1, "tool_use_result drops file.content");
        Ok(P("""{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":[{"type":"text","text":"Refusé"}],"is_error":true,"tool_use_id":"t"}]},"tool_use_result":"Error: x"}""") is ToolResultEvt { IsError: true, Text: "Refusé", Structured: null }, "tool_result error");
        Ok(P("""{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]},"parent_tool_use_id":null,"timestamp":"2026-10-09T12:41:34.602Z"}""") is UserTextEvt { Text: "[Request interrupted by user]", At: not null }, "user text");

        var perm = P("""{"type":"control_request","request_id":"edd9","request":{"subtype":"can_use_tool","tool_name":"Write","display_name":"Write","input":{"file_path":"C:\\w\\b.txt","content":"x"},"description":"b.txt","permission_suggestions":[{"type":"setMode","mode":"acceptEdits","destination":"session"}],"tool_use_id":"toolu_01Y"}}""") as PermissionEvt;
        Ok(perm is { RequestId: "edd9", Tool: "Write", Description: "b.txt", ToolUseId: "toolu_01Y", Suggestions: { } sg } && Str(sg[0], "type") == "setMode", "can_use_tool");
        var exit = P("""{"type":"control_request","request_id":"1c0e","request":{"subtype":"can_use_tool","tool_name":"ExitPlanMode","display_name":"ExitPlanMode","input":{"plan":"# Plan: create hello.txt\n\n## Steps\n1. Write `hello.txt`.\n","planFilePath":"/Users/u/.claude/plans/p.md"},"tool_use_id":"toolu_01Cc","requires_user_interaction":true}}""") as PermissionEvt;
        Ok(exit is { Tool: "ExitPlanMode", ToolUseId: "toolu_01Cc", Suggestions: null } && Str(exit.Input, "plan")!.StartsWith("# Plan"), "can_use_tool ExitPlanMode (no suggestions)");

        var ok = P("""{"type":"result","subtype":"success","is_error":false,"duration_ms":2548,"num_turns":3,"total_cost_usd":0.0063816,"usage":{"input_tokens":8,"cache_creation_input_tokens":22821,"cache_read_input_tokens":139210,"iterations":[{"input_tokens":2,"output_tokens":3,"cache_read_input_tokens":41768,"cache_creation_input_tokens":108}]},"modelUsage":{"claude-haiku-5-5":{"costUSD":0.0063816,"contextWindow":1000000}},"terminal_reason":"completed","fast_mode_state":"off","fast_mode_disabled_reason":"sdk_opt_in_required"}""") as ResultEvt;
        Ok(ok is { Subtype: "success", IsError: false, TotalCostUsd: 0.0063816m, DurationMs: 2548, NumTurns: 3, ContextTokens: 41878, ContextWindow: 1000000, TerminalReason: "completed", FastModeReason: "sdk_opt_in_required" }, "result success");
        var ab = P("""{"type":"result","subtype":"error_during_execution","is_error":true,"duration_ms":15100,"num_turns":2,"total_cost_usd":0.012436335,"usage":{"input_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"iterations":[]},"terminal_reason":"aborted_streaming"}""") as ResultEvt;
        Ok(ab is { Subtype: "error_during_execution", IsError: true, TerminalReason: "aborted_streaming", ContextTokens: 0, TotalCostUsd: 0.012436335m }, "result interrupted");

        var rl = P("""{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1791552000,"rateLimitType":"five_hour","unifiedWindows":{"five_hour":{"utilization":0.36,"resetsAt":1791552000},"seven_day":{"utilization":0.26,"resetsAt":1791828000}}}}""") as RateLimitEvt;
        Ok(rl is { FiveHour: 0.36, SevenDay: 0.26 } && rl.FiveHourReset.ToUnixTimeSeconds() == 1791552000, "rate_limit_event");

        Ok(P("""{"type":"system","subtype":"task_started","task_id":"ab1","tool_use_id":"toolu_B","description":"Reply pong","subagent_type":"general-purpose","is_backgrounded":false}""") is TaskStartedEvt { TaskId: "ab1", ToolUseId: "toolu_B", Description: "Reply pong", SubagentType: "general-purpose" }, "task_started");
        Ok(P("""{"type":"system","subtype":"task_progress","task_id":"ab1","usage":{"total_tokens":26511,"tool_uses":1,"duration_ms":2092}}""") is TaskProgressEvt { TotalTokens: 26511, ToolUses: 1, DurationMs: 2092 }, "task_progress");
        Ok(P("""{"type":"system","subtype":"task_notification","task_id":"ab1","tool_use_id":"toolu_B","status":"completed","usage":{"total_tokens":26702,"tool_uses":1,"duration_ms":3000}}""") is TaskDoneEvt { TaskId: "ab1", ToolUseId: "toolu_B", Status: "completed", Tokens: 26702, DurationMs: 3000 }, "task_notification");
        var notif = P("""{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>aa5aab0eece92a372</task-id>\n<tool-use-id>toolu_01TEgqvrrwU4RMLtNJYrEavF</tool-use-id>\n<status>completed</status>\n<summary>Agent finished</summary>\n<result>pong</result>\n<usage><subagent_tokens>31599</subagent_tokens><tool_uses>1</tool_uses><duration_ms>5088</duration_ms></usage>\n</task-notification>"},"parent_tool_use_id":null}""");
        Ok(notif is TaskDoneEvt { TaskId: "aa5aab0eece92a372", ToolUseId: "toolu_01TEgqvrrwU4RMLtNJYrEavF", Status: "completed", Result: "pong", Tokens: 31599, ToolUses: 1, DurationMs: 5088 }, "task-notification user message");
        // Background shell (Bash run_in_background) and its stop, from the 2.1.296 capture (--probe-cli background-tasks).
        Ok(P("""{"type":"system","subtype":"task_started","task_id":"b69el8u68","run_id":"0mv1jw258-f850d2f1","tool_use_id":"toolu_01JF","description":"Run 40-tick background loop","is_backgrounded":true,"task_type":"local_bash"}""") is TaskStartedEvt { TaskId: "b69el8u68", ToolUseId: "toolu_01JF", TaskType: "local_bash", Backgrounded: true }, "task_started local_bash");
        var upd = P("""{"type":"system","subtype":"task_updated","task_id":"b69el8u68","run_id":"0mv1jw258-f850d2f1","patch":{"status":"killed","end_time":1791585746931}}""") as TaskUpdatedEvt;
        Ok(upd is { TaskId: "b69el8u68", Status: "killed", EndedAt: { } end } && end.ToUnixTimeMilliseconds() == 1791585746931, "task_updated");
        Ok(P("""{"type":"system","subtype":"task_updated","task_id":"x","patch":{"is_backgrounded":true}}""") is TaskUpdatedEvt { Status: null, EndedAt: null }, "task_updated without status");
        Ok(P("""{"type":"system","subtype":"task_updated","task_id":"x","patch":{"status":"failed","end_time":1e300}}""") is TaskUpdatedEvt { Status: "failed", EndedAt: null }
            && P("""{"type":"system","subtype":"task_updated","task_id":"x","patch":{"status":"failed","end_time":1791585746931.5}}""") is TaskUpdatedEvt { EndedAt: { } fe } && fe.ToUnixTimeMilliseconds() == 1791585746931,
            "task_updated with a fractional or out-of-range end_time");
        Ok(P("""{"type":"system","subtype":"session_title_changed","title":"probe-session"}""") is TitleEvt { Title: "probe-session" }, "title");
        Ok(P("""{"type":"conversation_reset","new_conversation_id":"c7d8","uuid":"c7d8","trigger":"clear","user_message_uuid":"a8d3","timestamp":"2026-10-09T21:32:36.965Z","session_id":"cf56"}""") is ResetEvt, "conversation_reset");
        Ok(P("""{"type":"command_lifecycle","command_uuid":"64bc9ab2-94bb-49c2-8486-8978e4f94126","state":"cancelled","uuid":"e26f0eba-0e60-45ce-87e3-7fb748c98d6c","session_id":"f076ad9a"}""") is QueueEvt { Uuid: "64bc9ab2-94bb-49c2-8486-8978e4f94126", State: "cancelled" }, "command_lifecycle");
    }
}
