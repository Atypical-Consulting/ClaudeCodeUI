using System.Text.Json;
using System.Text.Json.Nodes;

namespace ClaudeCodeUI;

public abstract record ClaudeEvent;
public record InitEvt(string SessionId, string Cwd, string Model, string PermissionMode, string[] Tools,
                      McpBrief[] Mcp, string[] SlashCommands, string[] Skills, string[] Agents, string[] Plugins,
                      string FastModeState, string? FastModeReason, McpSkipped[]? McpErrors = null, PluginError[]? PluginErrors = null,
                      string? OutputStyle = null) : ClaudeEvent;
public record McpBrief(string Name, string Status);                                   // connected|failed|needs-auth|pending
// init.mcp_server_errors: a --mcp-config entry that failed validation and was skipped (absent from mcp_servers).
public record McpSkipped(string Name, string Type, string Message);
// init.plugin_errors: a plugin that did not load, or loaded demoted (type: path-not-found, manifest-validation-error,
// dependency-unsatisfied, hook-load-failed… an open set). Plugin is inline[N] for a --plugin-dir that never got a name.
public record PluginError(string Plugin, string Type, string Message, string? Path);
// system/commands_changed: the slash commands and skills after a reload or a plugin change, as initialize lists them.
public record CommandsEvt(JsonElement Commands) : ClaudeEvent;
// system/ui_status: a plugin's line on the REPL's status row ($.ui.status); Text null clears it.
public record PluginStatusEvt(string Plugin, string? Text) : ClaudeEvent;
// system/elicitation_complete: the MCP server says the URL-mode elicitation it asked for is done.
public record ElicitDoneEvt(string Server, string ElicitationId) : ClaudeEvent;
// system/notification (the REPL's notification bar: fast mode credits, a failed compaction, the stop-hook cap…) and a
// plugin's system/ui_toast. Color: error / warning / … when the CLI gives one.
public record NoticeEvt(string Text, string? Color) : ClaudeEvent;
public record StatusEvt(string? Status, string? PermissionMode) : ClaudeEvent;
public record ThinkingEvt(int EstimatedTokens) : ClaudeEvent;
// An assistant thinking block that has text: the CLI sends it empty unless the session asked for summaries
// (--thinking-display summarized, see AppFlag.Thinking).
public record ThoughtEvt(string Text, string? ParentToolUseId) : ClaudeEvent;
// system/compact_boundary (compact_metadata on the stream, compactMetadata in the transcript): the conversation was compacted.
public record CompactEvt(string Trigger, long PreTokens, long PostTokens, int DurationMs) : ClaudeEvent;
// The summary a compacted conversation goes on from: isCompactSummary in the transcript; on the stream a synthetic user
// message, told from a skill's body by the CLI's fixed first sentence (Events.SummaryLead).
public record SummaryEvt(string Text) : ClaudeEvent;
// system/model_fallback, model_refusal_fallback, model_refusal_no_fallback, model_consent_fallback: the CLI switched model,
// or a refusal got no retry. Retry = the refused user message (to edit and send again), Retracted = the refused partial's
// uuids to drop, Model = the session's model from now on (null when only that turn or a subagent switched).
public record FallbackEvt(string Text, string? Retry, IReadOnlyList<string> Retracted, string? Model) : ClaudeEvent;
// system/session_metadata: the session's published artifacts, the whole list on each change (null once empty). Url is
// kept only when http(s): it ends up in an href.
public record ArtifactsEvt(IReadOnlyList<Artifact> Artifacts) : ClaudeEvent;
public record Artifact(string Url, string Title);
public record TextDeltaEvt(string Text, string? ParentToolUseId) : ClaudeEvent;
public record AssistantTextEvt(string MessageId, string Text, string? ParentToolUseId, bool Synthetic, string? Uuid = null, DateTimeOffset? At = null) : ClaudeEvent; // Uuid = the line's (stream and .jsonl alike), At = transcript timestamp
public record ToolUseEvt(string Id, string Name, JsonElement Input, string? ParentToolUseId) : ClaudeEvent;
public record ToolResultEvt(string ToolUseId, string Text, bool IsError, JsonElement? Structured, bool NotRun = false) : ClaudeEvent; // Structured = tool_use_result (objects only); NotRun: see Events.NotRun
public record UserTextEvt(string Text, DateTimeOffset? At = null, IReadOnlyList<UserImage>? Images = null) : ClaudeEvent;     // At = transcript timestamp
public record PermissionEvt(string RequestId, string Tool, JsonElement Input, string? Description,
                            string? ToolUseId, JsonElement? Suggestions, PermissionInfo? Info = null) : ClaudeEvent;
// can_use_tool's context (claude 2.1.296): agent_id = the asking subagent (its task id), title = an MCP tool's display
// title, decision_reason[_type] / matched_ask_rule = why it asks, suppress_always_allow_rule = offer no "always" rule.
public record PermissionInfo(string? AgentId, string? Title, string? BlockedPath, string? ReasonType, string? Reason,
                             string? AskRule, string? AskRuleSource, bool NoAlwaysAllow);
// control_cancel_request: the CLI withdrew one of its own requests (a can_use_tool that timed out or was aborted).
public record CancelRequestEvt(string RequestId) : ClaudeEvent;
// system/api_retry: an API call failed and is retried after DelayMs (ErrorStatus null = no HTTP response, e.g. a timeout).
public record RetryEvt(int Attempt, int MaxRetries, int DelayMs, int? ErrorStatus, string? Error) : ClaudeEvent;
public record ResultEvt(string Subtype, bool IsError, decimal TotalCostUsd, int DurationMs, int NumTurns,
                        long ContextTokens, long? ContextWindow, string? FastModeState, string? FastModeReason,
                        string? TerminalReason = null, IReadOnlyList<string>? Denied = null,
                        long CacheRead = 0, long CacheWrite = 0) : ClaudeEvent;   // Denied: permission_denials' tool_use_ids; Cache*: the last call's
// Windows 0..1 (FiveHourReset default = the event had no unifiedWindows). Status: allowed|allowed_warning|rejected, for the
// limiting window Type (five_hour, seven_day…). ExtraOn = overageEnabled, ExtraInUse = isUsingOverage, ExtraOff = overageDisabledReason.
public record RateLimitEvt(double FiveHour, DateTimeOffset FiveHourReset, double SevenDay, DateTimeOffset SevenDayReset,
                           string Status = "allowed", string? Type = null, bool ExtraOn = false, bool ExtraInUse = false, string? ExtraOff = null) : ClaudeEvent;
