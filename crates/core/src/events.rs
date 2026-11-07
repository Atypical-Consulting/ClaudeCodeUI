//! Port of `Core/Events.cs` — the single parsing point for CLI stdout lines AND transcript (.jsonl) lines.
//! Spec 02 §2-§5. Owner: core-events.
//!
//! Malformed-line rule: a malformed line is skipped, never thrown; a malformed block is skipped and the next block
//! of the same message still parses. Never `unwrap()` a field: go through `ccui_shared::json`.
//! Tests: port every `Check()` of Events.cs (real claude 2.1.296 captures, culture fr) as #[test].

use ccui_shared::hooks::{HookEdit, HookEvent, HookListing, HookRow};
use ccui_shared::json::{self, bool as jbool, long, names, prop, str as jstr};
use ccui_shared::model::*;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// CLI 2.1.296 constant that tells a compaction summary from a skill body.
pub const SUMMARY_LEAD: &str = "This session is being continued from a previous conversation that ran out of context.";

/// `ResultEvt`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ResultEvt {
    pub subtype: String,
    pub is_error: bool,
    /// Cumulative per process: assign, never add.
    pub total_cost_usd: Decimal,
    pub duration_ms: i32,
    pub num_turns: i32,
    /// Last usage iteration: input + cache_read + cache_creation.
    pub context_tokens: i64,
    pub context_window: Option<i64>,
    pub fast_mode_state: Option<String>,
    pub fast_mode_reason: Option<String>,
    pub terminal_reason: Option<String>,
    /// permission_denials[].tool_use_id (None when absent).
    pub denied: Option<Vec<String>>,
    pub cache_read: i64,
    pub cache_write: i64,
}

/// `TaskDoneEvt` (system/task_notification and `<task-notification>` user text).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskDone {
    pub task_id: String,
    pub tool_use_id: String,
    pub status: String,
    pub result: Option<String>,
    pub tokens: i64,
    pub tool_uses: i32,
    pub duration_ms: i32,
}

/// The parsed event union (spec 02 §2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ClaudeEvent {
    Init(InitInfo),
    /// system/commands_changed: the `commands` array.
    Commands(Value),
    PluginStatus { plugin: String, text: Option<String> },
    ElicitDone { server: String, elicitation_id: String },
    Notice(NoticeEvt),
    Status { status: Option<String>, permission_mode: Option<String> },
    Thinking { estimated_tokens: i32 },
    Thought { text: String, parent_tool_use_id: Option<String> },
    Compact { trigger: String, pre_tokens: i64, post_tokens: i64, duration_ms: i32 },
    Summary { text: String },
    Fallback { text: String, retry: Option<String>, retracted: Vec<String>, model: Option<String> },
    Artifacts(Vec<Artifact>),
    TextDelta { text: String, parent_tool_use_id: Option<String> },
    AssistantText {
        message_id: String,
        text: String,
        parent_tool_use_id: Option<String>,
        synthetic: bool,
        uuid: Option<String>,
        at: Option<Time>,
        model: Option<String>,
    },
    ToolUse { id: String, name: String, input: Value, parent_tool_use_id: Option<String>, model: Option<String> },
    ToolResult { tool_use_id: String, text: String, is_error: bool, structured: Option<Value>, not_run: bool },
    UserText { text: String, at: Option<Time>, images: Option<Vec<UserImage>> },
    Permission {
        request_id: String,
        tool: String,
        input: Value,
        description: Option<String>,
        tool_use_id: Option<String>,
        suggestions: Option<Value>,
        info: Option<PermissionInfo>,
    },
    CancelRequest { request_id: String },
    Retry(RetryEvt),
    Result(ResultEvt),
    RateLimit(RateLimits),
    TaskStarted {
        task_id: String,
        tool_use_id: String,
        description: String,
        subagent_type: String,
        task_type: Option<String>,
        backgrounded: bool,
        ambient: bool,
    },
    TaskUpdated { task_id: String, status: Option<String>, ended_at: Option<Time>, backgrounded: bool },
    TaskProgress { task_id: String, total_tokens: i64, tool_uses: i32, duration_ms: i32, workflow: Option<Value> },
    TaskDone(TaskDone),
    Title { title: String },
    Scheduled { prompt: String, at: Option<Time> },
    Pr { url: String, number: Option<String> },
    Reset,
    /// state: queued | started | completed | cancelled | discarded | refused.
    Queue { uuid: String, state: String },
    Suggestion { text: String },
    Hook(HookEvt),
    HookStarted { id: String, name: String },
}

fn s(v: &Value, k: &str) -> Option<String> {
    jstr(v, k).map(str::to_string)
}

fn non_empty(v: &Value, k: &str) -> Option<String> {
    json::non_empty(v, k)
}

/// C# `Uri.TryCreate(u, UriKind.Absolute) && scheme is http or https`: an http(s) scheme followed by a host.
fn http_url(u: &str) -> bool {
    let lower = u.get(..8).unwrap_or(u).to_ascii_lowercase();
    let rest = if lower.starts_with("https://") {
        &u[8..]
    } else if lower.starts_with("http://") {
        &u[7..]
    } else {
        return false;
    };
    rest.chars().next().is_some_and(|c| !matches!(c, '/' | '?' | '#') && !c.is_whitespace())
        && !u.chars().any(|c| c.is_control())
}

