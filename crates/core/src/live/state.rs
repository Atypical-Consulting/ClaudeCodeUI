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

impl SessionState {
    /// `new LiveSession(id, name, cwd, mode, worktree, model, effort, add_dirs)` state: status Exited, thinking on,
    /// fast_mode_state "off", started_at = last_event_at = now.
    pub fn new(id: &str, name: &str, cwd: &str, mode: &str, now: Time) -> SessionState {
        todo!("port: LiveSession constructor (state part)")
    }

    pub fn items(&self) -> &[Item] {
        &self.items
    }

    /// Replace the whole list (Open / Fork history): every index dirty.
    pub fn set_items(&mut self, items: Vec<Item>) {
        todo!("port: (Items = …)")
    }

    pub fn push_item(&mut self, item: Item) {
        todo!("port: (Items = Items.Add)")
    }

    pub fn set_item(&mut self, index: usize, item: Item) {
        todo!("port: (Set(old, new))")
    }

    pub fn insert_item(&mut self, index: usize, item: Item) {
        todo!("port: (Items.Insert)")
    }

    pub fn remove_range(&mut self, range: std::ops::Range<usize>) {
        todo!("port: (Items.RemoveRange)")
    }

    /// Dirty items `(index, item)` ascending, cleared.
    pub fn take_dirty(&mut self) -> Vec<(usize, Item)> {
        todo!("port: (patch tracking)")
    }

    /// Index of the last ToolItem with that id (`Tool(id)`).
    pub fn tool_index(&self, id: &str) -> Option<usize> {
        todo!("port: LiveSession.Tool")
    }

    /// Push UserItem(text, now, ultracode, images, uuid); resume_turn = false; Running; turn_started_at = now;
    /// clear stream; thinking_tokens = 0.
    pub fn begin_turn(&mut self, text: &str, images: Option<Vec<UserImage>>, uuid: &str, now: Time) {
        todo!("port: LiveSession.BeginTurn")
    }

    /// Only if resume_turn && status ∈ {Idle, Starting}: push ResumedItem, Running, labelled once.
    pub fn resumed_turn(&mut self, now: Time) {
        todo!("port: LiveSession.ResumedTurn")
    }

    /// Running/Waiting tools (except background ones when keep_background) → Error at now; without keep_background
    /// running BgTasks → "killed".
    pub fn end_tools(&mut self, now: Time, keep_background: bool) {
        todo!("port: LiveSession.EndTools")
    }

    /// Empty the stream and bump `stream_epoch`.
    pub fn clear_stream(&mut self) {
        todo!("port: LiveSession.ClearStream")
    }

    /// Remove the pending request; its Waiting tool → Denied (deny) or Running; Waiting with nothing left → Running.
    pub fn resolve(&mut self, request_id: &str, decision: Decision) {
        todo!("port: LiveSession.Resolve")
    }

    /// Remove `[i, end)` from the UserItem with `uuid` (end = first later UserItem not in `rewound`, else the end).
    pub fn truncate(&mut self, uuid: &str, rewound: Option<&[String]>) {
        todo!("port: LiveSession.Truncate")
    }

    /// Restore the typed text (or `user_text(prefill)`) ahead of the draft, images if the draft has none, truncate;
    /// `draft.rev += 1` so the patch carries the restored draft and stale UI writes are ignored.
    pub fn cut_back(&mut self, uuid: &str, prefill: Option<&str>, rewound: Option<&[String]>) {
        todo!("port: LiveSession.CutBack")
    }

    /// Esc before Claude answered: the last UserItem (with uuid) when the turn was interrupted and nothing followed.
    pub fn unanswered(&self) -> Option<UserItem> {
        todo!("port: LiveSession.Unanswered")
    }

    /// `Session.NotStarted` if start_failed else `Session.ExitShort(exit_code)`.
    pub fn exit_label(&self) -> String {
        todo!("port: LiveSession.ExitLabel")
    }

    /// items non-empty && status ∉ {Running, Waiting}.
    pub fn can_fork(&self) -> bool {
        todo!("port: LiveSession.CanFork")
    }

    /// (worktree set || root_of(cwd) != cwd) && add_dirs empty.
    pub fn isolated(&self) -> bool {
        todo!("port: LiveSession.Isolated")
    }

    /// `remote_control_available` is not JSON false in initialize_info (absent = available).
    pub fn remote_available(&self) -> bool {
        todo!("port: LiveSession.RemoteAvailable")
    }

    /// The read model (without items).
    pub fn meta(&self, has_process: bool) -> SessionMeta {
        todo!("port: (SessionMeta from state)")
    }
}
