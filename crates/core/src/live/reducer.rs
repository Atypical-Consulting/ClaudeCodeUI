//! Port of `LiveSession.Reduce` and its parts (`OnResult`, rate-limit merge, `SubagentModel`, `Reloaded`) —
//! spec 01 §7.4-7.7. Owner: core-live-session.
//! Pure over `SessionState` + `now`: no I/O, no locking (the caller holds the gate). Transcript replay runs it too.
//! Tests: LiveSession.Check() cases (cost accounting, rate-limit merge, queue/drain, denied vs NotRun…).

use super::state::{task_running, SessionState};
use crate::events::{ClaudeEvent, ResultEvt};
use ccui_shared::api_errors::ApiError;
use ccui_shared::json;
use ccui_shared::model::*;
use ccui_shared::session_rules::{self, INTERRUPTED, PUSH_TOOL};
use ccui_shared::tool_kinds;
use serde_json::{Map, Value};

/// `o` with `key` set to `v` (the C# `With`: initialize_info stays a value, replaced whole). Not an object → `o`.
pub(crate) fn with_key(o: &Value, key: &str, v: Value) -> Value {
    let mut o = o.clone();
    if let Value::Object(m) = &mut o {
        m.insert(key.to_string(), v);
    }
    o
}

impl SessionState {
    /// Fold one event (rules in spec order; prelude: last_event_at, retry cleared by progress, hook removed from
    /// running_hooks).
    pub fn reduce(&mut self, e: &ClaudeEvent, now: Time) {
        self.last_event_at = now;
        if !matches!(e, ClaudeEvent::Retry(_) | ClaudeEvent::RateLimit(_) | ClaudeEvent::Hook(_) | ClaudeEvent::HookStarted { .. }) {
            self.retry = None;
        }
        if let ClaudeEvent::Hook(HookEvt { id: Some(hid), .. }) = e {
            self.running_hooks.retain(|h| &h.id != hid);
        }
        match e {
            ClaudeEvent::HookStarted { id, name } => {
                let hook = RunningHook { id: id.clone(), name: name.clone(), at: now };
                match self.running_hooks.iter_mut().find(|h| &h.id == id) {
                    Some(h) => *h = hook,
                    None => self.running_hooks.push(hook),
                }
            }

            ClaudeEvent::Retry(rt) => self.retry = Some(rt.clone()),

            // The CLI withdrew the request (e.g. a dangerous rm left unanswered): the card goes, the tool's result follows.
            ClaudeEvent::CancelRequest { request_id } => self.resolve(request_id, Decision::Allow),

            ClaudeEvent::Init(i) => {
                self.heavy.init = Some(i.clone());
                if !i.cwd.is_empty() {
                    self.cwd = i.cwd.clone();
                }
                if !i.model.is_empty() {
                    self.model = Some(i.model.clone());
                }
                if !i.permission_mode.is_empty() {
                    self.mode = i.permission_mode.clone();
                }
                self.fast_mode_state = i.fast_mode_state.clone();
                self.fast_mode_reason = i.fast_mode_reason.clone();
                if let Some(os) = &i.output_style {
                    self.output_style = Some(os.clone());
                }
            }

            ClaudeEvent::Status { status, permission_mode } => {
                if let Some(pm) = permission_mode {
                    self.mode = pm.clone();
                }
                // A compaction ends on the next status: {status:null, compact_result} or "requesting".
                self.compacting_since = if status.as_deref() == Some("compacting") { self.compacting_since.or(Some(now)) } else { None };
                if status.as_deref() == Some("requesting") {
                    self.resumed_turn(now);
                }
            }

            // CLAUDE_CODE_RESUME_INTERRUPTED_TURN: the relaunched CLI re-runs the cut-off turn with no user frame.
            ClaudeEvent::Queue { state, .. } if state == "started" && self.resume_turn && self.queued.is_empty() => self.resumed_turn(now),

            ClaudeEvent::Compact { trigger, pre_tokens, post_tokens, duration_ms } => self.push_item(Item::Compact(CompactItem {
                trigger: trigger.clone(),
                pre_tokens: *pre_tokens,
                post_tokens: *post_tokens,
                duration_ms: *duration_ms,
                summary: None,
            })),

            // The summary follows its boundary (SessionStart:compact hooks may come between, on the stream).
            ClaudeEvent::Summary { text } => {
                if let Some(ci) = self.items().iter().rposition(|i| matches!(i, Item::Compact(_))) {
                    if let Item::Compact(c) = &self.items()[ci] {
                        if c.summary.is_none() {
                            let mut c = c.clone();
                            c.summary = Some(text.clone());
                            self.set_item(ci, Item::Compact(c));
                        }
                    }
                }
            }

            ClaudeEvent::Thought { text, parent_tool_use_id: None } => self.push_item(Item::Thought(ThoughtItem { text: text.clone() })),

            // A refusal fallback retracts the refused partial (its uuids) before the retried answer comes.
            ClaudeEvent::Fallback { text, retry, retracted, model } => {
                if !retracted.is_empty() {
                    self.remove_items(|i| matches!(i, Item::Text(t) if t.uuid.as_ref().is_some_and(|u| retracted.contains(u))));
                    self.clear_stream();
                }
                if let Some(m) = model {
                    self.model = Some(m.clone());
                }
                self.push_item(Item::Fallback(FallbackItem { text: text.clone(), retry: retry.clone() }));
            }

            ClaudeEvent::Artifacts(a) => self.artifacts = a.clone(),

            ClaudeEvent::Thinking { estimated_tokens } => self.thinking_tokens = *estimated_tokens,

            ClaudeEvent::TextDelta { text, parent_tool_use_id: None } => self.stream.push_str(text),

            ClaudeEvent::AssistantText { text, parent_tool_use_id, synthetic, uuid, at, model, .. } => {
                match (*synthetic && parent_tool_use_id.is_none()).then(|| ApiError::parse(text)).flatten() {
                    Some(err) => {
                        self.clear_stream();
                        self.push_item(Item::ApiError(ApiErrorItem { error: err }));
                    }
                    None => {
                        if parent_tool_use_id.is_none() {
                            self.clear_stream();
                        }
                        self.subagent_model(parent_tool_use_id.as_deref(), model.as_deref());
                        self.push_item(Item::Text(TextItem {
                            markdown: text.clone(),
                            parent_tool_use_id: parent_tool_use_id.clone(),
                            uuid: uuid.clone(),
                            at: Some(at.unwrap_or(now)),
                        }));
                    }
                }
            }

            ClaudeEvent::Scheduled { prompt, at } => {
                self.push_item(Item::Scheduled(ScheduledItem { prompt: prompt.clone(), at: at.unwrap_or(now) }))
            }

            ClaudeEvent::ToolUse { id, name, input, parent_tool_use_id, model } => {
                if parent_tool_use_id.is_none() {
                    self.clear_stream();
                }
                self.subagent_model(parent_tool_use_id.as_deref(), model.as_deref());
                self.push_item(Item::Tool(ToolItem {
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                    parent_tool_use_id: parent_tool_use_id.clone(),
                    state: ToolState::Running,
                    result_text: None,
                    structured: None,
                    started_at: now,
                    ended_at: None,
                    task_id: None,
                    task_status: None,
                    progress: None,
                    tokens: 0,
                    sub_tool_uses: 0,
                    background: false,
                    model: None,
                }));
                self.tool_count += 1;
                if name == PUSH_TOOL {
                    self.pushes += 1;
                    self.last_push = json::str(input, "message").map(str::to_string);
                }
            }

            ClaudeEvent::ToolResult { tool_use_id, text, is_error, structured, not_run } => {
                let Some(ti) = self.tool_index(tool_use_id) else { return };
                let t = self.items()[ti].as_tool().cloned().unwrap();
                let launched = structured.as_ref().filter(|rs| {
                    t.state == ToolState::Running
                        && (tool_kinds::is_agent(&t.name) || t.name == ccui_shared::workflow::TOOL)
                        && json::str(rs, "status") == Some("async_launched")
                });
                if let Some(rs) = launched {
                    // launch receipt, not the outcome: stays Running until the task-notification
                    let task_id = json::str(rs, "agentId").or_else(|| json::str(rs, "taskId")).map(str::to_string);
                    let rs = rs.clone();
                    self.update_tool(ti, |x| {
                        x.background = true;
                        x.task_id = task_id.or(x.task_id.take());
                        x.structured = Some(rs);
                    });
                    return;
                }
                self.update_tool(ti, |x| {
                    x.state = if *not_run {
                        ToolState::NotRun
                    } else if *is_error {
                        if x.state == ToolState::Denied { ToolState::Denied } else { ToolState::Error }
                    } else {
                        ToolState::Done
                    };
                    x.result_text = Some(text.clone());
                    x.structured = structured.clone();
                    x.ended_at = Some(now);
                    if let Some(tok) = structured.as_ref().and_then(|s| s.get("totalTokens")).and_then(Value::as_f64) {
                        x.tokens = tok as i64;
                    }
                });
                // A subagent's EnterWorktree moves only that agent: top-level calls move the session.
                if !*is_error && t.parent_tool_use_id.is_none() {
                    if let Some(moved) = session_rules::worktree_move(&t.name, structured.as_ref()).filter(|m| !m.is_empty()) {
                        self.cwd = moved;
                        if t.name == "ExitWorktree" {
                            self.worktree = None;
                        }
                    }
                }
            }

            ClaudeEvent::UserText { text, at, images } => {
                let Some(text) = session_rules::user_text(text) else { return };
                let item = Item::User(UserItem { at: at.unwrap_or(now), ultracode: false, images: images.clone(), uuid: None, text });
                // A transcript writes the /compact line after the boundary and summary it produced: it goes back above.
                let above = matches!(&item, Item::User(u) if u.text.starts_with("/compact")) && matches!(self.items().last(), Some(Item::Compact(_)));
                if above {
                    self.insert_item(self.items().len() - 1, item);
                } else {
                    self.push_item(item);
                }
            }

            ClaudeEvent::Permission { request_id, tool, input, description, tool_use_id, suggestions, info } => {
                self.pending.push(PendingPermission {
                    request_id: request_id.clone(),
                    tool: tool.clone(),
                    input: input.clone(),
                    description: description.clone(),
                    tool_use_id: tool_use_id.clone(),
                    suggestions: suggestions.clone(),
                    at: now,
                    info: info.clone(),
                    target_exists: None,
                });
                if let Some(i) = tool_use_id.as_deref().and_then(|id| self.tool_index(id)) {
                    self.update_tool(i, |t| t.state = ToolState::Waiting);
                }
                self.status = SessionStatus::Waiting;
            }

            ClaudeEvent::Result(r) => self.on_result(r, now),

            ClaudeEvent::RateLimit(l) => self.on_rate_limit(l, now),

            ClaudeEvent::TaskStarted { task_id, tool_use_id, description, task_type, backgrounded, ambient, .. } => {
                if let Some(i) = self.tool_index(tool_use_id) {
                    self.update_tool(i, |t| t.task_id = Some(task_id.clone()));
                }
                // A shell only when backgrounded; agents and workflows have their own rows. Any other type only exists
                // in the background; housekeeping (ambient) stays out.
                let tt = task_type.as_deref();
                let listed = (tt == Some("local_bash") && *backgrounded)
                    || (!matches!(tt, None | Some("local_bash" | "local_agent" | "local_workflow")) && !*ambient);
                if listed && !self.tasks.iter().any(|x| &x.id == task_id) {
                    self.tasks.push(BgTask {
                        id: task_id.clone(),
                        tool_use_id: tool_use_id.clone(),
                        description: description.clone(),
                        started_at: now,
                        kind: task_type.clone().filter(|t| t != "local_bash"),
                        status: "running".into(),
                        ended_at: None,
                    });
                }
            }

            ClaudeEvent::TaskUpdated { task_id, status, ended_at, backgrounded } => {
                // Moved to the background mid-run: a shell joins the background tasks, an agent ends on its notification.
                if let Some(bi) = self.task_tool_index(task_id).filter(|_| *backgrounded) {
                    self.update_tool(bi, |t| t.background = true);
                    let bl = self.items()[bi].as_tool().cloned().unwrap();
                    if tool_kinds::is_shell(&bl.name) && !self.tasks.iter().any(|x| &x.id == task_id) {
                        let description = json::str(&bl.input, "description")
                            .map(str::to_string)
                            .unwrap_or_else(|| tool_kinds::target(&bl.name, &bl.input, &self.cwd));
                        self.tasks.push(BgTask {
                            id: task_id.clone(),
                            tool_use_id: bl.id.clone(),
                            description,
                            started_at: bl.started_at,
                            kind: None,
                            status: "running".into(),
                            ended_at: None,
                        });
                    }
                } else if let (Some(status), Some(bt)) = (status, self.tasks.iter_mut().find(|x| &x.id == task_id)) {
                    bt.status = status.clone();
                    bt.ended_at = if task_running(bt) { None } else { Some(ended_at.unwrap_or(now)) };
                }
            }

            ClaudeEvent::TaskProgress { task_id, total_tokens, tool_uses, workflow, .. } => {
                if let Some(i) = self.task_tool_index(task_id) {
                    self.update_tool(i, |t| {
                        t.tokens = *total_tokens;
                        t.sub_tool_uses = *tool_uses;
                        if workflow.is_some() {
                            t.progress = workflow.clone(); // some carry no snapshot
                        }
                    });
                }
            }

            ClaudeEvent::TaskDone(td) => {
                let i = self
                    .tool_index(&td.tool_use_id)
                    .or_else(|| (!td.task_id.is_empty()).then(|| self.task_tool_index(&td.task_id)).flatten());
                // A resumed background agent notifies again.
                let Some(i) = i.filter(|i| self.items()[*i].as_tool().is_some_and(|t| t.state == ToolState::Running || t.background)) else {
                    return;
                };
                self.update_tool(i, |t| {
                    t.state = if td.status == "completed" { ToolState::Done } else { ToolState::Error };
                    t.task_status = Some(td.status.clone());
                    t.ended_at = Some(if td.duration_ms > 0 { t.started_at + chrono::Duration::milliseconds(td.duration_ms as i64) } else { now });
                    if td.result.is_some() {
                        t.result_text = td.result.clone();
                    }
                    if td.tokens > 0 {
                        t.tokens = td.tokens;
                    }
                    if td.tool_uses > 0 {
                        t.sub_tool_uses = td.tool_uses;
                    }
                });
            }

            // The uuid is ours (Send). A message sent while idle is already in Items: only queued ones move here.
            ClaudeEvent::Queue { uuid, state } if state != "queued" => {
                let Some(qi) = self.queued.iter().position(|m| &m.uuid == uuid) else { return };
                let m = self.queued.remove(qi);
                if state == "started" {
                    // Folded into an ultracode turn, or started right after one: labelled ultracode (the CLI very likely
                    // runs it with ultracode still on, before the ultracode:false lands).
                    let ultracode = self.turn_ultra || (self.draining && self.ultra_carry);
                    self.push_item(Item::User(UserItem { text: m.text, at: now, ultracode, images: None, uuid: Some(m.uuid) }));
                    self.draining = false;
                    if self.status == SessionStatus::Idle {
                        self.status = SessionStatus::Running;
                        self.turn_started_at = Some(now);
                    }
                } else if self.draining && self.queued.is_empty() {
                    // cancelled / discarded / refused before it could start
                    self.draining = false;
                    self.turn_started_at = None;
                    if self.status == SessionStatus::Running {
                        self.status = SessionStatus::Idle;
                    }
                }
            }

            ClaudeEvent::Hook(h) if session_rules::shown(h) => self.push_item(Item::Hook(HookItem { hook: h.clone() })),

            ClaudeEvent::Title { title } if !title.is_empty() => {
                self.name = title.clone();
                self.untitled = false;
            }

            ClaudeEvent::Pr { url, .. } => self.pr_url = Some(url.clone()),

            ClaudeEvent::Reset => {
                self.push_item(Item::Reset);
                self.suggestion = None;
            }

            ClaudeEvent::Notice(n) => self.push_item(Item::Notice(NoticeItem { notice: n.clone() })),

            ClaudeEvent::PluginStatus { plugin, text } => match text {
                Some(line) => {
                    self.plugin_status.insert(plugin.clone(), line.clone());
                }
                None => {
                    self.plugin_status.remove(plugin);
                }
            },

            ClaudeEvent::Suggestion { text } => self.suggestion = Some(text.clone()),

            ClaudeEvent::Commands(c) => {
                if let Some(info) = &self.heavy.initialize_info {
                    self.heavy.initialize_info = Some(with_key(info, "commands", c.clone()));
                }
            }

            _ => {}
        }
    }

