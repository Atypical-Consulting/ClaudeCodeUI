//! CONTRACT — the session read model pushed to the UI (state sync protocol, docs/ARCHITECTURE.md §State sync).
//! Ports the public surface of `Core/LiveSession.cs` (spec 01 §2.4, spec 07 §0.3) and the SessionManager queries the
//! shell reads (`Active`, `DecisionQueue`, `Limits`, `UsageSource`, `RunningElsewhere`; spec 03 §2).
//!
//! Owner: tauri-glue (contract). Built by core `live::LiveSession::{meta, snapshot, take_patch}` and
//! `session_manager::SessionManager::summary`.

use crate::model::*;
use crate::usage::Usage;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Everything about a live session except its items, streaming text, the heavy rarely-changing JSON
/// ([`SessionHeavy`]) and the composer draft ([`SessionDraft`]). Small: sent whole on every patch.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionMeta {
    /// == `--session-id` == transcript file stem == route `/session/{id}`.
    pub id: String,
    pub name: String,
    pub cwd: String,
    /// "" = no mode chosen yet (CLI applies permissions.defaultMode), else one of `session_rules::MODES` (or a raw CLI mode).
    pub mode: String,
    pub branch: Option<String>,
    pub repo: Option<String>,
    /// `-w` name; dropped on ExitWorktree / set_cwd.
    pub worktree: Option<String>,
    pub add_dirs: Vec<String>,
    pub status: SessionStatus,
    pub pending: Vec<PendingPermission>,
    pub queued: Vec<QueuedMessage>,
    pub thinking_tokens: i32,
    pub cost_usd: Decimal,
    pub last_turn_cost_usd: Decimal,
    /// Cost when opened (Overview counts cost_usd − cost_at_open).
    pub cost_at_open: Decimal,
    pub tool_count: i32,
    pub context_tokens: i64,
    pub context_window: Option<i64>,
    pub cache_read: i64,
    pub cache_write: i64,
    pub started_at: Time,
    pub last_event_at: Time,
    pub turn_started_at: Option<Time>,
    pub last_turn_ms: Option<i64>,
    /// "interrupted" sentinel after an interrupt (never displayed).
    pub last_result_subtype: Option<String>,
    pub last_result_at: Option<Time>,
    pub exit_code: Option<i32>,
    pub exit_text: Option<String>,
    /// The process never spawned.
    pub start_failed: bool,
    /// `Session.NotStarted` if start_failed else `Session.ExitShort(exit_code)`, localized by the backend.
    pub exit_label: String,
    pub crashed_mid_turn: bool,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub advisor: Option<String>,
    pub output_style: Option<String>,
    /// Ultracode armed "for this turn".
    pub ultracode: bool,
    pub thinking: bool,
    pub fast_mode_state: String,
    pub fast_mode_reason: Option<String>,
    pub pr_url: Option<String>,
    pub remote_url: Option<String>,
    pub remote_available: bool,
    /// statusLine command output (≤ 3 lines).
    pub status_text: Option<String>,
    /// plugin → status line, sorted by plugin name.
    pub plugin_status: Vec<(String, String)>,
    pub running_hooks: Vec<RunningHook>,
    pub tasks: Vec<BgTask>,
    pub limits: Option<RateLimits>,
    pub limits_at: Option<Time>,
    pub retry: Option<RetryEvt>,
    pub compacting_since: Option<Time>,
    pub artifacts: Vec<Artifact>,
    pub pushes: i32,
    pub last_push: Option<String>,
    pub has_process: bool,
    /// Predicted next prompt (ghost text).
    pub suggestion: Option<String>,
    pub untitled: bool,
    pub can_fork: bool,
    pub isolated: bool,
    pub restricted: bool,
    pub safe_mode: bool,
    pub agent: Option<String>,
    pub fork_of: Option<String>,
    pub fork_at: Option<String>,
    pub fallback_model: Option<String>,
    /// UserItem uuids the current process knows (rewind targets). `session_rules::can_rewind` reads it.
    pub sent: Vec<String>,
}

/// The large, rarely-changing part of the read model. Held by the backend as ONE sub-struct of the session state
/// (`SessionState.heavy`) so a patch carries it only when it differs from the copy last pushed (compared by value
/// under the gate — no revision counter to forget).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionHeavy {
    pub init: Option<InitInfo>,
    /// initialize response: models, commands, agents, account, available_output_styles, claude_code_version…
    pub initialize_info: Option<Value>,
    pub status_rows: Option<Vec<StatusSection>>,
    pub memory_dialog: Option<Value>,
    /// Raw get_context_usage.
    pub context: Option<Value>,
    /// Raw mcp_status (`{"mcpServers":[…]}`).
    pub mcp_status: Option<Value>,
    pub usage: Option<Usage>,
}

/// The composer's unsent text + images, owned by the backend because rewind (`cut_back`) writes the restored prompt
/// into it. `rev` is bumped ONLY by backend-originated writes (cut_back); the UI's own writes
/// (`session_set_draft_text` / `session_set_draft_images`) carry the rev they are based on and are ignored by the
/// backend when it moved on, and they never bump it (so they are never echoed back).
/// A patch carries the draft only when `rev` changed since the last push; the Composer adopts a pushed draft only
/// when its rev differs from the one it holds (and drops any pending debounced write then).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionDraft {
    pub rev: u64,
    pub text: String,
    pub images: Vec<UserImage>,
}