// TaskType: local_agent (Agent), local_bash (a Bash/PowerShell run_in_background or a Monitor), local_workflow...
public record TaskStartedEvt(string TaskId, string ToolUseId, string Description, string SubagentType, string? TaskType = null, bool Backgrounded = false) : ClaudeEvent;
public record TaskUpdatedEvt(string TaskId, string? Status, DateTimeOffset? EndedAt, bool Backgrounded = false) : ClaudeEvent;   // patch.status: running|completed|failed|killed…; patch.is_backgrounded after background_tasks
public record TaskProgressEvt(string TaskId, long TotalTokens, int ToolUses, int DurationMs, JsonElement? Workflow = null) : ClaudeEvent;   // Workflow: workflow_progress, a full snapshot
public record TaskDoneEvt(string TaskId, string ToolUseId, string Status, string? Result = null, long Tokens = 0, int ToolUses = 0, int DurationMs = 0) : ClaudeEvent;
public record TitleEvt(string Title) : ClaudeEvent;
public record ResetEvt : ClaudeEvent;                                                // /clear: the CLI starts a new conversation (and session id)
public record QueueEvt(string Uuid, string State) : ClaudeEvent;                  // command_lifecycle: queued|started|completed|cancelled|discarded|refused
// prompt_suggestion (--prompt-suggestions): the CLI's guess at the next prompt, after a result. It holds it back while
// the rate limit is anything but "allowed" (allowed_warning included), and before the second assistant message.
public record SuggestionEvt(string Text) : ClaudeEvent;
// system/hook_response, streamed with --include-hook-events. Output = stdout + stderr; exit 2 is the CLI's blocking error.
// Id: hook_id, shared with the hook_started that announced it.
public record HookEvt(string Name, string Outcome, int? ExitCode, string Output, string? Id = null) : ClaudeEvent;
public record HookStartedEvt(string Id, string Name) : ClaudeEvent;   // system/hook_started: running until its hook_response
// One configured hook from the get_hooks_listing control response. Disabled is the row's own flag (claude 2.1.296 sets it
// on every row under disableAllHooks), not inferred from the policy. Edit: the entry as stored (editable), only on a row
// of a settings file the session may write: what `claude edit-hook` names as the target of a replace or remove.
public record HookRow(string Event, string Matcher, string Type, string Command, string? Display, string Source, string? SourceLabel, string? Plugin, int? Timeout,
                      bool Disabled = false, HookEdit? Edit = null)
{
    public bool BlocksOnFailure => Edit is { } e && Events.Str(e.Config, "onFailure") == "block";   // the CLI's onFailure: "block"
}
public record HookEdit(string Matcher, JsonElement Config);
public record HookEvent(string Name, string Summary, bool Matcher);   // eventCatalog: every event, for the add form
// Locked: a managed policy keeps every hook edit out (edit_hook refuses them all, fail-closed when it cannot be read).
public record HookListing(List<HookRow> Rows, bool Disabled, bool Locked, IReadOnlyList<HookEvent> Catalog);