    /// §7.5: cost (cumulative per process, cost_base), last turn, context, denied → Denied, end tools, drain queue.
    pub fn on_result(&mut self, r: &ResultEvt, now: Time) {
        // cumulative per process: assign, never add (slash commands report 0); a relaunch restarts at 0
        if r.total_cost_usd > rust_decimal::Decimal::ZERO {
            self.last_turn_cost_usd = self.cost_base + r.total_cost_usd - self.cost_usd;
            self.cost_usd = self.cost_base + r.total_cost_usd;
        } else {
            self.last_turn_cost_usd = rust_decimal::Decimal::ZERO;
        }
        self.last_turn_ms = Some(r.duration_ms as i64);
        self.last_result_subtype = Some(if r.terminal_reason.as_deref().is_some_and(|t| t.starts_with("aborted")) {
            INTERRUPTED.to_string()
        } else {
            r.subtype.clone()
        });
        self.last_result_at = Some(now);
        if r.context_tokens > 0 {
            (self.context_tokens, self.cache_read, self.cache_write) = (r.context_tokens, r.cache_read, r.cache_write);
        }
        if let Some(w) = r.context_window {
            self.context_window = Some(w);
        }
        if let Some(fs) = &r.fast_mode_state {
            self.fast_mode_state = fs.clone();
            self.fast_mode_reason = r.fast_mode_reason.clone();
        }
        // A call refused by a deny rule or the auto-mode classifier never asked: only its error result and this list say so.
        for id in r.denied.iter().flatten() {
            if let Some(i) = self.tool_index(id).filter(|i| self.items()[*i].as_tool().is_some_and(|t| t.state == ToolState::Error)) {
                self.update_tool(i, |t| t.state = ToolState::Denied);
            }
        }
        self.end_tools(now, true);
        self.pending.clear();
        self.clear_stream();
        self.thinking_tokens = 0;
        self.compacting_since = None;
        self.running_hooks.clear(); // one that never answered (an interrupted turn) must not hold the line
        if !self.queued.is_empty() {
            // a queued message runs next, even after an interrupt
            self.draining = true;
            self.ultra_carry = self.turn_ultra; // read before on_event resets it
            self.turn_started_at = Some(now);
            if matches!(self.status, SessionStatus::Waiting | SessionStatus::Starting) {
                self.status = SessionStatus::Running; // Esc while a permission waits
            }
            return;
        }
        self.turn_started_at = None;
        if matches!(self.status, SessionStatus::Running | SessionStatus::Waiting | SessionStatus::Starting) {
            self.status = SessionStatus::Idle;
        }
    }