/// Full state for first render (`session_open` / `session_watch` / `session_snapshot`). `version` and every field
/// are read under the same lock.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionSnapshot {
    pub version: u64,
    pub meta: SessionMeta,
    pub heavy: SessionHeavy,
    pub draft: SessionDraft,
    pub items: Vec<Item>,
    pub streaming_text: String,
}

/// Structural change, throttled 50 ms (immediate on permission request / result). Only WATCHED sessions get
/// patches (`session_watch`); built under the session lock together with `version`.
///
/// The upserts are every item changed since the previous patch (`base_version`), so a patch covers the range
/// `(base_version, version]`. UI rule (held = version the UI holds):
/// - `version <= held` → stale, ignore;
/// - `base_version <= held < version` → apply: truncate items to `len`, write each upsert at its index (an index
///   greater than the current length is a gap → re-fetch), replace `meta`, replace heavy/draft when present;
/// - `base_version > held` → a patch was missed → re-fetch `session_snapshot` (buffer patches meanwhile, then apply
///   the buffered ones by the same rule).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionPatch {
    pub id: String,
    /// Version of the previous patch pushed for this session (0 for the first).
    pub base_version: u64,
    pub version: u64,
    pub meta: SessionMeta,
    /// Only when it differs from the heavy part last pushed.
    pub heavy: Option<SessionHeavy>,
    /// Only when `draft.rev` changed since the last push.
    pub draft: Option<SessionDraft>,
    /// Items changed since the last push, `(index, item)` in ascending index order.
    pub items: Vec<(usize, Item)>,
    /// Item count after this patch (shorter = rewind/reset truncated).
    pub len: usize,
}

/// Streaming text increment (does not bump version; watched sessions only). UI rule: `offset == 0` → replace the
/// text with `append` (a cleared/restarted stream; `append` "" clears); `offset <= held length` → truncate the held
/// text to `offset` (byte offset, a char boundary: both sides hold the same prefix), then push `append`;
/// `offset > held length` → gap → re-fetch the snapshot (its `streaming_text`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionStream {
    pub id: String,
    pub offset: usize,
    pub append: String,
}

/// A tool name + input (Overview "last step", palette details).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolBrief {
    pub name: String,
    pub input: Value,
}

/// One live session as the rail, palette, mascot, quota meter and Overview see it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub version: u64,
    pub name: String,
    pub cwd: String,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub worktree: Option<String>,
    pub mode: String,
    pub status: SessionStatus,
    pub untitled: bool,
    pub items_len: usize,
    pub pending_count: usize,
    pub first_pending_tool: Option<String>,
    pub exit_label: String,
    pub exit_code: Option<i32>,
    pub start_failed: bool,
    pub cost_usd: Decimal,
    pub cost_at_open: Decimal,
    pub tool_count: i32,
    pub started_at: Time,
    pub last_event_at: Time,
    pub last_result_subtype: Option<String>,
    pub last_result_at: Option<Time>,
    pub last_turn_ms: Option<i64>,
    pub has_process: bool,
    pub can_fork: bool,
    /// init.mcp names with status "failed" (Rail badge reads the last session that has an init).
    pub has_init: bool,
    pub failed_mcp: i32,
    /// First pending tool, else the last top-level ToolItem (Overview "Last").
    pub last_tool: Option<ToolBrief>,
    /// `(completed, total)` of `TodoList::from(items)`, None when empty.
    pub todos: Option<(usize, usize)>,
}

/// `(session, pending permission)` across sessions, arrival order (Ctrl+Shift+A, Overview queue, PermissionCard).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DecisionEntry {
    pub session_id: String,
    pub session_name: String,
    pub session_cwd: String,
    pub permission: PendingPermission,
    /// `session_rules::agent_label(asking_agent(items, permission))`, computed by the backend (the UI holds only the
    /// watched session's items): PermissionCard's "also waiting" shows `name · label`.
    pub agent_label: Option<String>,
}

/// The session the quota meter can ask again (`UsageSource`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageSource {
    pub session_id: String,
    pub usage: Usage,
    pub has_process: bool,
}

/// App-wide session state (`Push::Sessions`, `sessions_summary`). Sent when any session's version moves.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionsSummary {
    /// Every live session in creation order (never re-sorted). `Active` = status != Exited.
    pub sessions: Vec<SessionSummary>,
    pub decision_queue: Vec<DecisionEntry>,
    /// Limits of the session with the latest limits_at.
    pub limits: Option<RateLimits>,
    pub limits_at: Option<Time>,
    pub usage_source: Option<UsageSource>,
    /// `TranscriptStore.HeldByClaude()`: session ids named on any running claude command line, this app's own included.
    /// `RunningElsewhere(id)` = (no live session with a process for id) && id ∈ held — see `session_rules::running_elsewhere`.
    pub held: Vec<String>,
}