// Answer of mcp_authenticate (verified by `--probe-cli mcp-auth`): `authUrl` to open in a browser, or none when the stored
// token was still valid. With callbackExpected the CLI listens on a localhost port for the redirect and reconnects the
// server itself; without it (claude.ai connectors) nothing comes back on its own. Url is kept only when http(s): it ends
// up in an href.
public record McpAuthStart(string? Url, bool CallbackExpected)
{
    public static McpAuthStart Parse(JsonElement r) => new(
        Events.Str(r, "authUrl") is { } u && Uri.TryCreate(u, UriKind.Absolute, out var uri) && uri.Scheme is "http" or "https" ? u : null,
        Events.Prop(r, "callbackExpected") is { ValueKind: JsonValueKind.True });
}

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
                            case "text": yield return new AssistantTextEvt(Str(msg, "id") ?? "", Str(c, "text") ?? "", parent, synthetic, Str(e, "uuid"), Time(e)); break;
                            case "thinking" when Str(c, "thinking") is { } th && th.Trim().Length > 0: yield return new ThoughtEvt(th.Trim(), parent); break;
                            case "tool_use" when Str(c, "id") is { } id && Prop(c, "input") is { } input:
                                yield return new ToolUseEvt(id, Str(c, "name") ?? "", input, parent); break;
                        }
                    break;
                }

            case "user":
                {
                    // Text the CLI injects itself (a skill's body: isSynthetic on the stream, isMeta in the transcript) is not a turn.
                    var parent = Bool(e, "isSynthetic") || Bool(e, "isMeta") ? "injected" : Str(e, "parent_tool_use_id");
                    var at = Time(e);
                    if (Prop(e, "message") is not { } msg || Prop(msg, "content") is not { } content) break;
                    if (Bool(e, "isCompactSummary") || Bool(e, "isSynthetic") && ResultText(content).StartsWith(SummaryLead))
                    {
                        yield return new SummaryEvt(ResultText(content));
                        break;
                    }
                    if (content.ValueKind == JsonValueKind.String)
                    {
                        if (parent is null) yield return Notification(content.GetString()!) ?? (ClaudeEvent)new UserTextEvt(content.GetString()!, at);
                        break;
                    }
                    if (content.ValueKind != JsonValueKind.Array) break;
                    JsonElement? structured = e.TryGetProperty("tool_use_result", out var tur) && tur.ValueKind == JsonValueKind.Object ? Slim(tur) : null;
                    // Image and document blocks ride on the message's first text, or stand alone in an attachment-only message.
                    UserImage[] images = parent is null ? [.. content.EnumerateArray().Where(c => Str(c, "type") is "image" or "document").Select(Images.Parse)] : [];
                    var texts = 0;
                    foreach (var c in content.EnumerateArray())
                        switch (Str(c, "type"))
                        {
                            case "tool_result" when Str(c, "tool_use_id") is { } toolUseId:
                                yield return new ToolResultEvt(toolUseId, ResultText(c.TryGetProperty("content", out var rc) ? rc : default),
                                    Bool(c, "is_error"), structured, Bool(c, "is_error") && NotRun(e, toolUseId));
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
                        Str(r, "description"), Str(r, "tool_use_id"), Prop(r, "permission_suggestions"), Info(r));
                // An MCP elicitation queues like a permission, the whole request as its input (see Elicit).
                else if (Str(r, "subtype") == "elicitation" && Str(e, "request_id") is { } elicitId)
                    yield return new PermissionEvt(elicitId, Elicit.Tool, r, Str(r, "message"), null, null, Info(r));
                break;

            case "control_cancel_request":
                if (Str(e, "request_id") is { } cancelled) yield return new CancelRequestEvt(cancelled);
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
                        Str(e, "fast_mode_state"), Str(e, "fast_mode_disabled_reason"), Str(e, "terminal_reason"),
                        Prop(e, "permission_denials") is { ValueKind: JsonValueKind.Array } pd
                            ? [.. pd.EnumerateArray().Select(x => Str(x, "tool_use_id")).OfType<string>()] : null,
                        usage is { } c ? Long(c, "cache_read_input_tokens") : 0, usage is { } w2 ? Long(w2, "cache_creation_input_tokens") : 0);
                    break;
                }

            case "conversation_reset":
                yield return new ResetEvt();
                break;

            case "command_lifecycle":
                if (Str(e, "command_uuid") is { } cu && Str(e, "state") is { } st) yield return new QueueEvt(cu, st);
                break;

            case "prompt_suggestion":
                if (Str(e, "suggestion") is { Length: > 0 } sug) yield return new SuggestionEvt(sug);
                break;

            case "rate_limit_event":
                if (Prop(e, "rate_limit_info") is { } info)
                {
                    var uw = Prop(info, "unifiedWindows");
                    JsonElement? h5 = uw is { } u5 ? Prop(u5, "five_hour") : null, d7 = uw is { } u7 ? Prop(u7, "seven_day") : null;
                    yield return new RateLimitEvt(h5 is { } w1 ? Double(w1, "utilization") : 0, h5 is { } w2 ? DateTimeOffset.FromUnixTimeSeconds(Long(w2, "resetsAt")) : default,
                        d7 is { } w3 ? Double(w3, "utilization") : 0, d7 is { } w4 ? DateTimeOffset.FromUnixTimeSeconds(Long(w4, "resetsAt")) : default,
                        Str(info, "status") ?? "allowed", Str(info, "rateLimitType"), Bool(info, "overageEnabled"), Bool(info, "isUsingOverage"), Str(info, "overageDisabledReason"));
                }
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
            Str(e, "fast_mode_state") ?? "off", Str(e, "fast_mode_disabled_reason"),
            Prop(e, "mcp_server_errors") is { ValueKind: JsonValueKind.Array } me
                ? [.. me.EnumerateArray().Select(x => new McpSkipped(Str(x, "name") ?? "", Str(x, "type") ?? "", Str(x, "message") ?? ""))] : null,
            Prop(e, "plugin_errors") is { ValueKind: JsonValueKind.Array } pe
                ? [.. pe.EnumerateArray().Select(x => new PluginError(Str(x, "plugin") ?? "", Str(x, "type") ?? "", Str(x, "message") ?? "", Str(x, "path")))] : null,
            Str(e, "output_style")),
        "status" => new StatusEvt(Str(e, "status"), Str(e, "permissionMode")),
        "thinking_tokens" => new ThinkingEvt((int)Long(e, "estimated_tokens")),
        "task_started" => new TaskStartedEvt(Str(e, "task_id") ?? "", Str(e, "tool_use_id") ?? "", Str(e, "description") ?? "", Str(e, "subagent_type") ?? "",
            Str(e, "task_type"), Bool(e, "is_backgrounded")),
        "task_updated" => Prop(e, "patch") is { } patch
            ? new TaskUpdatedEvt(Str(e, "task_id") ?? "", Str(patch, "status"),
                Long(patch, "end_time") is > 0 and < 253402300800000 and var end ? DateTimeOffset.FromUnixTimeMilliseconds(end) : null, Bool(patch, "is_backgrounded")) : null,   // out of range = unknown: the reducer uses now
        "task_progress" => Prop(e, "usage") is { } u
            ? new TaskProgressEvt(Str(e, "task_id") ?? "", Long(u, "total_tokens"), (int)Long(u, "tool_uses"), (int)Long(u, "duration_ms"),
                Prop(e, "workflow_progress") is { ValueKind: JsonValueKind.Array } wp ? wp.Clone() : null) : null,
        "task_notification" => Prop(e, "usage") is { } nu
            ? new TaskDoneEvt(Str(e, "task_id") ?? "", Str(e, "tool_use_id") ?? "", Str(e, "status") ?? "", null, Long(nu, "total_tokens"), (int)Long(nu, "tool_uses"), (int)Long(nu, "duration_ms"))
            : new TaskDoneEvt(Str(e, "task_id") ?? "", Str(e, "tool_use_id") ?? "", Str(e, "status") ?? ""),
        "api_retry" => new RetryEvt((int)Long(e, "attempt"), (int)Long(e, "max_retries"), (int)Long(e, "retry_delay_ms"),
            Prop(e, "error_status") is { ValueKind: JsonValueKind.Number } es ? (int)es.GetDouble() : null, Str(e, "error")),
        "session_title_changed" => new TitleEvt(Str(e, "title") ?? ""),
        "hook_response" => new HookEvt(Str(e, "hook_name") ?? Str(e, "hook_event") ?? "", Str(e, "outcome") ?? "",
            Prop(e, "exit_code") is { ValueKind: JsonValueKind.Number } x ? (int)x.GetDouble() : null, (Str(e, "output") ?? "").TrimEnd(), Str(e, "hook_id")),
        "hook_started" when Str(e, "hook_id") is { } hid => new HookStartedEvt(hid, Str(e, "hook_name") ?? Str(e, "hook_event") ?? ""),
        "elicitation_complete" => new ElicitDoneEvt(Str(e, "mcp_server_name") ?? "", Str(e, "elicitation_id") ?? ""),
        "notification" when Str(e, "text") is { Length: > 0 } nt => new NoticeEvt(nt, Str(e, "color")),
        // ui_log is a plugin's $.ui.log line: the REPL appends it to the transcript as a notice under the plugin's name.
        "ui_toast" or "ui_log" when Str(e, "text") is { Length: > 0 } tt => new NoticeEvt(Str(e, "plugin") is { } pl ? $"{pl}: {tt}" : tt, null),
        "ui_status" when Str(e, "plugin") is { Length: > 0 } sp => new PluginStatusEvt(sp, Str(e, "text") is { Length: > 0 } st ? st : null),
        "commands_changed" when Prop(e, "commands") is { ValueKind: JsonValueKind.Array } cc => new CommandsEvt(cc.Clone()),
        "compact_boundary" when (Prop(e, "compact_metadata") ?? Prop(e, "compactMetadata")) is { } cm =>
            new CompactEvt(Str(cm, "trigger") ?? "", Long(cm, "pre_tokens") + Long(cm, "preTokens"), Long(cm, "post_tokens") + Long(cm, "postTokens"),
                (int)(Long(cm, "duration_ms") + Long(cm, "durationMs"))),
        // Transcript lines name these in camelCase. Only a refusal says which message to retry; a session-scoped swap
        // (refusal fallback not "local", consent fallback) changes the model of the turns that follow.
        "model_fallback" or "model_refusal_fallback" or "model_refusal_no_fallback" or "model_consent_fallback" when Str(e, "content") is { Length: > 0 } fc =>
            new FallbackEvt(fc, Str(e, "refused_user_message_uuid") ?? Str(e, "refusedUserMessageUuid"),
                [.. Names(e, "retracted_message_uuids"), .. Names(e, "retractedMessageUuids")],
                Str(e, "subtype") is "model_consent_fallback" || Str(e, "subtype") == "model_refusal_fallback" && Str(e, "scope") != "local"
                    ? Str(e, "fallback_model") ?? Str(e, "fallbackModel") : null),
        "session_metadata" when Prop(e, "metadata") is { } md && md.TryGetProperty("artifacts", out var arts) =>
            new ArtifactsEvt(arts.ValueKind == JsonValueKind.Array
                ? [.. arts.EnumerateArray().Where(a => Uri.TryCreate(Str(a, "url"), UriKind.Absolute, out var au) && au.Scheme is "http" or "https").Select(a => new Artifact(Str(a, "url")!, Str(a, "title") is { Length: > 0 } t ? t : Str(a, "url")!))]
                : []),
        _ => null,
    };

    // get_hooks_listing → its `hooks` rows (grouped by event by the CLI), whether a policy turns them all off or locks edits.
    public static HookListing Hooks(JsonElement listing)
    {
        var rows = Prop(listing, "hooks") is { ValueKind: JsonValueKind.Array } a
            ? a.EnumerateArray().Select(h =>
            {
                var cmd = Str(h, "commandText") ?? Str(h, "displayText") ?? "";
                var display = Str(h, "displayText");
                return new HookRow(Str(h, "event") ?? "", Str(h, "matcher") ?? "", Str(h, "type") ?? "", cmd, display == cmd ? null : display,
                    Str(h, "source") ?? "", Str(h, "sourceLabel"), Str(h, "pluginName"),
                    Prop(h, "timeout") is { ValueKind: JsonValueKind.Number } t ? (int)t.GetDouble() : null, Bool(h, "disabled"),
                    Prop(h, "editable") is { } ed && Prop(ed, "config") is { ValueKind: JsonValueKind.Object } cfg ? new HookEdit(Str(ed, "matcher") ?? "", cfg.Clone()) : null);
            }).ToList()
            : [];
        var p = Prop(listing, "policy") ?? default;
        return new(rows, Bool(p, "allDisabled") || Bool(p, "disabledByPolicy"),
            Bool(p, "disabledByPolicy") || Bool(p, "managedOnly") || Bool(p, "pluginOnly") || Bool(p, "policyUnreadable"),
            Prop(listing, "eventCatalog") is { ValueKind: JsonValueKind.Array } ec
                ? [.. ec.EnumerateArray().Select(x => new HookEvent(Str(x, "name") ?? "", Str(x, "summary") ?? "", Bool(x, "supportsMatcher"))).Where(x => x.Name.Length > 0)] : []);
    }

    // A hook output that is only {"hookSpecificOutput":{"hookEventName":…,"additionalContext":…}}: context the CLI injects
    // into the prompt (SubagentStart, SessionStart and UserPromptSubmit plugins), for the model rather than the reader.
    // Anything else in it (a decision, a reason, a permission verdict) is worth showing.
    internal static bool ContextOnly(string output)
    {
        if (!output.TrimStart().StartsWith('{')) return false;
        try
        {
            using var d = JsonDocument.Parse(output);
            var r = d.RootElement;
            return r.ValueKind == JsonValueKind.Object
                   && r.EnumerateObject().All(p => p.Name is "hookSpecificOutput" or "suppressOutput")
                   && Prop(r, "hookSpecificOutput") is { ValueKind: JsonValueKind.Object } o
                   && o.EnumerateObject().All(p => p.Name is "hookEventName" or "additionalContext");
        }
        catch (JsonException) { return false; }
    }

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

    // The first sentence of the summary a compaction leaves (CLI 2.1.296 constant).
    internal const string SummaryLead = "This session is being continued from a previous conversation that ran out of context.";

    static DateTimeOffset? Time(JsonElement e) => Str(e, "timestamp") is { } ts && DateTimeOffset.TryParse(ts, out var t) ? t : null;

    // Why an is_error result did not run: tool_result_meta[].non_execution_kind on the stream, the line's toolDenialKind in
    // the transcript (claude 2.1.296). automode-unavailable / automode-parsing-error: auto mode's check had no usable
    // answer and the call was held back, the CLI's "Not run" (not a decision: retrying is fine). Others are denials or aborts.
    static bool NotRun(JsonElement e, string toolUseId) =>
        (Str(e, "toolDenialKind") ?? (Prop(e, "tool_result_meta") is { ValueKind: JsonValueKind.Array } meta
            ? meta.EnumerateArray().Where(m => Str(m, "id") == toolUseId).Select(m => Str(m, "non_execution_kind")).FirstOrDefault() : null))
        is "automode-unavailable" or "automode-parsing-error";

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

    static PermissionInfo? Info(JsonElement r)
    {
        var ask = Prop(r, "matched_ask_rule");
        var info = new PermissionInfo(Str(r, "agent_id"), Str(r, "title"), Str(r, "blocked_path"), Str(r, "decision_reason_type"),
            Str(r, "decision_reason"), ask is { } a ? Str(a, "tool_name") + (Str(a, "rule_content") is { } c ? $"({c})" : "") : null,
            ask is { } b ? Str(b, "source") : null, Bool(r, "suppress_always_allow_rule"));
        return info == new PermissionInfo(null, null, null, null, null, null, null, false) ? null : info;
    }

    // String arrays, or arrays of objects with a "name" (plugins).
    internal static string[] Names(JsonElement e, string name) => Prop(e, name) is { ValueKind: JsonValueKind.Array } a
        ? a.EnumerateArray().Select(x => x.ValueKind == JsonValueKind.String ? x.GetString()! : Str(x, "name") ?? "").ToArray()
        : [];

    public static string? Str(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Object && e.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() : null;

    public static JsonElement? Prop(JsonElement e, string name) =>
        e.ValueKind == JsonValueKind.Object && e.TryGetProperty(name, out var v) && v.ValueKind != JsonValueKind.Null ? v : null;

    internal static bool Bool(JsonElement e, string name) => Prop(e, name) is { ValueKind: JsonValueKind.True };
    internal static long Long(JsonElement e, string name) => Prop(e, name) is { ValueKind: JsonValueKind.Number } n ? (long)n.GetDouble() : 0;
    static double Double(JsonElement e, string name) => Prop(e, name) is { ValueKind: JsonValueKind.Number } n ? n.GetDouble() : 0;

    // Trimmed real lines from the protocol captures (s2/s3.jsonl).
    internal static void Check()
    {
        static ClaudeEvent? P(string json) => Parse(JsonDocument.Parse(json).RootElement.Clone());
        static void Ok(bool c, string what) => SelfCheck.Assert(c, "Events: " + what);

        var init = P("""{"type":"system","subtype":"init","cwd":"C:\\w","session_id":"s1","tools":["Task","Edit","PowerShell"],"mcp_servers":[{"name":"plugin:github:github","status":"failed","source":"plugin"},{"name":"x","status":"connected"}],"model":"claude-haiku-5-5","permissionMode":"default","slash_commands":["cost"],"skills":["impeccable"],"agents":["Explore"],"plugins":[{"name":"context7","path":"C:\\p","source":"x"}],"fast_mode_state":"off","fast_mode_disabled_reason":"sdk_opt_in_required"}""") as InitEvt;
        Ok(init is { SessionId: "s1", Cwd: @"C:\w", Model: "claude-haiku-5-5", PermissionMode: "default", FastModeReason: "sdk_opt_in_required" }, "init");
        Ok(init!.Tools.Contains("PowerShell") && init.Mcp is [{ Status: "failed" }, { Name: "x" }] && init.Plugins is ["context7"] && init.Skills is ["impeccable"] && init.McpErrors is null && init.OutputStyle is null, "init lists");
        Ok(P("""{"type":"system","subtype":"init","session_id":"s","output_style":"Concise","memory_paths":{"auto":"/u/.claude/projects/-w/memory/"}}""") is InitEvt { OutputStyle: "Concise" }, "init output_style");
        // claude 2.1.296 with --mcp-config holding {"bad":{"url":…}} (no type).
        Ok(P("""{"type":"system","subtype":"init","session_id":"s","mcp_servers":[{"name":"probe","status":"connected","source":"dynamic"}],"mcp_server_errors":[{"name":"bad","type":"url_missing_type","message":"Skipped — MCP server \"bad\" has a \"url\" but no \"type\""}]}""")
           is InitEvt { McpErrors: [{ Name: "bad", Type: "url_missing_type", Message: var skipped }] } && skipped.StartsWith("Skipped"), "init mcp_server_errors");
        // MCP elicitation, form then URL mode, and its completion (claude 2.1.296, the probe's fake stdio server).
        Ok(P("""{"type":"control_request","request_id":"9627","request":{"subtype":"elicitation","mcp_server_name":"probe","message":"What is your name?","mode":"form","requested_schema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]},"title":"Sign up"}}""")
           is PermissionEvt { RequestId: "9627", Tool: Elicit.Tool, Description: "What is your name?", ToolUseId: null, Info.Title: "Sign up" } ef && Events.Str(ef.Input, "mcp_server_name") == "probe", "elicitation form");
        Ok(P("""{"type":"control_request","request_id":"4717","request":{"subtype":"elicitation","mcp_server_name":"probe","message":"Open the page","mode":"url","url":"https://example.com/link?x=1","elicitation_id":"el-42"}}""")
           is PermissionEvt { Tool: Elicit.Tool, Info: null } ue && Elicit.IsUrl(ue.Input), "elicitation url");
        Ok(P("""{"type":"system","subtype":"elicitation_complete","mcp_server_name":"probe","elicitation_id":"el-42","uuid":"6be3","session_id":"b0f2"}""") is ElicitDoneEvt { Server: "probe", ElicitationId: "el-42" }, "elicitation_complete");
        // The SDK schema of system/notification (the compaction failure the CLI emits) and of a plugin's ui_toast.
        Ok(P("""{"type":"system","subtype":"notification","key":"error-compacting-conversation","text":"Error compacting conversation","priority":"immediate","color":"error","uuid":"u","session_id":"s"}""") is NoticeEvt { Text: "Error compacting conversation", Color: "error" }
           && P("""{"type":"system","subtype":"ui_toast","plugin":"impeccable","text":"Saved","timeout_ms":4000,"uuid":"u","session_id":"s"}""") is NoticeEvt { Text: "impeccable: Saved", Color: null }
           && P("""{"type":"system","subtype":"notification","key":"k","text":"","priority":"low"}""") is null, "notification and ui_toast");
        // The SDK schemas of a plugin's ui_log and ui_status pushes (2.1.296), and commands_changed after reload_plugins.
        Ok(P("""{"type":"system","subtype":"ui_log","plugin":"demo","text":"indexed 12 files","uuid":"u","session_id":"s"}""") is NoticeEvt { Text: "demo: indexed 12 files" }
           && P("""{"type":"system","subtype":"ui_status","plugin":"demo","text":"syncing","uuid":"u","session_id":"s"}""") is PluginStatusEvt { Plugin: "demo", Text: "syncing" }
           && P("""{"type":"system","subtype":"ui_status","plugin":"demo","text":null,"uuid":"u","session_id":"s"}""") is PluginStatusEvt { Text: null }, "ui_log and ui_status");
        Ok(P("""{"type":"system","subtype":"commands_changed","commands":[{"name":"cua-driver-rs","description":"Drive a GUI app (user)","argumentHint":"","aliases":["cua-driver"]}],"uuid":"u","session_id":"s"}""")
           is CommandsEvt { Commands: var cmds } && cmds.GetArrayLength() == 1, "commands_changed");
        // claude 2.1.296 with --plugin-dir ./badplug (absent): the entry never loaded, so it has the positional name and a path.
        Ok(P("""{"type":"system","subtype":"init","session_id":"s","plugins":[],"plugin_errors":[{"plugin":"inline[0]","type":"path-not-found","message":"Path not found: /w/badplug (commands)","path":"/w/badplug"}]}""")
           is InitEvt { PluginErrors: [{ Plugin: "inline[0]", Type: "path-not-found", Path: "/w/badplug" }] } && init.PluginErrors is null, "init plugin_errors");

        Ok(P("""{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"done"}},"parent_tool_use_id":null}""") is TextDeltaEvt { Text: "done", ParentToolUseId: null }, "text_delta");
        Ok(P("""{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"","estimated_tokens":50}}}""") is null, "thinking_delta ignored");
        // claude 2.1.296 with --include-hook-events, a SessionStart plugin hook (trimmed): started, then its response by hook_id.
        Ok(P("""{"type":"system","subtype":"hook_started","hook_id":"d11d","hook_name":"SessionStart:startup","hook_event":"SessionStart","uuid":"6e4d","session_id":"7f85"}""") is HookStartedEvt { Id: "d11d", Name: "SessionStart:startup" }
           && P("""{"type":"system","subtype":"hook_started"}""") is null, "hook_started");
        // claude 2.1.296 shapes: the SDK schema of api_retry, the cancel its control client writes, a probed background_tasks patch.
        Ok(P("""{"type":"system","subtype":"api_retry","attempt":2,"max_retries":10,"retry_delay_ms":4000,"error_status":529,"error":"overloaded"}""")
           is RetryEvt { Attempt: 2, MaxRetries: 10, DelayMs: 4000, ErrorStatus: 529, Error: "overloaded" }, "api_retry");
        Ok(P("""{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":500,"error_status":null,"error":"unknown"}""") is RetryEvt { ErrorStatus: null }, "api_retry without HTTP status");
        Ok(P("""{"type":"control_cancel_request","request_id":"r9"}""") is CancelRequestEvt { RequestId: "r9" }, "control_cancel_request");
        Ok(P("""{"type":"system","subtype":"task_updated","task_id":"b6","patch":{"is_backgrounded":true}}""") is TaskUpdatedEvt { TaskId: "b6", Status: null, Backgrounded: true }, "task moved to the background");
        // claude 2.1.296 with --include-hook-events: a PreToolUse hook that exits 2.
        Ok(P("""{"type":"system","subtype":"hook_response","hook_id":"98f5","hook_name":"PreToolUse:Write","hook_event":"PreToolUse","output":"nope: blocked\n","stdout":"","stderr":"nope: blocked\n","exit_code":2,"outcome":"error","uuid":"7e61","session_id":"d427"}""")
           is HookEvt { Name: "PreToolUse:Write", Outcome: "error", ExitCode: 2, Output: "nope: blocked", Id: "98f5" }, "hook_response");
        var (hooks, off, locked, catalog) = Hooks(JsonDocument.Parse("""{"events":[],"hooks":[{"event":"PreToolUse","matcher":"Bash|PowerShell","source":"projectSettings","sourceLabel":"Project settings (.claude/settings.json)","type":"command","displayText":"echo ok","commandText":"echo ok","contentLabel":"Command","timeout":5,"editable":{}},{"event":"Stop","matcher":"","source":"pluginHook","sourceLabel":"Plugin hooks (~/.claude/plugins/*/hooks/hooks.json)","pluginName":"impeccable@impeccable","type":"command","displayText":"Design deep pass","commandText":"impeccable hook","contentLabel":"Command","statusMessage":"Design deep pass"}],"policy":{"disabledByPolicy":false,"managedOnly":false,"pluginOnly":false,"allDisabled":false,"policyHookCount":0}}""").RootElement);
        Ok(!off && !locked && catalog.Count == 0 && hooks is [{ Event: "PreToolUse", Matcher: "Bash|PowerShell", Type: "command", Command: "echo ok", Display: null, Source: "projectSettings", Plugin: null, Timeout: 5, Edit: null },
                             { Event: "Stop", Matcher: "", Command: "impeccable hook", Display: "Design deep pass", Source: "pluginHook", Plugin: "impeccable@impeccable", Timeout: null, Edit: null }], "get_hooks_listing");
        // claude 2.1.296, a hook written by `claude edit-hook` with onFailure, and the head of the event catalog.
        var ed = Hooks(JsonDocument.Parse("""{"hooks":[{"event":"PostToolUse","matcher":"","source":"projectSettings","sourceLabel":"Project settings (.claude/settings.json)","type":"command","displayText":"echo bye","commandText":"echo bye","contentLabel":"Command","editable":{"matcher":"","config":{"type":"command","command":"echo bye","onFailure":"block"}}}],"eventCatalog":[{"name":"PreToolUse","summary":"Before tool execution","supportsMatcher":true},{"name":"Stop","summary":"Right before Claude concludes its response","supportsMatcher":false}],"policy":{"disabledByPolicy":false,"managedOnly":false,"pluginOnly":false,"allDisabled":false,"policyHookCount":0,"policyUnreadable":true}}""").RootElement);
        Ok(ed is { Locked: true, Disabled: false, Rows: [{ BlocksOnFailure: true, Edit: { Matcher: "" } e1 }], Catalog: [{ Name: "PreToolUse", Matcher: true }, { Name: "Stop", Matcher: false }] }
           && Str(e1.Config, "command") == "echo bye" && !hooks[0].BlocksOnFailure, "editable rows, onFailure, catalog, policy lock");
        Ok(Hooks(JsonDocument.Parse("""{"hooks":[],"policy":{"allDisabled":true}}""").RootElement).Disabled, "hooks disabled by policy");
        // claude 2.1.296, project settings with "disableAllHooks": true: every row carries "disabled": true.
        Ok(Hooks(JsonDocument.Parse("""{"hooks":[{"event":"Stop","matcher":"","source":"projectSettings","type":"command","displayText":"echo a","commandText":"echo a","disabled":true}],"policy":{"allDisabled":true}}""").RootElement).Rows is [{ Disabled: true }]
           && hooks.All(h => !h.Disabled), "hook row disabled flag");
        // claude 2.1.296: a SubagentStart hook's output (compact), a SessionStart plugin's (pretty-printed).
        Ok(ContextOnly("""{"hookSpecificOutput": {"hookEventName": "SubagentStart", "additionalContext": "ccui-probe-subagent-ctx"}}"""), "context-only hook output");
        Ok(ContextOnly("{\n  \"hookSpecificOutput\": {\n    \"hookEventName\": \"SessionStart\",\n    \"additionalContext\": \"x\"\n  }\n}"), "context-only, pretty-printed");
        Ok(!ContextOnly("""{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"no"}}"""), "a decision is not context");
        Ok(!ContextOnly("""{"decision":"block","reason":"tests fail"}""") && !ContextOnly("ccui-probe-pre") && !ContextOnly("{oops"), "plain or other output is not context");
        Ok(P("""{"type":"system","subtype":"thinking_tokens","estimated_tokens":250,"estimated_tokens_delta":200}""") is ThinkingEvt { EstimatedTokens: 250 }, "thinking_tokens");
        Ok(P("""{"type":"system","subtype":"status","status":null,"permissionMode":"acceptEdits"}""") is StatusEvt { Status: null, PermissionMode: "acceptEdits" }, "status");
        // claude 2.1.296, /compact (probed): status compacting, then the boundary and the summary as a synthetic user message;
        // in the transcript the same boundary in camelCase and the summary flagged isCompactSummary (and isVisibleInTranscriptOnly).
        Ok(P("""{"type":"system","subtype":"status","status":"compacting","session_id":"f9ce","uuid":"c24f"}""") is StatusEvt { Status: "compacting", PermissionMode: null }, "status compacting");
        Ok(P("""{"type":"system","subtype":"compact_boundary","session_id":"f9ce","uuid":"9b6e","compact_metadata":{"trigger":"manual","pre_tokens":37317,"post_tokens":8995,"cumulative_dropped_tokens":28322,"duration_ms":3207},"logical_parent_uuid":"e464"}""")
           is CompactEvt { Trigger: "manual", PreTokens: 37317, PostTokens: 8995, DurationMs: 3207 }, "compact_boundary");
        Ok(P("""{"parentUuid":null,"logicalParentUuid":"e464","isSidechain":false,"type":"system","subtype":"compact_boundary","content":"Conversation compacted","isMeta":false,"timestamp":"2026-10-11T00:26:19.756Z","uuid":"9b6e","level":"info","compactMetadata":{"trigger":"auto","preTokens":37317,"durationMs":3207,"postTokens":8995,"cumulativeDroppedTokens":28322}}""")
           is CompactEvt { Trigger: "auto", PreTokens: 37317, PostTokens: 8995, DurationMs: 3207 }, "compact_boundary in the transcript");
        const string summary = SummaryLead + " The summary below covers the earlier portion of the conversation.\\n\\nSummary:\\n1. Primary Request";   // JSON-escaped
        Ok(P($$"""{"type":"user","message":{"role":"user","content":"{{summary}}"},"session_id":"s","parent_tool_use_id":null,"uuid":"30d0","timestamp":"2026-10-11T00:45:05.600Z","isReplay":false,"isSynthetic":true}""") is SummaryEvt { Text: var st1 } && st1.EndsWith("Primary Request"), "compact summary on the stream");
        Ok(P($$"""{"parentUuid":"9b6e","isSidechain":false,"type":"user","message":{"role":"user","content":"{{summary}}"},"isVisibleInTranscriptOnly":true,"isCompactSummary":true,"uuid":"ec30","timestamp":"2026-10-11T00:26:19.754Z"}""") is SummaryEvt, "compact summary in the transcript");
        // claude 2.1.296 with --thinking-display summarized (probed): the thinking block carries the summary; by default it is "".
        Ok(All("""{"type":"assistant","message":{"model":"claude-haiku-5-5","id":"msg_1","content":[{"type":"thinking","thinking":"Checking 391: 17 × 23 = 391, so it's not prime.\n\n","signature":"CAQS"}]},"parent_tool_use_id":null}""") is [ThoughtEvt { Text: "Checking 391: 17 × 23 = 391, so it's not prime.", ParentToolUseId: null }]
           && All("""{"type":"assistant","message":{"id":"m","content":[{"type":"thinking","thinking":"","signature":"CAQS"},{"type":"text","text":"hi"}]}}""") is [AssistantTextEvt], "thinking block, empty one skipped");
        // The SDK schemas (2.1.296) of the fallback notices; a local refusal fallback leaves the session's model alone.
        Ok(P("""{"type":"system","subtype":"model_refusal_fallback","trigger":"refusal","direction":"retry","scope":"session","original_model":"claude-fable-5","fallback_model":"claude-opus-5-5","request_id":null,"api_refusal_category":"cyber","retracted_message_uuids":["a1","a2"],"refused_user_message_uuid":"u7","content":"Fable 5 declined; retried with Opus 5.5","uuid":"f1","session_id":"s"}""")
           is FallbackEvt { Text: "Fable 5 declined; retried with Opus 5.5", Retry: "u7", Retracted: ["a1", "a2"], Model: "claude-opus-5-5" }, "model_refusal_fallback");
        Ok(P("""{"type":"system","subtype":"model_refusal_fallback","trigger":"refusal","direction":"retry","scope":"local","original_model":"a","fallback_model":"b","request_id":null,"refused_user_message_uuid":null,"content":"x","uuid":"f","session_id":"s"}""") is FallbackEvt { Retry: null, Retracted: [], Model: null }
           && P("""{"type":"system","subtype":"model_fallback","trigger":"overloaded","original_model":"a","fallback_model":"b","content":"Switched to b: a is overloaded","uuid":"f","session_id":"s"}""") is FallbackEvt { Retry: null, Model: null }
           && P("""{"type":"system","subtype":"model_consent_fallback","choice":"cancelled","original_model":"a","fallback_model":"b","persisted_as_default":false,"content":"Using b","uuid":"f","session_id":"s"}""") is FallbackEvt { Model: "b" }, "turn-scoped and consent fallbacks");
        Ok(P("""{"type":"system","subtype":"session_metadata","metadata":{"artifacts":[{"url":"https://claude.ai/a/1","title":"Report","kind":"frame","updated_at":"2026-10-11T00:00:00Z"},{"url":"https://claude.ai/a/2","kind":"frame","updated_at":"x"},{"url":"javascript:alert(1)","title":"x"}]},"uuid":"u","session_id":"s"}""")
           is ArtifactsEvt { Artifacts: [{ Title: "Report" }, { Url: "https://claude.ai/a/2", Title: "https://claude.ai/a/2" }] }
           && P("""{"type":"system","subtype":"session_metadata","metadata":{"artifacts":null},"uuid":"u","session_id":"s"}""") is ArtifactsEvt { Artifacts: [] }
           && P("""{"type":"system","subtype":"session_metadata","metadata":{},"uuid":"u","session_id":"s"}""") is null, "session_metadata artifacts");

        var tu = P("""{"type":"assistant","message":{"model":"claude-haiku-5-5","id":"msg_1","content":[{"type":"tool_use","id":"toolu_01Y","name":"Write","input":{"file_path":"C:\\w\\b.txt","content":"x"}}]},"parent_tool_use_id":null}""") as ToolUseEvt;
        Ok(tu is { Id: "toolu_01Y", Name: "Write", ParentToolUseId: null } && Str(tu.Input, "file_path") == @"C:\w\b.txt", "tool_use");
        Ok(P("""{"type":"assistant","message":{"model":"<synthetic>","id":"m","content":[{"type":"text","text":"Total cost"}]},"parent_tool_use_id":null}""") is AssistantTextEvt { Synthetic: true, Text: "Total cost", MessageId: "m", Uuid: null }, "synthetic text");
        Ok(P("""{"type":"assistant","message":{"model":"claude-haiku-5-5","id":"msg_2","role":"assistant","content":[{"type":"text","text":"OK"}]},"parent_tool_use_id":null,"session_id":"s","uuid":"66d742fc-2e1b-4c7e-9a43-0f4e3c0b8f11"}""") is AssistantTextEvt { Text: "OK", Uuid: "66d742fc-2e1b-4c7e-9a43-0f4e3c0b8f11", At: null }, "assistant uuid");
        Ok(P("""{"type":"assistant","message":{"id":"m","content":[{"type":"text","text":"OK"}]},"uuid":"u","timestamp":"2026-10-09T12:41:34.602Z"}""") is AssistantTextEvt { At: { } aat } && aat.ToUnixTimeMilliseconds() == 1791549694602, "assistant timestamp (transcript)");

        static List<ClaudeEvent> All(string json) => ParseAll(JsonDocument.Parse(json).RootElement.Clone()).ToList();
        Ok(All("""{"type":"assistant"}""") is [] && All("""{"type":"user","message":"x"}""") is []
           && All("""{"type":"control_request","request_id":"r","request":{"subtype":"can_use_tool","tool_name":"Write"}}""") is [], "malformed lines skipped");
        Ok(All("""{"type":"assistant","message":{"id":"m","content":[{"type":"tool_use","id":"t","name":"Read"},{"type":"text","text":"ok"}]}}""") is [AssistantTextEvt { Text: "ok" }], "block without input skipped, next kept");

        var tr = P("""{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_E","type":"tool_result","content":"The file a.txt has been updated."}]},"parent_tool_use_id":null,"tool_use_result":{"filePath":"C:\\w\\a.txt","oldString":"hello","newString":"bye","originalFile":"hello\n","structuredPatch":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-hello","+bye"]}]}}""") as ToolResultEvt;
        Ok(tr is { ToolUseId: "toolu_E", IsError: false, Structured: { } st } && st.GetProperty("structuredPatch")[0].GetProperty("lines")[1].GetString() == "+bye", "tool_result + tool_use_result");
        Ok(tr?.Structured is { } ts && Prop(ts, "originalFile") is null && Str(ts, "filePath") == @"C:\w\a.txt", "tool_use_result drops originalFile");
        var rd = P("""{"type":"user","message":{"role":"user","content":[{"tool_use_id":"r","type":"tool_result","content":"1→x"}]},"tool_use_result":{"type":"text","file":{"filePath":"a","content":"x","numLines":1}}}""") as ToolResultEvt;
        Ok(rd?.Structured is { } rs && Prop(rs.GetProperty("file"), "content") is null && rs.GetProperty("file").GetProperty("numLines").GetInt32() == 1, "tool_use_result drops file.content");
        Ok(P("""{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":[{"type":"text","text":"Refusé"}],"is_error":true,"tool_use_id":"t"}]},"tool_use_result":"Error: x"}""") is ToolResultEvt { IsError: true, Text: "Refusé", Structured: null, NotRun: false }, "tool_result error");
        // The SDK's tool_result_meta (stream) and the transcript's toolDenialKind (2.1.296): only auto mode's missing answer is "not run".
        Ok(P("""{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"held back","is_error":true,"tool_use_id":"t1"}]},"tool_result_meta":[{"id":"t1","non_execution_kind":"automode-unavailable"}]}""") is ToolResultEvt { NotRun: true }
           && P("""{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"no","is_error":true,"tool_use_id":"t1"}]},"tool_result_meta":[{"id":"t1","non_execution_kind":"automode-blocked"}]}""") is ToolResultEvt { NotRun: false }
           && P("""{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"x","is_error":true,"tool_use_id":"t1"}]},"toolDenialKind":"automode-parsing-error"}""") is ToolResultEvt { NotRun: true }, "not run by auto mode");
        Ok(P("""{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]},"parent_tool_use_id":null,"timestamp":"2026-10-09T12:41:34.602Z"}""") is UserTextEvt { Text: "[Request interrupted by user]", At: not null }, "user text");
        // claude 2.1.296, the Skill tool's injected body (stream-json), trimmed.
        Ok(All("""{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Base directory for this skill: /s\n\n# Pingo"}]},"parent_tool_use_id":null,"isSynthetic":true}""") is []
           && All("""{"type":"user","message":{"role":"user","content":"Base directory for this skill: /s"},"isMeta":true}""") is [], "injected skill body skipped");

        var perm = P("""{"type":"control_request","request_id":"edd9","request":{"subtype":"can_use_tool","tool_name":"Write","display_name":"Write","input":{"file_path":"C:\\w\\b.txt","content":"x"},"description":"b.txt","permission_suggestions":[{"type":"setMode","mode":"acceptEdits","destination":"session"}],"tool_use_id":"toolu_01Y"}}""") as PermissionEvt;
        Ok(perm is { RequestId: "edd9", Tool: "Write", Description: "b.txt", ToolUseId: "toolu_01Y", Suggestions: { } sg } && Str(sg[0], "type") == "setMode", "can_use_tool");
        // claude 2.1.296 with a project ask rule Bash(echo:*): decision_reason_type only, no permission_suggestions.
        Ok(P("""{"type":"control_request","request_id":"a1","request":{"subtype":"can_use_tool","tool_name":"Bash","display_name":"Bash","input":{"command":"echo hi","description":"Print hi"},"description":"Print hi","decision_reason_type":"rule","tool_use_id":"toolu_01Dq"}}""")
           is PermissionEvt { Info: { ReasonType: "rule", AskRule: null, NoAlwaysAllow: false }, Suggestions: null }, "can_use_tool reason type");
        Ok(P("""{"type":"control_request","request_id":"a2","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"git push"},"tool_use_id":"t","agent_id":"a7f3","decision_reason":"Asks before pushing","matched_ask_rule":{"source":"projectSettings","tool_name":"Bash","rule_content":"git push:*"},"suppress_always_allow_rule":true}}""")
           is PermissionEvt { Info: { AgentId: "a7f3", Reason: "Asks before pushing", AskRule: "Bash(git push:*)", AskRuleSource: "projectSettings", NoAlwaysAllow: true } }, "can_use_tool context");
        Ok(perm is { Info: null }, "can_use_tool without context");
        var exit = P("""{"type":"control_request","request_id":"1c0e","request":{"subtype":"can_use_tool","tool_name":"ExitPlanMode","display_name":"ExitPlanMode","input":{"plan":"# Plan: create hello.txt\n\n## Steps\n1. Write `hello.txt`.\n","planFilePath":"/Users/u/.claude/plans/p.md"},"tool_use_id":"toolu_01Cc","requires_user_interaction":true}}""") as PermissionEvt;
        Ok(exit is { Tool: "ExitPlanMode", ToolUseId: "toolu_01Cc", Suggestions: null } && Str(exit.Input, "plan")!.StartsWith("# Plan"), "can_use_tool ExitPlanMode (no suggestions)");

        var ok = P("""{"type":"result","subtype":"success","is_error":false,"duration_ms":2548,"num_turns":3,"total_cost_usd":0.0063816,"usage":{"input_tokens":8,"cache_creation_input_tokens":22821,"cache_read_input_tokens":139210,"iterations":[{"input_tokens":2,"output_tokens":3,"cache_read_input_tokens":41768,"cache_creation_input_tokens":108}]},"modelUsage":{"claude-haiku-5-5":{"costUSD":0.0063816,"contextWindow":1000000}},"terminal_reason":"completed","fast_mode_state":"off","fast_mode_disabled_reason":"sdk_opt_in_required"}""") as ResultEvt;
        Ok(ok is { Subtype: "success", IsError: false, TotalCostUsd: 0.0063816m, DurationMs: 2548, NumTurns: 3, ContextTokens: 41878, ContextWindow: 1000000, TerminalReason: "completed", FastModeReason: "sdk_opt_in_required", CacheRead: 41768, CacheWrite: 108 }, "result success");
        // claude 2.1.296: a Read refused by a project deny rule (never a can_use_tool) is listed in permission_denials.
        Ok(P("""{"type":"result","subtype":"success","is_error":false,"duration_ms":10,"num_turns":2,"total_cost_usd":0.01,"usage":{},"permission_denials":[{"tool_name":"Read","tool_use_id":"toolu_01Ao","tool_input":{"file_path":"/w/secret.txt"}}],"terminal_reason":"completed"}""")
           is ResultEvt { Denied: ["toolu_01Ao"] }, "result permission_denials");
        Ok(ok is { Denied: null }, "result without permission_denials");
        var ab = P("""{"type":"result","subtype":"error_during_execution","is_error":true,"duration_ms":15100,"num_turns":2,"total_cost_usd":0.012436335,"usage":{"input_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"iterations":[]},"terminal_reason":"aborted_streaming"}""") as ResultEvt;
        Ok(ab is { Subtype: "error_during_execution", IsError: true, TerminalReason: "aborted_streaming", ContextTokens: 0, TotalCostUsd: 0.012436335m }, "result interrupted");

        var rl = P("""{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1791552000,"rateLimitType":"five_hour","unifiedWindows":{"five_hour":{"utilization":0.36,"resetsAt":1791552000},"seven_day":{"utilization":0.26,"resetsAt":1791828000}}}}""") as RateLimitEvt;
        Ok(rl is { FiveHour: 0.36, SevenDay: 0.26, Status: "allowed", ExtraOn: false } && rl.FiveHourReset.ToUnixTimeSeconds() == 1791552000, "rate_limit_event");
        // claude 2.1.296 past 75 % of the week, extra usage off for the org (probed).
        Ok(P("""{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1791828000,"rateLimitType":"seven_day","utilization":0.91,"overageStatus":"rejected","overageDisabledReason":"org_level_disabled","isUsingOverage":false,"surpassedThreshold":0.75,"unifiedWindows":{"five_hour":{"utilization":0.05,"resetsAt":1791696000},"seven_day":{"utilization":0.91,"resetsAt":1791828000}},"overageEnabled":false},"uuid":"u","session_id":"s"}""")
           is RateLimitEvt { Status: "allowed_warning", Type: "seven_day", SevenDay: 0.91, ExtraOn: false, ExtraOff: "org_level_disabled" }
           && P("""{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour","overageEnabled":true,"isUsingOverage":true}}""") is RateLimitEvt { Status: "rejected", ExtraInUse: true, FiveHourReset.Ticks: 0 }, "rate_limit_event warning and rejected");

        Ok(P("""{"type":"system","subtype":"task_started","task_id":"ab1","tool_use_id":"toolu_B","description":"Reply pong","subagent_type":"general-purpose","is_backgrounded":false}""") is TaskStartedEvt { TaskId: "ab1", ToolUseId: "toolu_B", Description: "Reply pong", SubagentType: "general-purpose" }, "task_started");
        Ok(P("""{"type":"system","subtype":"task_progress","task_id":"ab1","usage":{"total_tokens":26511,"tool_uses":1,"duration_ms":2092}}""") is TaskProgressEvt { TotalTokens: 26511, ToolUses: 1, DurationMs: 2092, Workflow: null }, "task_progress");
        // claude 2.1.296, a Workflow run (--probe-cli workflow), trimmed.
        Ok(P("""{"type":"system","subtype":"task_progress","task_id":"w0wlsjjl5","run_id":"0mv27zrg4-60a257c6","tool_use_id":"toolu_01KX","description":"Ping: pong:a","usage":{"total_tokens":0,"tool_uses":0,"duration_ms":20},"last_tool_name":"pong:a","summary":"Two agents reply pong","workflow_progress":[{"type":"workflow_phase","index":1,"title":"Ping"},{"type":"workflow_agent","index":1,"label":"pong:a","phaseIndex":1,"phaseTitle":"Ping","state":"start"}]}""")
           is TaskProgressEvt { TaskId: "w0wlsjjl5", DurationMs: 20, Workflow: { } wp } && wp.GetArrayLength() == 2, "task_progress workflow_progress");
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

        Ok(P("""{"type":"prompt_suggestion","suggestion":"write the function","uuid":"4965ab55-46c7-4edc-8391-e478a7290066","session_id":"51318cf7"}""") is SuggestionEvt { Text: "write the function" }, "prompt_suggestion");

        static McpAuthStart A(string json) => McpAuthStart.Parse(JsonDocument.Parse(json).RootElement.Clone());
        Ok(A("""{"authUrl":"http://127.0.0.1:5/authorize?response_type=code&state=s","requiresUserAction":true,"callbackExpected":true,"redirectScheme":"localhost","state":"s","callbackPort":61234}""")
           is { Url: "http://127.0.0.1:5/authorize?response_type=code&state=s", CallbackExpected: true }, "mcp_authenticate authUrl");
        Ok(A("""{"requiresUserAction":false,"callbackExpected":false}""") is { Url: null, CallbackExpected: false }, "mcp_authenticate already signed in");
        Ok(A("""{"authUrl":"javascript:alert(1)","callbackExpected":true}""").Url is null && A("""{"authUrl":"/relative"}""").Url is null, "mcp_authenticate non-http url dropped");
    }
}
