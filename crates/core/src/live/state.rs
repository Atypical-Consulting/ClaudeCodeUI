//! `LiveSession` state (spec 01 §2.4) and the state-level helpers of `Core/LiveSession.cs` (begin_turn,
//! resumed_turn, end_tools, resolve, truncate, cut_back, unanswered, clear_stream). Owner: core-live-session.
//!
//! Also the reducer's scratch target: transcript replay and workflow agent logs build a bare `SessionState`.
//!
//! PATCH TRACKING: mutate `items` ONLY through `push_item` / `set_item` / `insert_item` / `remove_range`, which
//! record dirty indexes; `take_dirty` turns them into `SessionPatch.items`. A removal at index i marks every index
//! ≥ i that remains dirty (the UI truncates to `len`, then writes the upserts).

use ccui_shared::model::*;
use ccui_shared::session::{SessionDraft, SessionHeavy, SessionMeta};
use ccui_shared::session_rules::{self, INTERRUPTED};
use ccui_shared::t;
use ccui_shared::usage::Usage;
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};

pub struct SessionState {
    // identity & init-only
    pub id: String,
    pub name: String,
    pub cwd: String,
    pub mode: String,
    pub branch: Option<String>,
    pub repo: Option<String>,
    pub worktree: Option<String>,
    pub add_dirs: Vec<String>,
    pub fallback_model: Option<String>,
    pub restricted: bool,
    pub safe_mode: bool,
    pub agent: Option<String>,
    pub fork_of: Option<String>,
    pub fork_at: Option<String>,
    pub cost_at_open: Decimal,

    // conversation
    items: Vec<Item>,
    pub pending: Vec<PendingPermission>,
    pub queued: Vec<QueuedMessage>,
    /// Streamed text buffer (top-level text_delta only).
    pub stream: String,
    /// Bumped by `clear_stream` (the next stream push restarts at offset 0).
    pub stream_epoch: u64,
    pub status: SessionStatus,
    pub thinking_tokens: i32,
    pub cost_usd: Decimal,
    pub last_turn_cost_usd: Decimal,
    pub tool_count: i32,
    pub context_tokens: i64,
    pub context_window: Option<i64>,
    pub cache_read: i64,
    pub cache_write: i64,
    pub started_at: Time,
    pub last_event_at: Time,
    pub turn_started_at: Option<Time>,
    pub last_turn_ms: Option<i64>,
    pub last_result_subtype: Option<String>,
    pub last_result_at: Option<Time>,
    pub exit_code: Option<i32>,
    pub exit_text: Option<String>,
    pub start_failed: bool,
    /// init, initialize_info, status_rows, memory_dialog, context, mcp_status, usage (pushed only when changed).
    pub heavy: SessionHeavy,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub ultracode: bool,
    pub pr_url: Option<String>,
    pub remote_url: Option<String>,
    pub thinking: bool,
    pub advisor: Option<String>,
    pub output_style: Option<String>,
    pub status_text: Option<String>,
    /// hook_id → (name, started_at), insertion order.
    pub running_hooks: Vec<RunningHook>,
    pub fast_mode_state: String,
    pub fast_mode_reason: Option<String>,
    pub plugin_status: BTreeMap<String, String>,
    pub tasks: Vec<BgTask>,
    pub limits: Option<RateLimits>,
    pub limits_at: Option<Time>,
    pub retry: Option<RetryEvt>,
    pub compacting_since: Option<Time>,
    pub artifacts: Vec<Artifact>,
    pub pushes: i32,
    pub last_push: Option<String>,
    /// Composer draft; `draft.rev` += 1 only in `cut_back` (backend-originated writes).
    pub draft: SessionDraft,
    pub suggestion: Option<String>,
    pub untitled: bool,

    // reducer-private (spec 01 §2.4 "Private state")
    pub turn_ultra: bool,
    pub crashed_mid_turn: bool,
    pub resume_turn: bool,
    pub draining: bool,
    pub ultra_carry: bool,
    /// UserItem uuids the CURRENT process knows (rewind targets).
    pub sent: HashSet<String>,
    /// `cost_usd - persisted cost-state` at spawn (total_cost_usd counts on from it).
    pub cost_base: Decimal,

    dirty: BTreeSet<usize>,
}

/// `BgTask.Running` (status ∈ running, pending, paused). Local twin of the contract's `BgTask::running`.
pub(crate) fn task_running(t: &BgTask) -> bool {
    matches!(t.status.as_str(), "running" | "pending" | "paused")
}