    /// §7.6: merge windows (a window whose reset is None keeps its previous value).
    pub fn on_rate_limit(&mut self, e: &RateLimits, now: Time) {
        // unifiedWindows carries each window only once known: keep the other one, and show nothing before a first window.
        let k = match &self.limits {
            Some(k) => k.clone(),
            None if e.five_hour_reset.is_none() && e.seven_day_reset.is_none() => return,
            None => e.clone(),
        };
        let mut l = e.clone();
        if e.five_hour_reset.is_none() {
            (l.five_hour, l.five_hour_reset) = (k.five_hour, k.five_hour_reset);
        }
        if e.seven_day_reset.is_none() {
            (l.seven_day, l.seven_day_reset) = (k.seven_day, k.seven_day_reset);
        }
        self.limits = Some(l);
        self.limits_at = Some(now);
    }

    /// First message model of an Agent-kind parent tool.
    pub fn subagent_model(&mut self, parent: Option<&str>, model: Option<&str>) {
        let (Some(parent), Some(model)) = (parent, model) else { return };
        if let Some(i) = self.tool_index(parent) {
            if self.items()[i].as_tool().is_some_and(|a| a.model.is_none() && tool_kinds::is_agent(&a.name)) {
                self.update_tool(i, |a| a.model = Some(model.to_string()));
            }
        }
    }

    /// §7.7: after reload_plugins + reload_skills.
    pub fn reloaded(&mut self, plugins: &Value, skills: &Value) {
        let r = plugins;
        if let Some(i) = &mut self.heavy.init {
            i.plugins = json::names(r, "plugins");
            i.agents = json::names(r, "agents");
            i.skills = json::names(skills, "skills");
        }
        if let (Some(info), Some(a)) = (&self.heavy.initialize_info, r.get("agents").filter(|a| a.is_array())) {
            self.heavy.initialize_info = Some(with_key(info, "agents", a.clone()));
        }
        if let Some(m) = r.get("mcpServers").filter(|m| m.is_array()) {
            let mut o = Map::new();
            o.insert("mcpServers".into(), m.clone());
            self.heavy.mcp_status = Some(Value::Object(o));
        }
    }
}