/// Every event of one line (one per content block for assistant/user lines). Unknown types → [].
pub fn parse_all(line: &Value) -> Vec<ClaudeEvent> {
    let e = line;
    let mut out = Vec::new();
    match jstr(e, "type") {
        Some("system") => out.extend(parse_system(e)),

        Some("stream_event") => {
            if let Some(se) = prop(e, "event") {
                if jstr(se, "type") == Some("content_block_delta") {
                    if let Some(d) = prop(se, "delta").filter(|d| jstr(d, "type") == Some("text_delta")) {
                        out.push(ClaudeEvent::TextDelta { text: json::string(d, "text"), parent_tool_use_id: s(e, "parent_tool_use_id") });
                    }
                }
            }
        }

        Some("assistant") => {
            // Malformed lines (missing message, id or input) are skipped, never thrown.
            let Some(msg) = prop(e, "message") else { return out };
            let Some(Value::Array(content)) = prop(msg, "content") else { return out };
            let parent = s(e, "parent_tool_use_id");
            let synthetic = jstr(msg, "model") == Some("<synthetic>");
            let model = if synthetic { None } else { s(msg, "model") };
            for c in content {
                match jstr(c, "type") {
                    Some("text") => out.push(ClaudeEvent::AssistantText {
                        message_id: json::string(msg, "id"),
                        text: json::string(c, "text"),
                        parent_tool_use_id: parent.clone(),
                        synthetic,
                        uuid: s(e, "uuid"),
                        at: json::time(e),
                        model: model.clone(),
                    }),
                    Some("thinking") => {
                        if let Some(th) = jstr(c, "thinking").map(str::trim).filter(|t| !t.is_empty()) {
                            out.push(ClaudeEvent::Thought { text: th.to_string(), parent_tool_use_id: parent.clone() });
                        }
                    }
                    Some("tool_use") => {
                        if let (Some(id), Some(input)) = (jstr(c, "id"), prop(c, "input")) {
                            out.push(ClaudeEvent::ToolUse {
                                id: id.to_string(),
                                name: json::string(c, "name"),
                                input: input.clone(),
                                parent_tool_use_id: parent.clone(),
                                model: model.clone(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        Some("user") => {
            // Text the CLI injects itself (a skill's body: isSynthetic on the stream, isMeta in the transcript) is not a turn.
            let parent =
                if jbool(e, "isSynthetic") || jbool(e, "isMeta") { Some("injected".to_string()) } else { s(e, "parent_tool_use_id") };
            let at = json::time(e);
            let Some(content) = prop(e, "message").and_then(|m| prop(m, "content")) else { return out };
            if jstr(e, "turnOrigin") == Some("scheduled") && jbool(e, "isMeta") {
                out.push(ClaudeEvent::Scheduled { prompt: json::result_text(content), at });
                return out;
            }
            if jbool(e, "isCompactSummary") || jbool(e, "isSynthetic") && json::result_text(content).starts_with(SUMMARY_LEAD) {
                out.push(ClaudeEvent::Summary { text: json::result_text(content) });
                return out;
            }
            let user_text = |text: &str, images: Option<Vec<UserImage>>| match notification(text) {
                Some(done) => ClaudeEvent::TaskDone(done),
                None => ClaudeEvent::UserText { text: text.to_string(), at, images },
            };
            let blocks = match content {
                Value::String(text) => {
                    if parent.is_none() {
                        out.push(user_text(text, None));
                    }
                    return out;
                }
                Value::Array(a) => a,
                _ => return out,
            };
            let structured = prop(e, "tool_use_result").filter(|t| t.is_object()).map(slim);
            // Image and document blocks ride on the message's first text, or stand alone in an attachment-only message.
            let images: Vec<UserImage> = if parent.is_none() {
                blocks
                    .iter()
                    .filter(|c| matches!(jstr(c, "type"), Some("image" | "document")))
                    .map(ccui_shared::images::parse)
                    .collect()
            } else {
                Vec::new()
            };
            let mut texts = 0;
            for c in blocks {
                match jstr(c, "type") {
                    Some("tool_result") => {
                        if let Some(id) = jstr(c, "tool_use_id") {
                            let is_error = jbool(c, "is_error");
                            out.push(ClaudeEvent::ToolResult {
                                tool_use_id: id.to_string(),
                                text: json::result_text(c.get("content").unwrap_or(&json::UNDEFINED)),
                                is_error,
                                structured: structured.clone(),
                                not_run: is_error && not_run(e, id),
                            });
                        }
                    }
                    Some("text") if parent.is_none() => {
                        let first = texts == 0;
                        texts += 1;
                        let imgs = (first && !images.is_empty()).then(|| images.clone());
                        out.push(user_text(jstr(c, "text").unwrap_or_default(), imgs));
                    }
                    _ => {}
                }
            }
            if texts == 0 && !images.is_empty() {
                out.push(ClaudeEvent::UserText { text: String::new(), at, images: Some(images) });
            }
        }

        Some("control_request") => {
            let r = json::get(e, "request");
            match (jstr(r, "subtype"), jstr(e, "request_id")) {
                (Some("can_use_tool"), Some(id)) => {
                    if let Some(input) = prop(r, "input") {
                        out.push(ClaudeEvent::Permission {
                            request_id: id.to_string(),
                            tool: json::string(r, "tool_name"),
                            input: input.clone(),
                            description: s(r, "description"),
                            tool_use_id: s(r, "tool_use_id"),
                            suggestions: prop(r, "permission_suggestions").cloned(),
                            info: info(r),
                        });
                    }
                }
                // An MCP elicitation queues like a permission, the whole request as its input (see Elicit).
                (Some("elicitation"), Some(id)) => out.push(ClaudeEvent::Permission {
                    request_id: id.to_string(),
                    tool: ccui_shared::elicit::TOOL.to_string(),
                    input: r.clone(),
                    description: s(r, "message"),
                    tool_use_id: None,
                    suggestions: None,
                    info: info(r),
                }),
                _ => {}
            }
        }

        Some("control_cancel_request") => {
            if let Some(id) = s(e, "request_id") {
                out.push(ClaudeEvent::CancelRequest { request_id: id });
            }
        }

        Some("result") => {
            // Context = the last API call's prompt size; top-level usage sums every iteration of the turn.
            let mut usage = prop(e, "usage");
            if let Some(Value::Array(it)) = usage.and_then(|u| prop(u, "iterations")) {
                if let Some(last) = it.last() {
                    usage = Some(last);
                }
            }
            let u = usage.unwrap_or(&json::UNDEFINED);
            let ctx = long(u, "input_tokens") + long(u, "cache_read_input_tokens") + long(u, "cache_creation_input_tokens");
            let mut window = None;
            if let Some(Value::Object(mu)) = prop(e, "modelUsage") {
                for m in mu.values() {
                    if let Some(w) = json::num(m, "contextWindow") {
                        window = Some(w);
                    }
                }
            }
            out.push(ClaudeEvent::Result(ResultEvt {
                subtype: json::string(e, "subtype"),
                is_error: jbool(e, "is_error"),
                total_cost_usd: json::decimal(e, "total_cost_usd").unwrap_or_default(),
                duration_ms: long(e, "duration_ms") as i32,
                num_turns: long(e, "num_turns") as i32,
                context_tokens: ctx,
                context_window: window,
                fast_mode_state: s(e, "fast_mode_state"),
                fast_mode_reason: s(e, "fast_mode_disabled_reason"),
                terminal_reason: s(e, "terminal_reason"),
                denied: match prop(e, "permission_denials") {
                    Some(Value::Array(pd)) => Some(pd.iter().filter_map(|x| s(x, "tool_use_id")).collect()),
                    _ => None,
                },
                cache_read: long(u, "cache_read_input_tokens"),
                cache_write: long(u, "cache_creation_input_tokens"),
            }));
        }

        Some("conversation_reset") => out.push(ClaudeEvent::Reset),

        Some("command_lifecycle") => {
            if let (Some(uuid), Some(state)) = (s(e, "command_uuid"), s(e, "state")) {
                out.push(ClaudeEvent::Queue { uuid, state });
            }
        }

        Some("prompt_suggestion") => {
            if let Some(text) = non_empty(e, "suggestion") {
                out.push(ClaudeEvent::Suggestion { text });
            }
        }

        Some("rate_limit_event") => {
            if let Some(info) = prop(e, "rate_limit_info") {
                let uw = prop(info, "unifiedWindows");
                let h5 = uw.and_then(|u| prop(u, "five_hour"));
                let d7 = uw.and_then(|u| prop(u, "seven_day"));
                let reset = |w: Option<&Value>| w.and_then(|w| chrono::DateTime::from_timestamp(long(w, "resetsAt"), 0));
                out.push(ClaudeEvent::RateLimit(RateLimits {
                    five_hour: h5.map_or(0.0, |w| json::double(w, "utilization")),
                    five_hour_reset: reset(h5),
                    seven_day: d7.map_or(0.0, |w| json::double(w, "utilization")),
                    seven_day_reset: reset(d7),
                    status: s(info, "status").unwrap_or_else(|| "allowed".into()),
                    kind: s(info, "rateLimitType"),
                    extra_on: jbool(info, "overageEnabled"),
                    extra_in_use: jbool(info, "isUsingOverage"),
                    extra_off: s(info, "overageDisabledReason"),
                }));
            }
        }

        _ => {}
    }
    out
}

/// First of `parse_all`.
pub fn parse(line: &Value) -> Option<ClaudeEvent> {
    parse_all(line).into_iter().next()
}

/// `type: "system"` → at most one event (spec 02 §3.1).
pub fn parse_system(line: &Value) -> Option<ClaudeEvent> {
    let e = line;
    let arr = |k: &str| match prop(e, k) {
        Some(Value::Array(a)) => Some(a),
        _ => None,
    };
    let subtype = jstr(e, "subtype")?;
    Some(match subtype {
        "init" => ClaudeEvent::Init(InitInfo {
            session_id: json::string(e, "session_id"),
            cwd: json::string(e, "cwd"),
            model: json::string(e, "model"),
            permission_mode: json::string(e, "permissionMode"),
            tools: names(e, "tools"),
            mcp: arr("mcp_servers")
                .map(|a| a.iter().map(|m| McpBrief { name: json::string(m, "name"), status: json::string(m, "status") }).collect())
                .unwrap_or_default(),
            slash_commands: names(e, "slash_commands"),
            skills: names(e, "skills"),
            agents: names(e, "agents"),
            plugins: names(e, "plugins"),
            fast_mode_state: s(e, "fast_mode_state").unwrap_or_else(|| "off".into()),
            fast_mode_reason: s(e, "fast_mode_disabled_reason"),
            mcp_errors: arr("mcp_server_errors").map(|a| {
                a.iter()
                    .map(|x| McpSkipped { name: json::string(x, "name"), kind: json::string(x, "type"), message: json::string(x, "message") })
                    .collect()
            }),
            plugin_errors: arr("plugin_errors").map(|a| {
                a.iter()
                    .map(|x| PluginError {
                        plugin: json::string(x, "plugin"),
                        kind: json::string(x, "type"),
                        message: json::string(x, "message"),
                        path: s(x, "path"),
                    })
                    .collect()
            }),
            output_style: s(e, "output_style"),
        }),
        "status" => ClaudeEvent::Status { status: s(e, "status"), permission_mode: s(e, "permissionMode") },
        "thinking_tokens" => ClaudeEvent::Thinking { estimated_tokens: long(e, "estimated_tokens") as i32 },
        "task_started" => ClaudeEvent::TaskStarted {
            task_id: json::string(e, "task_id"),
            tool_use_id: json::string(e, "tool_use_id"),
            description: json::string(e, "description"),
            subagent_type: json::string(e, "subagent_type"),
            task_type: s(e, "task_type"),
            backgrounded: jbool(e, "is_backgrounded"),
            ambient: jbool(e, "ambient"),
        },
        "task_updated" => {
            let patch = prop(e, "patch")?;
            let end = long(patch, "end_time");
            // Out of range = unknown: the reducer uses now.
            let ended_at = if end > 0 && end < 253_402_300_800_000 { chrono::DateTime::from_timestamp_millis(end) } else { None };
            ClaudeEvent::TaskUpdated {
                task_id: json::string(e, "task_id"),
                status: s(patch, "status"),
                ended_at,
                backgrounded: jbool(patch, "is_backgrounded"),
            }
        }
        "task_progress" => {
            let u = prop(e, "usage")?;
            ClaudeEvent::TaskProgress {
                task_id: json::string(e, "task_id"),
                total_tokens: long(u, "total_tokens"),
                tool_uses: long(u, "tool_uses") as i32,
                duration_ms: long(u, "duration_ms") as i32,
                workflow: arr("workflow_progress").map(|_| e["workflow_progress"].clone()),
            }
        }
        "task_notification" => {
            let mut done = TaskDone {
                task_id: json::string(e, "task_id"),
                tool_use_id: json::string(e, "tool_use_id"),
                status: json::string(e, "status"),
                ..Default::default()
            };
            if let Some(nu) = prop(e, "usage") {
                done.tokens = long(nu, "total_tokens");
                done.tool_uses = long(nu, "tool_uses") as i32;
                done.duration_ms = long(nu, "duration_ms") as i32;
            }
            ClaudeEvent::TaskDone(done)
        }
        "api_retry" => ClaudeEvent::Retry(RetryEvt {
            attempt: long(e, "attempt") as i32,
            max_retries: long(e, "max_retries") as i32,
            delay_ms: long(e, "retry_delay_ms") as i32,
            error_status: json::num(e, "error_status").map(|x| x as i32),
            error: s(e, "error"),
        }),
        "session_title_changed" => ClaudeEvent::Title { title: json::string(e, "title") },
        "code_change_published" => {
            let url = jstr(e, "url").filter(|u| http_url(u))?;
            ClaudeEvent::Pr { url: url.to_string(), number: s(e, "identifier") }
        }
        "hook_response" => ClaudeEvent::Hook(HookEvt {
            name: s(e, "hook_name").or_else(|| s(e, "hook_event")).unwrap_or_default(),
            outcome: json::string(e, "outcome"),
            exit_code: json::num(e, "exit_code").map(|x| x as i32),
            output: jstr(e, "output").unwrap_or_default().trim_end().to_string(),
            id: s(e, "hook_id"),
        }),
        "hook_started" => ClaudeEvent::HookStarted {
            id: s(e, "hook_id")?,
            name: s(e, "hook_name").or_else(|| s(e, "hook_event")).unwrap_or_default(),
        },
        "elicitation_complete" => ClaudeEvent::ElicitDone {
            server: json::string(e, "mcp_server_name"),
            elicitation_id: json::string(e, "elicitation_id"),
        },
        "notification" => ClaudeEvent::Notice(NoticeEvt { text: non_empty(e, "text")?, color: s(e, "color") }),
        // ui_log is a plugin's $.ui.log line: the REPL appends it to the transcript as a notice under the plugin's name.
        "ui_toast" | "ui_log" => {
            let text = non_empty(e, "text")?;
            ClaudeEvent::Notice(NoticeEvt {
                text: match jstr(e, "plugin") {
                    Some(pl) => format!("{pl}: {text}"),
                    None => text,
                },
                color: None,
            })
        }
        "ui_status" => ClaudeEvent::PluginStatus { plugin: non_empty(e, "plugin")?, text: non_empty(e, "text") },
        "commands_changed" => ClaudeEvent::Commands(Value::Array(arr("commands")?.clone())),
        "compact_boundary" => {
            let cm = prop(e, "compact_metadata").or_else(|| prop(e, "compactMetadata"))?;
            ClaudeEvent::Compact {
                trigger: json::string(cm, "trigger"),
                pre_tokens: long(cm, "pre_tokens") + long(cm, "preTokens"),
                post_tokens: long(cm, "post_tokens") + long(cm, "postTokens"),
                duration_ms: (long(cm, "duration_ms") + long(cm, "durationMs")) as i32,
            }
        }
        // Transcript lines name these in camelCase. Only a refusal says which message to retry; a session-scoped swap
        // (refusal fallback not "local", consent fallback) changes the model of the turns that follow.
        "model_fallback" | "model_refusal_fallback" | "model_refusal_no_fallback" | "model_consent_fallback" => {
            let text = non_empty(e, "content")?;
            let session_scoped =
                subtype == "model_consent_fallback" || subtype == "model_refusal_fallback" && jstr(e, "scope") != Some("local");
            let mut retracted = names(e, "retracted_message_uuids");
            retracted.extend(names(e, "retractedMessageUuids"));
            ClaudeEvent::Fallback {
                text,
                retry: s(e, "refused_user_message_uuid").or_else(|| s(e, "refusedUserMessageUuid")),
                retracted,
                model: if session_scoped { s(e, "fallback_model").or_else(|| s(e, "fallbackModel")) } else { None },
            }
        }
        "session_metadata" => {
            let arts = prop(e, "metadata")?.as_object()?.get("artifacts")?;
            ClaudeEvent::Artifacts(match arts {
                Value::Array(a) => a
                    .iter()
                    .filter_map(|a| {
                        let url = jstr(a, "url").filter(|u| http_url(u))?;
                        Some(Artifact { url: url.to_string(), title: non_empty(a, "title").unwrap_or_else(|| url.to_string()) })
                    })
                    .collect(),
                _ => Vec::new(),
            })
        }
        _ => return None,
    })
}

/// `Notification(text)`: a `<task-notification>` block → TaskDone (result span cut out before reading the tags).
pub fn notification(text: &str) -> Option<TaskDone> {
    if !text.starts_with("<task-notification>") {
        return None;
    }
    // Same as the C# lazy, single-line regex `<n>(.*?)</n>`: the first open tag, then the first close after it.
    fn tag<'a>(t: &'a str, n: &str) -> Option<&'a str> {
        let open = format!("<{n}>");
        let start = t.find(&open)? + open.len();
        let len = t[start..].find(&format!("</{n}>"))?;
        Some(&t[start..start + len])
    }
    let r = text.find("<result>");
    let re = text.rfind("</result>");
    let (result, rest) = match (r, re) {
        // Tags below must not match inside the result.
        (Some(r), Some(re)) if re > r => (Some(text[r + 8..re].to_string()), format!("{}{}", &text[..r], &text[re + 9..])),
        _ => (None, text.to_string()),
    };
    let num = |n: &str| tag(&rest, n).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
    Some(TaskDone {
        task_id: tag(&rest, "task-id").unwrap_or_default().to_string(),
        tool_use_id: tag(&rest, "tool-use-id").unwrap_or_default().to_string(),
        status: tag(&rest, "status").unwrap_or_default().to_string(),
        result,
        tokens: num("subagent_tokens"),
        tool_uses: i32::try_from(num("tool_uses")).unwrap_or(0),
        duration_ms: i32::try_from(num("duration_ms")).unwrap_or(0),
    })
}

/// tool_use_result minus `originalFile` and `file.content`.
pub fn slim(tool_use_result: &Value) -> Value {
    let mut o = tool_use_result.clone();
    if let Some(m) = o.as_object_mut() {
        m.shift_remove("originalFile");
        if let Some(Value::Object(f)) = m.get_mut("file") {
            f.shift_remove("content");
        }
    }
    o
}

/// toolDenialKind (transcript) ?? tool_result_meta[id].non_execution_kind (stream) ∈ {automode-unavailable,
/// automode-parsing-error}.
pub fn not_run(line: &Value, tool_use_id: &str) -> bool {
    let kind = jstr(line, "toolDenialKind").or_else(|| match prop(line, "tool_result_meta") {
        Some(Value::Array(meta)) => meta.iter().find(|m| jstr(m, "id") == Some(tool_use_id)).and_then(|m| jstr(m, "non_execution_kind")),
        _ => None,
    });
    matches!(kind, Some("automode-unavailable" | "automode-parsing-error"))
}

/// `Info(request)` → PermissionInfo, None when every field is empty/false.
pub fn info(request: &Value) -> Option<PermissionInfo> {
    let r = request;
    let ask = prop(r, "matched_ask_rule");
    let info = PermissionInfo {
        agent_id: s(r, "agent_id"),
        title: s(r, "title"),
        blocked_path: s(r, "blocked_path"),
        reason_type: s(r, "decision_reason_type"),
        reason: s(r, "decision_reason"),
        ask_rule: ask.map(|a| {
            let name = jstr(a, "tool_name").unwrap_or_default();
            match jstr(a, "rule_content") {
                Some(c) => format!("{name}({c})"),
                None => name.to_string(),
            }
        }),
        ask_rule_source: ask.and_then(|a| s(a, "source")),
        no_always_allow: jbool(r, "suppress_always_allow_rule"),
    };
    (info != PermissionInfo::default()).then_some(info)
}

/// `get_hooks_listing` response → HookListing (spec 02 §5.1).
pub fn hooks(listing: &Value) -> HookListing {
    let rows = match prop(listing, "hooks") {
        Some(Value::Array(a)) => a
            .iter()
            .map(|h| {
                let display = s(h, "displayText");
                let command = s(h, "commandText").or_else(|| display.clone()).unwrap_or_default();
                HookRow {
                    event: json::string(h, "event"),
                    matcher: json::string(h, "matcher"),
                    kind: json::string(h, "type"),
                    display: display.filter(|d| *d != command),
                    command,
                    source: json::string(h, "source"),
                    source_label: s(h, "sourceLabel"),
                    plugin_name: s(h, "pluginName"),
                    timeout: json::num(h, "timeout").map(|t| t as i32),
                    disabled: jbool(h, "disabled"),
                    edit: prop(h, "editable").and_then(|ed| {
                        let cfg = prop(ed, "config").filter(|c| c.is_object())?;
                        Some(HookEdit { matcher: json::string(ed, "matcher"), config: cfg.clone() })
                    }),
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    let p = json::get(listing, "policy");
    HookListing {
        rows,
        disabled: jbool(p, "allDisabled") || jbool(p, "disabledByPolicy"),
        locked: jbool(p, "disabledByPolicy") || jbool(p, "managedOnly") || jbool(p, "pluginOnly") || jbool(p, "policyUnreadable"),
        catalog: match prop(listing, "eventCatalog") {
            Some(Value::Array(ec)) => ec
                .iter()
                .map(|x| HookEvent { name: json::string(x, "name"), summary: json::string(x, "summary"), matcher: jbool(x, "supportsMatcher") })
                .filter(|x| !x.name.is_empty())
                .collect(),
            _ => Vec::new(),
        },
    }
}

/// `mcp_authenticate` response → McpAuthStart (spec 02 §5.3).
pub fn mcp_auth(response: &Value) -> McpAuthStart {
    McpAuthStart {
        url: jstr(response, "authUrl").filter(|u| http_url(u)).map(str::to_string),
        callback_expected: jbool(response, "callbackExpected"),
    }
}

/// `get_status` response → sections (spec 02 §5.4).
pub fn status_sections(response: &Value) -> Vec<StatusSection> {
    let Some(Value::Array(sections)) = prop(response, "sections") else { return Vec::new() };
    sections
        .iter()
        .map(|x| StatusSection {
            title: json::string(x, "title"),
            rows: match prop(x, "rows") {
                Some(Value::Array(rows)) => {
                    rows.iter().map(|w| StatusRow { label: s(w, "label"), value: json::string(w, "value") }).collect()
                }
                _ => Vec::new(),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ClaudeEvent as E;

    fn v(json: &str) -> Value {
        serde_json::from_str(json).expect("test json")
    }
    fn p(json: &str) -> Option<ClaudeEvent> {
        parse(&v(json))
    }
    fn all(json: &str) -> Vec<ClaudeEvent> {
        parse_all(&v(json))
    }

    #[test]
    fn init() {
        let Some(E::Init(i)) = p(
            r#"{"type":"system","subtype":"init","cwd":"C:\\w","session_id":"s1","tools":["Task","Edit","PowerShell"],"mcp_servers":[{"name":"plugin:github:github","status":"failed","source":"plugin"},{"name":"x","status":"connected"}],"model":"claude-haiku-5-5","permissionMode":"default","slash_commands":["cost"],"skills":["impeccable"],"agents":["Explore"],"plugins":[{"name":"context7","path":"C:\\p","source":"x"}],"fast_mode_state":"off","fast_mode_disabled_reason":"sdk_opt_in_required"}"#,
        ) else {
            panic!("init")
        };
        assert_eq!((i.session_id.as_str(), i.cwd.as_str(), i.model.as_str()), ("s1", r"C:\w", "claude-haiku-5-5"));
        assert_eq!(i.permission_mode, "default");
        assert_eq!(i.fast_mode_reason.as_deref(), Some("sdk_opt_in_required"));
        assert!(i.tools.contains(&"PowerShell".to_string()));
        assert_eq!(i.mcp.len(), 2);
        assert_eq!(i.mcp[0].status, "failed");
        assert_eq!(i.mcp[1].name, "x");
        assert_eq!(i.plugins, vec!["context7"]);
        assert_eq!(i.skills, vec!["impeccable"]);
        assert!(i.mcp_errors.is_none() && i.output_style.is_none() && i.plugin_errors.is_none());

        assert!(matches!(
            p(r#"{"type":"system","subtype":"init","session_id":"s","output_style":"Concise","memory_paths":{"auto":"/u/.claude/projects/-w/memory/"}}"#),
            Some(E::Init(InitInfo { output_style: Some(ref o), .. })) if o == "Concise"
        ));
        // claude 2.1.296 with --mcp-config holding {"bad":{"url":…}} (no type).
        let Some(E::Init(i)) = p(
            r#"{"type":"system","subtype":"init","session_id":"s","mcp_servers":[{"name":"probe","status":"connected","source":"dynamic"}],"mcp_server_errors":[{"name":"bad","type":"url_missing_type","message":"Skipped — MCP server \"bad\" has a \"url\" but no \"type\""}]}"#,
        ) else {
            panic!()
        };
        let errs = i.mcp_errors.unwrap();
        assert_eq!(errs.len(), 1);
        assert_eq!((errs[0].name.as_str(), errs[0].kind.as_str()), ("bad", "url_missing_type"));
        assert!(errs[0].message.starts_with("Skipped"));
        // claude 2.1.296 with --plugin-dir ./badplug (absent).
        let Some(E::Init(i)) = p(
            r#"{"type":"system","subtype":"init","session_id":"s","plugins":[],"plugin_errors":[{"plugin":"inline[0]","type":"path-not-found","message":"Path not found: /w/badplug (commands)","path":"/w/badplug"}]}"#,
        ) else {
            panic!()
        };
        let pe = i.plugin_errors.unwrap();
        assert_eq!((pe[0].plugin.as_str(), pe[0].kind.as_str(), pe[0].path.as_deref()), ("inline[0]", "path-not-found", Some("/w/badplug")));
        assert_eq!(i.fast_mode_state, "off");
    }

    #[test]
    fn elicitation() {
        let Some(E::Permission { request_id, tool, input, description, tool_use_id, info, .. }) = p(
            r#"{"type":"control_request","request_id":"9627","request":{"subtype":"elicitation","mcp_server_name":"probe","message":"What is your name?","mode":"form","requested_schema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]},"title":"Sign up"}}"#,
        ) else {
            panic!("elicitation form")
        };
        assert_eq!((request_id.as_str(), tool.as_str()), ("9627", ccui_shared::elicit::TOOL));
        assert_eq!(description.as_deref(), Some("What is your name?"));
        assert!(tool_use_id.is_none());
        assert_eq!(info.unwrap().title.as_deref(), Some("Sign up"));
        assert_eq!(jstr(&input, "mcp_server_name"), Some("probe"));

        let Some(E::Permission { tool, input, info, .. }) = p(
            r#"{"type":"control_request","request_id":"4717","request":{"subtype":"elicitation","mcp_server_name":"probe","message":"Open the page","mode":"url","url":"https://example.com/link?x=1","elicitation_id":"el-42"}}"#,
        ) else {
            panic!("elicitation url")
        };
        assert!(tool == ccui_shared::elicit::TOOL && info.is_none() && jstr(&input, "mode") == Some("url"));
        assert_eq!(
            p(r#"{"type":"system","subtype":"elicitation_complete","mcp_server_name":"probe","elicitation_id":"el-42","uuid":"6be3","session_id":"b0f2"}"#),
            Some(E::ElicitDone { server: "probe".into(), elicitation_id: "el-42".into() })
        );
    }

    #[test]
    fn notices_plugins_commands() {
        assert_eq!(
            p(r#"{"type":"system","subtype":"notification","key":"error-compacting-conversation","text":"Error compacting conversation","priority":"immediate","color":"error","uuid":"u","session_id":"s"}"#),
            Some(E::Notice(NoticeEvt { text: "Error compacting conversation".into(), color: Some("error".into()) }))
        );
        assert_eq!(
            p(r#"{"type":"system","subtype":"ui_toast","plugin":"impeccable","text":"Saved","timeout_ms":4000,"uuid":"u","session_id":"s"}"#),
            Some(E::Notice(NoticeEvt { text: "impeccable: Saved".into(), color: None }))
        );
        assert_eq!(p(r#"{"type":"system","subtype":"notification","key":"k","text":"","priority":"low"}"#), None);
        assert_eq!(
            p(r#"{"type":"system","subtype":"ui_log","plugin":"demo","text":"indexed 12 files","uuid":"u","session_id":"s"}"#),
            Some(E::Notice(NoticeEvt { text: "demo: indexed 12 files".into(), color: None }))
        );
        assert_eq!(
            p(r#"{"type":"system","subtype":"ui_status","plugin":"demo","text":"syncing","uuid":"u","session_id":"s"}"#),
            Some(E::PluginStatus { plugin: "demo".into(), text: Some("syncing".into()) })
        );
        assert_eq!(
            p(r#"{"type":"system","subtype":"ui_status","plugin":"demo","text":null,"uuid":"u","session_id":"s"}"#),
            Some(E::PluginStatus { plugin: "demo".into(), text: None })
        );
        assert!(matches!(
            p(r#"{"type":"system","subtype":"commands_changed","commands":[{"name":"cua-driver-rs","description":"Drive a GUI app (user)","argumentHint":"","aliases":["cua-driver"]}],"uuid":"u","session_id":"s"}"#),
            Some(E::Commands(Value::Array(ref a))) if a.len() == 1
        ));
    }

    #[test]
    fn stream_hooks_retry_cancel() {
        assert_eq!(
            p(r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"done"}},"parent_tool_use_id":null}"#),
            Some(E::TextDelta { text: "done".into(), parent_tool_use_id: None })
        );
        assert_eq!(
            p(r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"","estimated_tokens":50}}}"#),
            None
        );
        assert_eq!(
            p(r#"{"type":"system","subtype":"hook_started","hook_id":"d11d","hook_name":"SessionStart:startup","hook_event":"SessionStart","uuid":"6e4d","session_id":"7f85"}"#),
            Some(E::HookStarted { id: "d11d".into(), name: "SessionStart:startup".into() })
        );
        assert_eq!(p(r#"{"type":"system","subtype":"hook_started"}"#), None);
        assert_eq!(
            p(r#"{"type":"system","subtype":"api_retry","attempt":2,"max_retries":10,"retry_delay_ms":4000,"error_status":529,"error":"overloaded"}"#),
            Some(E::Retry(RetryEvt { attempt: 2, max_retries: 10, delay_ms: 4000, error_status: Some(529), error: Some("overloaded".into()) }))
        );
        assert!(matches!(
            p(r#"{"type":"system","subtype":"api_retry","attempt":1,"max_retries":10,"retry_delay_ms":500,"error_status":null,"error":"unknown"}"#),
            Some(E::Retry(RetryEvt { error_status: None, .. }))
        ));
        assert_eq!(p(r#"{"type":"control_cancel_request","request_id":"r9"}"#), Some(E::CancelRequest { request_id: "r9".into() }));
        assert_eq!(
            p(r#"{"type":"system","subtype":"task_updated","task_id":"b6","patch":{"is_backgrounded":true}}"#),
            Some(E::TaskUpdated { task_id: "b6".into(), status: None, ended_at: None, backgrounded: true })
        );
        assert_eq!(
            p(r#"{"type":"system","subtype":"hook_response","hook_id":"98f5","hook_name":"PreToolUse:Write","hook_event":"PreToolUse","output":"nope: blocked\n","stdout":"","stderr":"nope: blocked\n","exit_code":2,"outcome":"error","uuid":"7e61","session_id":"d427"}"#),
            Some(E::Hook(HookEvt {
                name: "PreToolUse:Write".into(),
                outcome: "error".into(),
                exit_code: Some(2),
                output: "nope: blocked".into(),
                id: Some("98f5".into())
            }))
        );
    }

    #[test]
    fn hooks_listing() {
        let l = hooks(&v(
            r#"{"events":[],"hooks":[{"event":"PreToolUse","matcher":"Bash|PowerShell","source":"projectSettings","sourceLabel":"Project settings (.claude/settings.json)","type":"command","displayText":"echo ok","commandText":"echo ok","contentLabel":"Command","timeout":5,"editable":{}},{"event":"Stop","matcher":"","source":"pluginHook","sourceLabel":"Plugin hooks (~/.claude/plugins/*/hooks/hooks.json)","pluginName":"impeccable@impeccable","type":"command","displayText":"Design deep pass","commandText":"impeccable hook","contentLabel":"Command","statusMessage":"Design deep pass"}],"policy":{"disabledByPolicy":false,"managedOnly":false,"pluginOnly":false,"allDisabled":false,"policyHookCount":0}}"#,
        ));
        assert!(!l.disabled && !l.locked && l.catalog.is_empty() && l.rows.len() == 2);
        let (a, b) = (&l.rows[0], &l.rows[1]);
        assert_eq!((a.event.as_str(), a.matcher.as_str(), a.kind.as_str(), a.command.as_str()), ("PreToolUse", "Bash|PowerShell", "command", "echo ok"));
        assert!(a.display.is_none() && a.source == "projectSettings" && a.plugin_name.is_none() && a.timeout == Some(5) && a.edit.is_none());
        assert_eq!((b.event.as_str(), b.matcher.as_str(), b.command.as_str()), ("Stop", "", "impeccable hook"));
        assert_eq!(b.display.as_deref(), Some("Design deep pass"));
        assert!(b.source == "pluginHook" && b.plugin_name.as_deref() == Some("impeccable@impeccable") && b.timeout.is_none() && b.edit.is_none());
        assert!(!a.blocks_on_failure());

        let ed = hooks(&v(
            r#"{"hooks":[{"event":"PostToolUse","matcher":"","source":"projectSettings","sourceLabel":"Project settings (.claude/settings.json)","type":"command","displayText":"echo bye","commandText":"echo bye","contentLabel":"Command","editable":{"matcher":"","config":{"type":"command","command":"echo bye","onFailure":"block"}}}],"eventCatalog":[{"name":"PreToolUse","summary":"Before tool execution","supportsMatcher":true},{"name":"Stop","summary":"Right before Claude concludes its response","supportsMatcher":false}],"policy":{"disabledByPolicy":false,"managedOnly":false,"pluginOnly":false,"allDisabled":false,"policyHookCount":0,"policyUnreadable":true}}"#,
        ));
        assert!(ed.locked && !ed.disabled && ed.rows.len() == 1 && ed.rows[0].blocks_on_failure());
        let e1 = ed.rows[0].edit.as_ref().unwrap();
        assert!(e1.matcher.is_empty() && jstr(&e1.config, "command") == Some("echo bye"));
        assert_eq!(ed.catalog.len(), 2);
        assert!(ed.catalog[0].name == "PreToolUse" && ed.catalog[0].matcher && ed.catalog[1].name == "Stop" && !ed.catalog[1].matcher);
        assert!(hooks(&v(r#"{"hooks":[],"policy":{"allDisabled":true}}"#)).disabled);
        // claude 2.1.296, project settings with "disableAllHooks": true: every row carries "disabled": true.
        let d = hooks(&v(
            r#"{"hooks":[{"event":"Stop","matcher":"","source":"projectSettings","type":"command","displayText":"echo a","commandText":"echo a","disabled":true}],"policy":{"allDisabled":true}}"#,
        ));
        assert!(d.rows.len() == 1 && d.rows[0].disabled && l.rows.iter().all(|h| !h.disabled));
    }

    #[test]
    fn thinking_status_compact_summary() {
        assert_eq!(
            p(r#"{"type":"system","subtype":"thinking_tokens","estimated_tokens":250,"estimated_tokens_delta":200}"#),
            Some(E::Thinking { estimated_tokens: 250 })
        );
        assert_eq!(
            p(r#"{"type":"system","subtype":"status","status":null,"permissionMode":"acceptEdits"}"#),
            Some(E::Status { status: None, permission_mode: Some("acceptEdits".into()) })
        );
        assert_eq!(
            p(r#"{"type":"system","subtype":"status","status":"compacting","session_id":"f9ce","uuid":"c24f"}"#),
            Some(E::Status { status: Some("compacting".into()), permission_mode: None })
        );
        assert_eq!(
            p(r#"{"type":"system","subtype":"compact_boundary","session_id":"f9ce","uuid":"9b6e","compact_metadata":{"trigger":"manual","pre_tokens":37317,"post_tokens":8995,"cumulative_dropped_tokens":28322,"duration_ms":3207},"logical_parent_uuid":"e464"}"#),
            Some(E::Compact { trigger: "manual".into(), pre_tokens: 37317, post_tokens: 8995, duration_ms: 3207 })
        );
        assert_eq!(
            p(r#"{"parentUuid":null,"logicalParentUuid":"e464","isSidechain":false,"type":"system","subtype":"compact_boundary","content":"Conversation compacted","isMeta":false,"timestamp":"2026-10-11T00:26:19.756Z","uuid":"9b6e","level":"info","compactMetadata":{"trigger":"auto","preTokens":37317,"durationMs":3207,"postTokens":8995,"cumulativeDroppedTokens":28322}}"#),
            Some(E::Compact { trigger: "auto".into(), pre_tokens: 37317, post_tokens: 8995, duration_ms: 3207 })
        );
        let summary = format!(r"{SUMMARY_LEAD} The summary below covers the earlier portion of the conversation.\n\nSummary:\n1. Primary Request");
        let stream = r#"{"type":"user","message":{"role":"user","content":"SUM"},"session_id":"s","parent_tool_use_id":null,"uuid":"30d0","timestamp":"2026-10-11T00:45:05.600Z","isReplay":false,"isSynthetic":true}"#;
        assert!(matches!(p(&stream.replace("SUM", &summary)), Some(E::Summary { ref text }) if text.ends_with("Primary Request")));
        let transcript = r#"{"parentUuid":"9b6e","isSidechain":false,"type":"user","message":{"role":"user","content":"SUM"},"isVisibleInTranscriptOnly":true,"isCompactSummary":true,"uuid":"ec30","timestamp":"2026-10-11T00:26:19.754Z"}"#;
        assert!(matches!(p(&transcript.replace("SUM", &summary)), Some(E::Summary { .. })));
        // --thinking-display summarized: the thinking block carries the summary; by default it is "".
        assert_eq!(
            all(r#"{"type":"assistant","message":{"model":"claude-haiku-5-5","id":"msg_1","content":[{"type":"thinking","thinking":"Checking 391: 17 × 23 = 391, so it's not prime.\n\n","signature":"CAQS"}]},"parent_tool_use_id":null}"#),
            vec![E::Thought { text: "Checking 391: 17 × 23 = 391, so it's not prime.".into(), parent_tool_use_id: None }]
        );
        let a = all(r#"{"type":"assistant","message":{"id":"m","content":[{"type":"thinking","thinking":"","signature":"CAQS"},{"type":"text","text":"hi"}]}}"#);
        assert!(matches!(a.as_slice(), [E::AssistantText { .. }]));
    }

    #[test]
    fn fallbacks_and_artifacts() {
        assert_eq!(
            p(r#"{"type":"system","subtype":"model_refusal_fallback","trigger":"refusal","direction":"retry","scope":"session","original_model":"claude-fable-5","fallback_model":"claude-opus-5-5","request_id":null,"api_refusal_category":"cyber","retracted_message_uuids":["a1","a2"],"refused_user_message_uuid":"u7","content":"Fable 5 declined; retried with Opus 5.5","uuid":"f1","session_id":"s"}"#),
            Some(E::Fallback {
                text: "Fable 5 declined; retried with Opus 5.5".into(),
                retry: Some("u7".into()),
                retracted: vec!["a1".into(), "a2".into()],
                model: Some("claude-opus-5-5".into())
            })
        );
        assert!(matches!(
            p(r#"{"type":"system","subtype":"model_refusal_fallback","trigger":"refusal","direction":"retry","scope":"local","original_model":"a","fallback_model":"b","request_id":null,"refused_user_message_uuid":null,"content":"x","uuid":"f","session_id":"s"}"#),
            Some(E::Fallback { retry: None, ref retracted, model: None, .. }) if retracted.is_empty()
        ));
        assert!(matches!(
            p(r#"{"type":"system","subtype":"model_fallback","trigger":"overloaded","original_model":"a","fallback_model":"b","content":"Switched to b: a is overloaded","uuid":"f","session_id":"s"}"#),
            Some(E::Fallback { retry: None, model: None, .. })
        ));
        assert!(matches!(
            p(r#"{"type":"system","subtype":"model_consent_fallback","choice":"cancelled","original_model":"a","fallback_model":"b","persisted_as_default":false,"content":"Using b","uuid":"f","session_id":"s"}"#),
            Some(E::Fallback { model: Some(ref m), .. }) if m == "b"
        ));
        assert_eq!(
            p(r#"{"type":"system","subtype":"session_metadata","metadata":{"artifacts":[{"url":"https://claude.ai/a/1","title":"Report","kind":"frame","updated_at":"2026-10-11T00:00:00Z"},{"url":"https://claude.ai/a/2","kind":"frame","updated_at":"x"},{"url":"javascript:alert(1)","title":"x"}]},"uuid":"u","session_id":"s"}"#),
            Some(E::Artifacts(vec![
                Artifact { url: "https://claude.ai/a/1".into(), title: "Report".into() },
                Artifact { url: "https://claude.ai/a/2".into(), title: "https://claude.ai/a/2".into() },
            ]))
        );
        assert_eq!(
            p(r#"{"type":"system","subtype":"session_metadata","metadata":{"artifacts":null},"uuid":"u","session_id":"s"}"#),
            Some(E::Artifacts(vec![]))
        );
        assert_eq!(p(r#"{"type":"system","subtype":"session_metadata","metadata":{},"uuid":"u","session_id":"s"}"#), None);
    }

    #[test]
    fn assistant_lines() {
        let Some(E::ToolUse { id, name, input, parent_tool_use_id, .. }) = p(
            r#"{"type":"assistant","message":{"model":"claude-haiku-5-5","id":"msg_1","content":[{"type":"tool_use","id":"toolu_01Y","name":"Write","input":{"file_path":"C:\\w\\b.txt","content":"x"}}]},"parent_tool_use_id":null}"#,
        ) else {
            panic!("tool_use")
        };
        assert!(id == "toolu_01Y" && name == "Write" && parent_tool_use_id.is_none() && jstr(&input, "file_path") == Some(r"C:\w\b.txt"));
        assert!(matches!(
            p(r#"{"type":"assistant","message":{"model":"<synthetic>","id":"m","content":[{"type":"text","text":"Total cost"}]},"parent_tool_use_id":null}"#),
            Some(E::AssistantText { synthetic: true, ref text, ref message_id, uuid: None, .. }) if text == "Total cost" && message_id == "m"
        ));
        assert!(matches!(
            p(r#"{"type":"assistant","message":{"model":"claude-haiku-5-5","id":"msg_2","role":"assistant","content":[{"type":"text","text":"OK"}]},"parent_tool_use_id":null,"session_id":"s","uuid":"66d742fc-2e1b-4c7e-9a43-0f4e3c0b8f11"}"#),
            Some(E::AssistantText { ref text, uuid: Some(ref u), at: None, .. }) if text == "OK" && u == "66d742fc-2e1b-4c7e-9a43-0f4e3c0b8f11"
        ));
        assert!(matches!(
            p(r#"{"type":"assistant","message":{"id":"m","content":[{"type":"text","text":"OK"}]},"uuid":"u","timestamp":"2026-10-09T12:41:34.602Z"}"#),
            Some(E::AssistantText { at: Some(t), .. }) if t.timestamp_millis() == 1791549694602
        ));
        assert!(all(r#"{"type":"assistant"}"#).is_empty() && all(r#"{"type":"user","message":"x"}"#).is_empty());
        assert!(all(r#"{"type":"control_request","request_id":"r","request":{"subtype":"can_use_tool","tool_name":"Write"}}"#).is_empty());
        assert!(matches!(
            all(r#"{"type":"assistant","message":{"id":"m","content":[{"type":"tool_use","id":"t","name":"Read"},{"type":"text","text":"ok"}]}}"#).as_slice(),
            [E::AssistantText { text, .. }] if text == "ok"
        ));
        // A subagent's message names its model; a synthetic one does not.
        assert!(matches!(
            all(r#"{"type":"assistant","message":{"model":"claude-haiku-5-5","id":"m","content":[{"type":"text","text":"hi"},{"type":"tool_use","id":"t1","name":"Read","input":{}}]},"parent_tool_use_id":"toolu_A"}"#).as_slice(),
            [E::AssistantText { model: Some(m1), parent_tool_use_id: Some(pa), .. }, E::ToolUse { model: Some(m2), .. }]
                if m1 == "claude-haiku-5-5" && pa == "toolu_A" && m2 == "claude-haiku-5-5"
        ));
        assert!(matches!(
            all(r#"{"type":"assistant","message":{"model":"<synthetic>","id":"m","content":[{"type":"text","text":"x"}]}}"#).as_slice(),
            [E::AssistantText { model: None, .. }]
        ));
    }

    #[test]
    fn user_lines() {
        let Some(E::ToolResult { tool_use_id, is_error, structured: Some(st), .. }) = p(
            r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"toolu_E","type":"tool_result","content":"The file a.txt has been updated."}]},"parent_tool_use_id":null,"tool_use_result":{"filePath":"C:\\w\\a.txt","oldString":"hello","newString":"bye","originalFile":"hello\n","structuredPatch":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-hello","+bye"]}]}}"#,
        ) else {
            panic!("tool_result + tool_use_result")
        };
        assert!(tool_use_id == "toolu_E" && !is_error);
        assert_eq!(st["structuredPatch"][0]["lines"][1], "+bye");
        assert!(prop(&st, "originalFile").is_none() && jstr(&st, "filePath") == Some(r"C:\w\a.txt"));
        let keys: Vec<_> = st.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["filePath", "oldString", "newString", "structuredPatch"], "key order kept");

        let Some(E::ToolResult { structured: Some(rs), .. }) = p(
            r#"{"type":"user","message":{"role":"user","content":[{"tool_use_id":"r","type":"tool_result","content":"1→x"}]},"tool_use_result":{"type":"text","file":{"filePath":"a","content":"x","numLines":1}}}"#,
        ) else {
            panic!()
        };
        assert!(prop(&rs["file"], "content").is_none() && rs["file"]["numLines"] == 1);
        assert_eq!(
            p(r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":[{"type":"text","text":"Refusé"}],"is_error":true,"tool_use_id":"t"}]},"tool_use_result":"Error: x"}"#),
            Some(E::ToolResult { tool_use_id: "t".into(), text: "Refusé".into(), is_error: true, structured: None, not_run: false })
        );
        // The SDK's tool_result_meta (stream) and the transcript's toolDenialKind: only auto mode's missing answer is "not run".
        assert!(matches!(
            p(r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"held back","is_error":true,"tool_use_id":"t1"}]},"tool_result_meta":[{"id":"t1","non_execution_kind":"automode-unavailable"}]}"#),
            Some(E::ToolResult { not_run: true, .. })
        ));
        assert!(matches!(
            p(r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"no","is_error":true,"tool_use_id":"t1"}]},"tool_result_meta":[{"id":"t1","non_execution_kind":"automode-blocked"}]}"#),
            Some(E::ToolResult { not_run: false, .. })
        ));
        assert!(matches!(
            p(r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"x","is_error":true,"tool_use_id":"t1"}]},"toolDenialKind":"automode-parsing-error"}"#),
            Some(E::ToolResult { not_run: true, .. })
        ));
        assert!(matches!(
            p(r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]},"parent_tool_use_id":null,"timestamp":"2026-10-09T12:41:34.602Z"}"#),
            Some(E::UserText { ref text, at: Some(_), images: None }) if text == "[Request interrupted by user]"
        ));
        // claude 2.1.296, the Skill tool's injected body (stream-json), trimmed.
        assert!(all(r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Base directory for this skill: /s\n\n# Pingo"}]},"parent_tool_use_id":null,"isSynthetic":true}"#).is_empty());
        assert!(all(r#"{"type":"user","message":{"role":"user","content":"Base directory for this skill: /s"},"isMeta":true}"#).is_empty());
        // The scheduled prompt as the 2.1.296 transcript writes it (probed, CronCreate).
        assert!(matches!(
            p(r#"{"parentUuid":"e376","isSidechain":false,"type":"user","message":{"role":"user","content":"Reply with the single word tick"},"isMeta":true,"uuid":"d592","timestamp":"2026-10-11T03:19:00.431Z","promptSource":"system","turnOrigin":"scheduled"}"#),
            Some(E::Scheduled { ref prompt, at: Some(_) }) if prompt == "Reply with the single word tick"
        ));
        assert_eq!(p(r#"{"type":"user","message":{"role":"user","content":"skill body"},"isMeta":true}"#), None);
    }

    #[test]
    fn permissions() {
        let Some(E::Permission { request_id, tool, description, tool_use_id, suggestions: Some(sg), info, .. }) = p(
            r#"{"type":"control_request","request_id":"edd9","request":{"subtype":"can_use_tool","tool_name":"Write","display_name":"Write","input":{"file_path":"C:\\w\\b.txt","content":"x"},"description":"b.txt","permission_suggestions":[{"type":"setMode","mode":"acceptEdits","destination":"session"}],"tool_use_id":"toolu_01Y"}}"#,
        ) else {
            panic!("can_use_tool")
        };
        assert!(request_id == "edd9" && tool == "Write" && description.as_deref() == Some("b.txt") && tool_use_id.as_deref() == Some("toolu_01Y"));
        assert_eq!(jstr(&sg[0], "type"), Some("setMode"));
        assert!(info.is_none(), "can_use_tool without context");
        // A project ask rule Bash(echo:*): decision_reason_type only, no permission_suggestions.
        assert!(matches!(
            p(r#"{"type":"control_request","request_id":"a1","request":{"subtype":"can_use_tool","tool_name":"Bash","display_name":"Bash","input":{"command":"echo hi","description":"Print hi"},"description":"Print hi","decision_reason_type":"rule","tool_use_id":"toolu_01Dq"}}"#),
            Some(E::Permission { info: Some(PermissionInfo { reason_type: Some(ref rt), ask_rule: None, no_always_allow: false, .. }), suggestions: None, .. }) if rt == "rule"
        ));
        let Some(E::Permission { info: Some(i), .. }) = p(
            r#"{"type":"control_request","request_id":"a2","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"git push"},"tool_use_id":"t","agent_id":"a7f3","decision_reason":"Asks before pushing","matched_ask_rule":{"source":"projectSettings","tool_name":"Bash","rule_content":"git push:*"},"suppress_always_allow_rule":true}}"#,
        ) else {
            panic!("can_use_tool context")
        };
        assert_eq!(i.agent_id.as_deref(), Some("a7f3"));
        assert_eq!(i.reason.as_deref(), Some("Asks before pushing"));
        assert_eq!(i.ask_rule.as_deref(), Some("Bash(git push:*)"));
        assert_eq!(i.ask_rule_source.as_deref(), Some("projectSettings"));
        assert!(i.no_always_allow);
        let Some(E::Permission { tool, tool_use_id, suggestions: None, input, .. }) = p(
            r##"{"type":"control_request","request_id":"1c0e","request":{"subtype":"can_use_tool","tool_name":"ExitPlanMode","display_name":"ExitPlanMode","input":{"plan":"# Plan: create hello.txt\n\n## Steps\n1. Write `hello.txt`.\n","planFilePath":"/Users/u/.claude/plans/p.md"},"tool_use_id":"toolu_01Cc","requires_user_interaction":true}}"##,
        ) else {
            panic!("ExitPlanMode")
        };
        assert!(tool == "ExitPlanMode" && tool_use_id.as_deref() == Some("toolu_01Cc") && jstr(&input, "plan").unwrap().starts_with("# Plan"));
    }

    #[test]
    fn results() {
        let Some(E::Result(ok)) = p(
            r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":2548,"num_turns":3,"total_cost_usd":0.0063816,"usage":{"input_tokens":8,"cache_creation_input_tokens":22821,"cache_read_input_tokens":139210,"iterations":[{"input_tokens":2,"output_tokens":3,"cache_read_input_tokens":41768,"cache_creation_input_tokens":108}]},"modelUsage":{"claude-haiku-5-5":{"costUSD":0.0063816,"contextWindow":1000000}},"terminal_reason":"completed","fast_mode_state":"off","fast_mode_disabled_reason":"sdk_opt_in_required"}"#,
        ) else {
            panic!()
        };
        assert_eq!(
            ok,
            ResultEvt {
                subtype: "success".into(),
                is_error: false,
                total_cost_usd: "0.0063816".parse().unwrap(),
                duration_ms: 2548,
                num_turns: 3,
                context_tokens: 41878,
                context_window: Some(1000000),
                fast_mode_state: Some("off".into()),
                fast_mode_reason: Some("sdk_opt_in_required".into()),
                terminal_reason: Some("completed".into()),
                denied: None,
                cache_read: 41768,
                cache_write: 108,
            }
        );
        assert!(matches!(
            p(r#"{"type":"result","subtype":"success","is_error":false,"duration_ms":10,"num_turns":2,"total_cost_usd":0.01,"usage":{},"permission_denials":[{"tool_name":"Read","tool_use_id":"toolu_01Ao","tool_input":{"file_path":"/w/secret.txt"}}],"terminal_reason":"completed"}"#),
            Some(E::Result(ResultEvt { denied: Some(ref d), .. })) if d == &["toolu_01Ao"]
        ));
        let Some(E::Result(ab)) = p(
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"duration_ms":15100,"num_turns":2,"total_cost_usd":0.012436335,"usage":{"input_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"iterations":[]},"terminal_reason":"aborted_streaming"}"#,
        ) else {
            panic!()
        };
        assert!(ab.subtype == "error_during_execution" && ab.is_error && ab.terminal_reason.as_deref() == Some("aborted_streaming"));
        assert_eq!((ab.context_tokens, ab.total_cost_usd), (0, "0.012436335".parse().unwrap()));
    }

    #[test]
    fn rate_limits() {
        let Some(E::RateLimit(rl)) = p(
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1791552000,"rateLimitType":"five_hour","unifiedWindows":{"five_hour":{"utilization":0.36,"resetsAt":1791552000},"seven_day":{"utilization":0.26,"resetsAt":1791828000}}}}"#,
        ) else {
            panic!()
        };
        assert!(rl.five_hour == 0.36 && rl.seven_day == 0.26 && rl.status == "allowed" && !rl.extra_on);
        assert_eq!(rl.five_hour_reset.unwrap().timestamp(), 1791552000);
        let Some(E::RateLimit(w)) = p(
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed_warning","resetsAt":1791828000,"rateLimitType":"seven_day","utilization":0.91,"overageStatus":"rejected","overageDisabledReason":"org_level_disabled","isUsingOverage":false,"surpassedThreshold":0.75,"unifiedWindows":{"five_hour":{"utilization":0.05,"resetsAt":1791696000},"seven_day":{"utilization":0.91,"resetsAt":1791828000}},"overageEnabled":false},"uuid":"u","session_id":"s"}"#,
        ) else {
            panic!()
        };
        assert!(w.status == "allowed_warning" && w.kind.as_deref() == Some("seven_day") && w.seven_day == 0.91 && !w.extra_on);
        assert_eq!(w.extra_off.as_deref(), Some("org_level_disabled"));
        let Some(E::RateLimit(r)) = p(
            r#"{"type":"rate_limit_event","rate_limit_info":{"status":"rejected","rateLimitType":"five_hour","overageEnabled":true,"isUsingOverage":true}}"#,
        ) else {
            panic!()
        };
        assert!(r.status == "rejected" && r.extra_in_use && r.five_hour_reset.is_none());
    }

    #[test]
    fn tasks() {
        assert!(matches!(
            p(r#"{"type":"system","subtype":"task_started","task_id":"ab1","tool_use_id":"toolu_B","description":"Reply pong","subagent_type":"general-purpose","is_backgrounded":false}"#),
            Some(E::TaskStarted { ref task_id, ref tool_use_id, ref description, ref subagent_type, .. })
                if task_id == "ab1" && tool_use_id == "toolu_B" && description == "Reply pong" && subagent_type == "general-purpose"
        ));
        assert_eq!(
            p(r#"{"type":"system","subtype":"task_progress","task_id":"ab1","usage":{"total_tokens":26511,"tool_uses":1,"duration_ms":2092}}"#),
            Some(E::TaskProgress { task_id: "ab1".into(), total_tokens: 26511, tool_uses: 1, duration_ms: 2092, workflow: None })
        );
        assert!(matches!(
            p(r#"{"type":"system","subtype":"task_progress","task_id":"w0wlsjjl5","run_id":"0mv27zrg4-60a257c6","tool_use_id":"toolu_01KX","description":"Ping: pong:a","usage":{"total_tokens":0,"tool_uses":0,"duration_ms":20},"last_tool_name":"pong:a","summary":"Two agents reply pong","workflow_progress":[{"type":"workflow_phase","index":1,"title":"Ping"},{"type":"workflow_agent","index":1,"label":"pong:a","phaseIndex":1,"phaseTitle":"Ping","state":"start"}]}"#),
            Some(E::TaskProgress { ref task_id, duration_ms: 20, workflow: Some(Value::Array(ref wp)), .. }) if task_id == "w0wlsjjl5" && wp.len() == 2
        ));
        assert_eq!(
            p(r#"{"type":"system","subtype":"task_notification","task_id":"ab1","tool_use_id":"toolu_B","status":"completed","usage":{"total_tokens":26702,"tool_uses":1,"duration_ms":3000}}"#),
            Some(E::TaskDone(TaskDone {
                task_id: "ab1".into(),
                tool_use_id: "toolu_B".into(),
                status: "completed".into(),
                result: None,
                tokens: 26702,
                tool_uses: 1,
                duration_ms: 3000
            }))
        );
        assert_eq!(
            p(r#"{"type":"user","message":{"role":"user","content":"<task-notification>\n<task-id>aa5aab0eece92a372</task-id>\n<tool-use-id>toolu_01TEgqvrrwU4RMLtNJYrEavF</tool-use-id>\n<status>completed</status>\n<summary>Agent finished</summary>\n<result>pong</result>\n<usage><subagent_tokens>31599</subagent_tokens><tool_uses>1</tool_uses><duration_ms>5088</duration_ms></usage>\n</task-notification>"},"parent_tool_use_id":null}"#),
            Some(E::TaskDone(TaskDone {
                task_id: "aa5aab0eece92a372".into(),
                tool_use_id: "toolu_01TEgqvrrwU4RMLtNJYrEavF".into(),
                status: "completed".into(),
                result: Some("pong".into()),
                tokens: 31599,
                tool_uses: 1,
                duration_ms: 5088
            }))
        );
        assert!(matches!(
            p(r#"{"type":"system","subtype":"task_started","task_id":"b69el8u68","run_id":"0mv1jw258-f850d2f1","tool_use_id":"toolu_01JF","description":"Run 40-tick background loop","is_backgrounded":true,"task_type":"local_bash"}"#),
            Some(E::TaskStarted { ref task_id, task_type: Some(ref tt), backgrounded: true, .. }) if task_id == "b69el8u68" && tt == "local_bash"
        ));
        assert!(matches!(
            p(r#"{"type":"system","subtype":"task_started","task_id":"m1","tool_use_id":"toolu_M","description":"github/search","task_type":"mcp_task","uuid":"u","session_id":"s"}"#),
            Some(E::TaskStarted { backgrounded: false, ambient: false, .. })
        ));
        assert!(matches!(
            p(r#"{"type":"system","subtype":"task_started","task_id":"d1","description":"scan","task_type":"auto_mode_scan","ambient":true}"#),
            Some(E::TaskStarted { ambient: true, .. })
        ));
        assert!(matches!(
            p(r#"{"type":"system","subtype":"task_updated","task_id":"b69el8u68","run_id":"0mv1jw258-f850d2f1","patch":{"status":"killed","end_time":1791585746931}}"#),
            Some(E::TaskUpdated { ref status, ended_at: Some(end), .. }) if status.as_deref() == Some("killed") && end.timestamp_millis() == 1791585746931
        ));
        assert!(matches!(
            p(r#"{"type":"system","subtype":"task_updated","task_id":"x","patch":{"status":"failed","end_time":1e300}}"#),
            Some(E::TaskUpdated { ended_at: None, .. })
        ));
        assert!(matches!(
            p(r#"{"type":"system","subtype":"task_updated","task_id":"x","patch":{"status":"failed","end_time":1791585746931.5}}"#),
            Some(E::TaskUpdated { ended_at: Some(fe), .. }) if fe.timestamp_millis() == 1791585746931
        ));
    }

    #[test]
    fn misc_lines() {
        assert_eq!(p(r#"{"type":"system","subtype":"session_title_changed","title":"probe-session"}"#), Some(E::Title { title: "probe-session".into() }));
        assert_eq!(
            p(r#"{"type":"system","subtype":"code_change_published","provider":"github","url":"https://github.com/o/r/pull/42","repo":"o/r","identifier":"42","action":"created","uuid":"u","session_id":"s"}"#),
            Some(E::Pr { url: "https://github.com/o/r/pull/42".into(), number: Some("42".into()) })
        );
        assert_eq!(p(r#"{"type":"system","subtype":"code_change_published","url":"javascript:alert(1)","identifier":"1"}"#), None);
        assert_eq!(
            status_sections(&v(
                r#"{"sections":[{"title":"Session","rows":[{"label":"Version","value":"2.1.296"},{"label":"Login method","value":"Claude Max account"}]},{"title":"Environment","rows":[{"value":"A sentence row"}]}]}"#
            )),
            vec![
                StatusSection {
                    title: "Session".into(),
                    rows: vec![
                        StatusRow { label: Some("Version".into()), value: "2.1.296".into() },
                        StatusRow { label: Some("Login method".into()), value: "Claude Max account".into() },
                    ]
                },
                StatusSection { title: "Environment".into(), rows: vec![StatusRow { label: None, value: "A sentence row".into() }] },
            ]
        );
        assert_eq!(
            p(r#"{"type":"conversation_reset","new_conversation_id":"c7d8","uuid":"c7d8","trigger":"clear","user_message_uuid":"a8d3","timestamp":"2026-10-09T21:32:36.965Z","session_id":"cf56"}"#),
            Some(E::Reset)
        );
        assert_eq!(
            p(r#"{"type":"command_lifecycle","command_uuid":"64bc9ab2-94bb-49c2-8486-8978e4f94126","state":"cancelled","uuid":"e26f0eba-0e60-45ce-87e3-7fb748c98d6c","session_id":"f076ad9a"}"#),
            Some(E::Queue { uuid: "64bc9ab2-94bb-49c2-8486-8978e4f94126".into(), state: "cancelled".into() })
        );
        assert_eq!(
            p(r#"{"type":"prompt_suggestion","suggestion":"write the function","uuid":"4965ab55-46c7-4edc-8391-e478a7290066","session_id":"51318cf7"}"#),
            Some(E::Suggestion { text: "write the function".into() })
        );
    }

    #[test]
    fn mcp_authenticate() {
        assert_eq!(
            mcp_auth(&v(
                r#"{"authUrl":"http://127.0.0.1:5/authorize?response_type=code&state=s","requiresUserAction":true,"callbackExpected":true,"redirectScheme":"localhost","state":"s","callbackPort":61234}"#
            )),
            McpAuthStart { url: Some("http://127.0.0.1:5/authorize?response_type=code&state=s".into()), callback_expected: true }
        );
        assert_eq!(mcp_auth(&v(r#"{"requiresUserAction":false,"callbackExpected":false}"#)), McpAuthStart { url: None, callback_expected: false });
        assert!(mcp_auth(&v(r#"{"authUrl":"javascript:alert(1)","callbackExpected":true}"#)).url.is_none());
        assert!(mcp_auth(&v(r#"{"authUrl":"/relative"}"#)).url.is_none());
    }

    #[test]
    fn http_urls_and_notification_edges() {
        assert!(http_url("HTTPS://x.y/z") && http_url("http://a"));
        assert!(!http_url("https://") && !http_url("https:///x") && !http_url("ftp://x") && !http_url("é"));
        assert_eq!(notification("plain"), None);
        // A tag inside the result must not be read as the task's.
        let d = notification("<task-notification><result><status>fake</status></result><status>ok</status>").unwrap();
        assert_eq!((d.status.as_str(), d.result.as_deref()), ("ok", Some("<status>fake</status>")));
        let d = notification("<task-notification><tool_uses> 3 </tool_uses><duration_ms>x</duration_ms>").unwrap();
        assert_eq!((d.tool_uses, d.duration_ms, d.result), (3, 0, None));
    }
}