impl SessionState {
    /// `new LiveSession(id, name, cwd, mode, worktree, model, effort, add_dirs)` state: status Exited, thinking on,
    /// fast_mode_state "off", started_at = last_event_at = now.
    pub fn new(id: &str, name: &str, cwd: &str, mode: &str, now: Time) -> SessionState {
        SessionState {
            id: id.into(),
            name: name.into(),
            cwd: cwd.into(),
            mode: mode.into(),
            branch: None,
            repo: None,
            worktree: None,
            add_dirs: Vec::new(),
            fallback_model: None,
            restricted: false,
            safe_mode: false,
            agent: None,
            fork_of: None,
            fork_at: None,
            cost_at_open: Decimal::ZERO,
            items: Vec::new(),
            pending: Vec::new(),
            queued: Vec::new(),
            stream: String::new(),
            stream_epoch: 0,
            status: SessionStatus::Exited,
            thinking_tokens: 0,
            cost_usd: Decimal::ZERO,
            last_turn_cost_usd: Decimal::ZERO,
            tool_count: 0,
            context_tokens: 0,
            context_window: None,
            cache_read: 0,
            cache_write: 0,
            started_at: now,
            last_event_at: now,
            turn_started_at: None,
            last_turn_ms: None,
            last_result_subtype: None,
            last_result_at: None,
            exit_code: None,
            exit_text: None,
            start_failed: false,
            heavy: SessionHeavy::default(),
            model: None,
            effort: None,
            ultracode: false,
            pr_url: None,
            remote_url: None,
            thinking: true,
            advisor: None,
            output_style: None,
            status_text: None,
            running_hooks: Vec::new(),
            fast_mode_state: "off".into(),
            fast_mode_reason: None,
            plugin_status: BTreeMap::new(),
            tasks: Vec::new(),
            limits: None,
            limits_at: None,
            retry: None,
            compacting_since: None,
            artifacts: Vec::new(),
            pushes: 0,
            last_push: None,
            draft: SessionDraft::default(),
            suggestion: None,
            untitled: false,
            turn_ultra: false,
            crashed_mid_turn: false,
            resume_turn: false,
            draining: false,
            ultra_carry: false,
            sent: HashSet::new(),
            cost_base: Decimal::ZERO,
            dirty: BTreeSet::new(),
        }
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Replace the whole list (Open / Fork history): every index dirty.
    pub fn set_items(&mut self, items: Vec<Item>) {
        self.items = items;
        self.dirty = (0..self.items.len()).collect();
    }

    pub fn push_item(&mut self, item: Item) {
        self.items.push(item);
        self.dirty.insert(self.items.len() - 1);
    }

    pub fn set_item(&mut self, index: usize, item: Item) {
        if index < self.items.len() {
            self.items[index] = item;
            self.dirty.insert(index);
        }
    }

    pub fn insert_item(&mut self, index: usize, item: Item) {
        let index = index.min(self.items.len());
        self.items.insert(index, item);
        self.dirty.extend(index..self.items.len());
    }

    pub fn remove_range(&mut self, range: std::ops::Range<usize>) {
        let end = range.end.min(self.items.len());
        if range.start >= end {
            return;
        }
        self.items.drain(range.start..end);
        self.dirty.retain(|i| *i < range.start);
        self.dirty.extend(range.start..self.items.len());
    }

    /// Dirty items `(index, item)` ascending, cleared.
    pub fn take_dirty(&mut self) -> Vec<(usize, Item)> {
        let dirty = std::mem::take(&mut self.dirty);
        dirty.into_iter().filter_map(|i| self.items.get(i).map(|it| (i, it.clone()))).collect()
    }

    /// Drop every item `f` matches (`Items.RemoveAll`): indexes from the first removed one are dirty.
    pub(crate) fn remove_items(&mut self, f: impl Fn(&Item) -> bool) {
        let Some(first) = self.items.iter().position(&f) else { return };
        self.items.retain(|i| !f(i));
        self.dirty.retain(|i| *i < first);
        self.dirty.extend(first..self.items.len());
    }

    /// Replace the ToolItem at `index` with `f` applied (no-op when it is not a ToolItem).
    pub(crate) fn update_tool(&mut self, index: usize, f: impl FnOnce(&mut ToolItem)) {
        if let Some(Item::Tool(t)) = self.items.get(index) {
            let mut t = t.clone();
            f(&mut t);
            self.set_item(index, Item::Tool(t));
        }
    }

    /// Index of the last ToolItem with that id (`Tool(id)`).
    pub fn tool_index(&self, id: &str) -> Option<usize> {
        self.items.iter().rposition(|i| matches!(i, Item::Tool(t) if t.id == id))
    }

    /// Index of the last ToolItem whose task_id is `task_id`.
    pub(crate) fn task_tool_index(&self, task_id: &str) -> Option<usize> {
        self.items.iter().rposition(|i| matches!(i, Item::Tool(t) if t.task_id.as_deref() == Some(task_id)))
    }

    pub(crate) fn tool(&self, id: &str) -> Option<&ToolItem> {
        self.tool_index(id).and_then(|i| self.items[i].as_tool())
    }

    /// Push UserItem(text, now, ultracode, images, uuid); resume_turn = false; Running; turn_started_at = now;
    /// clear stream; thinking_tokens = 0.
    pub fn begin_turn(&mut self, text: &str, images: Option<Vec<UserImage>>, uuid: &str, now: Time) {
        let uuid = (!uuid.is_empty()).then(|| uuid.to_string());
        self.push_item(Item::User(UserItem { text: text.into(), at: now, ultracode: self.ultracode, images, uuid }));
        self.resume_turn = false;
        self.status = SessionStatus::Running;
        self.turn_started_at = Some(now);
        self.clear_stream();
        self.thinking_tokens = 0;
    }

    /// Only if resume_turn && status ∈ {Idle, Starting}: push ResumedItem, Running, labelled once.
    pub fn resumed_turn(&mut self, now: Time) {
        if !self.resume_turn || !matches!(self.status, SessionStatus::Idle | SessionStatus::Starting) {
            return;
        }
        self.resume_turn = false;
        self.push_item(Item::Resumed);
        self.status = SessionStatus::Running;
        self.turn_started_at = Some(now);
        self.clear_stream();
        self.thinking_tokens = 0;
    }

    /// Running/Waiting tools (except background ones when keep_background) → Error at now; without keep_background
    /// running BgTasks → "killed".
    pub fn end_tools(&mut self, now: Time, keep_background: bool) {
        let open: Vec<usize> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                matches!(i, Item::Tool(t) if matches!(t.state, ToolState::Running | ToolState::Waiting) && !(keep_background && t.background))
            })
            .map(|(i, _)| i)
            .collect();
        for i in open {
            self.update_tool(i, |t| {
                t.state = ToolState::Error;
                t.ended_at = Some(now);
            });
        }
        if !keep_background {
            for t in self.tasks.iter_mut().filter(|t| task_running(t)) {
                t.status = "killed".into();
                t.ended_at = Some(now);
            }
        }
    }

    /// Empty the stream and bump `stream_epoch`.
    pub fn clear_stream(&mut self) {
        self.stream.clear();
        self.stream_epoch += 1;
    }

    /// Remove the pending request; its Waiting tool → Denied (deny) or Running; Waiting with nothing left → Running.
    pub fn resolve(&mut self, request_id: &str, decision: Decision) {
        let Some(at) = self.pending.iter().position(|p| p.request_id == request_id) else { return };
        let p = self.pending.remove(at);
        if let Some(i) = p.tool_use_id.as_deref().and_then(|id| self.tool_index(id)) {
            if self.items[i].as_tool().is_some_and(|t| t.state == ToolState::Waiting) {
                self.update_tool(i, |t| t.state = if decision == Decision::Deny { ToolState::Denied } else { ToolState::Running });
            }
        }
        if self.pending.is_empty() && self.status == SessionStatus::Waiting {
            self.status = SessionStatus::Running;
        }
    }

    /// Remove `[i, end)` from the UserItem with `uuid` (end = first later UserItem not in `rewound`, else the end).
    pub fn truncate(&mut self, uuid: &str, rewound: Option<&[String]>) {
        let is_user = |x: &Item, f: &dyn Fn(&str) -> bool| matches!(x, Item::User(u) if f(u.uuid.as_deref().unwrap_or("")));
        let Some(i) = self.items.iter().position(|x| matches!(x, Item::User(u) if u.uuid.as_deref() == Some(uuid))) else { return };
        let end = rewound
            .and_then(|rw| {
                let kept = |id: &str| !rw.iter().any(|r| r == id);
                self.items[i + 1..].iter().position(|x| is_user(x, &kept)).map(|p| p + i + 1)
            })
            .unwrap_or(self.items.len());
        self.remove_range(i..end);
    }

    /// Restore the typed text (or `user_text(prefill)`) ahead of the draft, images if the draft has none, truncate;
    /// `draft.rev += 1` so the patch carries the restored draft and stale UI writes are ignored.
    pub fn cut_back(&mut self, uuid: &str, prefill: Option<&str>, rewound: Option<&[String]>) {
        self.suggestion = None; // predicted after the turn that leaves
        let item = self.items.iter().find_map(|x| x.as_user().filter(|u| u.uuid.as_deref() == Some(uuid))).cloned();
        let back = match &item {
            Some(u) => Some(u.text.clone()),
            None => prefill.and_then(session_rules::user_text),
        };
        if let Some(back) = back {
            self.draft.text = if self.draft.text.trim().is_empty() { back } else { format!("{back}\n\n{}", self.draft.text) };
        }
        if let Some(images) = item.as_ref().and_then(|u| u.images.as_ref()).filter(|im| !im.is_empty()) {
            if self.draft.images.is_empty() {
                self.draft.images = images.iter().filter(|i| i.url.is_some()).cloned().collect();
            }
        }
        self.truncate(uuid, rewound);
        self.draft.rev += 1;
    }

    /// Esc before Claude answered: the last UserItem (with uuid) when the turn was interrupted and nothing followed.
    pub fn unanswered(&self) -> Option<UserItem> {
        if self.last_result_subtype.as_deref() != Some(INTERRUPTED) || !self.queued.is_empty() {
            return None;
        }
        let at = self.items.iter().rposition(|i| matches!(i, Item::User(_)))?;
        let answered = self.items[at + 1..].iter().any(|i| {
            matches!(i, Item::Text(_) | Item::Tool(_) | Item::Thought(_) | Item::ApiError(_) | Item::Compact(_) | Item::Fallback(_))
        });
        match &self.items[at] {
            Item::User(u) if !answered && u.uuid.is_some() => Some(u.clone()),
            _ => None,
        }
    }

    /// `Session.NotStarted` if start_failed else `Session.ExitShort(exit_code)`.
    pub fn exit_label(&self) -> String {
        if self.start_failed {
            t!("Session.NotStarted")
        } else {
            t!("Session.ExitShort", self.exit_code.map(|c| c.to_string()).unwrap_or_default())
        }
    }

    /// items non-empty && status ∉ {Running, Waiting}.
    pub fn can_fork(&self) -> bool {
        !self.items.is_empty() && !matches!(self.status, SessionStatus::Running | SessionStatus::Waiting)
    }

    /// (worktree set || root_of(cwd) != cwd) && add_dirs empty.
    pub fn isolated(&self) -> bool {
        (self.worktree.is_some() || ccui_shared::text::root_of(&self.cwd) != self.cwd) && self.add_dirs.is_empty()
    }

    /// `remote_control_available` is not JSON false in initialize_info (absent = available).
    pub fn remote_available(&self) -> bool {
        self.heavy.initialize_info.as_ref().is_some_and(|i| i.get("remote_control_available") != Some(&Value::Bool(false)))
    }

    /// The read model (without items).
    pub fn meta(&self, has_process: bool) -> SessionMeta {
        let mut sent: Vec<String> = self.sent.iter().cloned().collect();
        sent.sort();
        SessionMeta {
            id: self.id.clone(),
            name: self.name.clone(),
            cwd: self.cwd.clone(),
            mode: self.mode.clone(),
            branch: self.branch.clone(),
            repo: self.repo.clone(),
            worktree: self.worktree.clone(),
            add_dirs: self.add_dirs.clone(),
            status: self.status,
            pending: self.pending.clone(),
            queued: self.queued.clone(),
            thinking_tokens: self.thinking_tokens,
            cost_usd: self.cost_usd,
            last_turn_cost_usd: self.last_turn_cost_usd,
            cost_at_open: self.cost_at_open,
            tool_count: self.tool_count,
            context_tokens: self.context_tokens,
            context_window: self.context_window,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
            started_at: self.started_at,
            last_event_at: self.last_event_at,
            turn_started_at: self.turn_started_at,
            last_turn_ms: self.last_turn_ms,
            last_result_subtype: self.last_result_subtype.clone(),
            last_result_at: self.last_result_at,
            exit_code: self.exit_code,
            exit_text: self.exit_text.clone(),
            start_failed: self.start_failed,
            exit_label: self.exit_label(),
            crashed_mid_turn: self.crashed_mid_turn,
            model: self.model.clone(),
            effort: self.effort.clone(),
            advisor: self.advisor.clone(),
            output_style: self.output_style.clone(),
            ultracode: self.ultracode,
            thinking: self.thinking,
            fast_mode_state: self.fast_mode_state.clone(),
            fast_mode_reason: self.fast_mode_reason.clone(),
            pr_url: self.pr_url.clone(),
            remote_url: self.remote_url.clone(),
            remote_available: self.remote_available(),
            status_text: self.status_text.clone(),
            plugin_status: self.plugin_status.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            running_hooks: self.running_hooks.clone(),
            tasks: self.tasks.clone(),
            limits: self.limits.clone(),
            limits_at: self.limits_at,
            retry: self.retry.clone(),
            compacting_since: self.compacting_since,
            artifacts: self.artifacts.clone(),
            pushes: self.pushes,
            last_push: self.last_push.clone(),
            has_process,
            suggestion: self.suggestion.clone(),
            untitled: self.untitled,
            can_fork: self.can_fork(),
            isolated: self.isolated(),
            restricted: self.restricted,
            safe_mode: self.safe_mode,
            agent: self.agent.clone(),
            fork_of: self.fork_of.clone(),
            fork_at: self.fork_at.clone(),
            fallback_model: self.fallback_model.clone(),
            sent,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn st(cwd: &str) -> SessionState {
        SessionState::new("i", "i", cwd, "auto", Utc::now())
    }

    fn user_uuid(s: &SessionState, i: usize) -> Option<&str> {
        s.items()[i].as_user().and_then(|u| u.uuid.as_deref())
    }

    #[test]
    fn dirty_tracking() {
        let mut s = st("/w");
        let now = Utc::now();
        for u in ["a", "b", "c", "d"] {
            s.begin_turn(u, None, u, now);
        }
        assert_eq!(s.take_dirty().iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
        assert!(s.take_dirty().is_empty(), "cleared");
        s.set_item(1, Item::Reset);
        s.remove_range(2..3);
        // index 1 changed, removal at 2 → every remaining index ≥ 2 dirty.
        assert_eq!(s.take_dirty().iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![1, 2]);
        s.insert_item(0, Item::Resumed);
        assert_eq!(s.take_dirty().len(), 4, "an insert shifts everything after it");
        s.set_item(3, Item::Reset);
        s.remove_range(1..4);
        assert!(s.take_dirty().is_empty(), "a removed dirty index past the end is dropped");
        assert_eq!(s.items().len(), 1);
    }

    #[test]
    fn isolation() {
        assert!(st("/r/.claude/worktrees/x").isolated());
        let mut w = st("/r");
        w.worktree = Some("x".into());
        assert!(w.isolated());
        assert!(!st("/r").isolated());
        let mut o = st("/r/.claude/worktrees/x");
        o.add_dirs = vec!["/o".into()];
        assert!(!o.isolated(), "extra folders are never isolated");
        w.add_dirs = vec!["/o".into()];
        assert!(!w.isolated());
    }

    #[test]
    fn truncate_and_cut_back() {
        let now = Utc::now();
        // A slash command comes back as typed, not as the CLI's prefillText XML, ahead of what the box already holds.
        const COMPACT_XML: &str =
            "<command-message>compact</command-message>\n<command-name>/compact</command-name>\n<command-args>keep tests</command-args>";
        let mut cb = st("/w");
        cb.draft.text = "unsent".into();
        cb.begin_turn("/compact keep tests", None, "c1", now);
        cb.cut_back("c1", Some(COMPACT_XML), None);
        assert_eq!(cb.draft.text, "/compact keep tests\n\nunsent");
        assert!(cb.items().is_empty());
        assert_eq!(cb.draft.rev, 1, "a backend write moves the rev");
        let mut cx = st("/w");
        cx.cut_back("gone", Some(COMPACT_XML), None);
        assert_eq!(cx.draft.text, "/compact keep tests", "prefillText normalised when the message is not in the thread");
        // Sent while the automatic restore awaited its reply: the CLI holds it, so it stays.
        let mut rs = st("/w");
        rs.begin_turn("stop", None, "s1", now);
        rs.begin_turn("new", None, "s2", now);
        rs.push_item(Item::Text(TextItem { markdown: "ok".into(), parent_tool_use_id: None, uuid: None, at: None }));
        rs.cut_back("s1", None, Some(&["s1".to_string()]));
        assert_eq!(rs.draft.text, "stop");
        assert_eq!(rs.items().len(), 2);
        assert_eq!(user_uuid(&rs, 0), Some("s2"));
    }
}
