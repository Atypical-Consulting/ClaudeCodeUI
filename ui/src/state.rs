//! Front-end stores (Leptos context). Ports `Core/UiState.cs` (per window — spec 03 §7, 06 §3, 07 §0.2) and replaces
//! the Blazor `Changed` subscriptions by one push dispatcher (spec 06 §11). Owner: ui-shell.
//!
//! - `UiState`: per-window UI signals + the relayed key / rewind events (sequence numbers, so identical consecutive
//!   events re-fire).
//! - `AppStore`: app-wide data pushed by the backend (session summary, counts, prefs) and the per-session stores.
//! - `SessionHandle`: one session's read model (meta, items, version, streaming text) + front-end-only per-session
//!   state that must outlive components (memory panel edit). Created under the root owner, keyed by session id,
//!   so it survives SessionPage remounts. Patches apply in place (see `ccui_shared::session::SessionPatch`).

use ccui_shared::api::{BootInfo, Flags, Prefs, WorktreeRunStep};
use ccui_shared::memory::MemoryEdit;
use ccui_shared::model::{Item, UserItem};
use ccui_shared::push::Push;
use ccui_shared::session::{SessionMeta, SessionPatch, SessionSnapshot, SessionsSummary};
use leptos::prelude::*;
use std::collections::HashMap;

/// `UiState` (default inspector open).
#[derive(Clone, Copy)]
pub struct UiState {
    pub inspector_open: RwSignal<bool>,
    pub palette_open: RwSignal<bool>,
    /// Inspector shows uncommitted changes (`/diff`).
    pub diff_open: RwSignal<bool>,
    /// A ToolItem id or an Agent row id.
    pub selected_tool_id: RwSignal<Option<String>>,
    /// Phase title of the selected Workflow call (None = panel default).
    pub selected_phase: RwSignal<Option<String>>,
    /// Relayed keys: "Escape", "Enter", "Shift+Enter", "Delete" with a sequence number.
    pub key: RwSignal<Option<(u64, String)>>,
    /// Typed `/rewind`: the Composer asks the Thread (which holds the dialog) to open it on that message.
    pub rewind: RwSignal<Option<(u64, UserItem)>>,
}

impl UiState {
    pub fn new() -> UiState {
        todo!("port: UiState defaults")
    }

    /// `Ui.RaiseKey(key)`.
    pub fn raise_key(&self, key: &str) {
        todo!("port: UiState.RaiseKey")
    }

    /// `Ui.RaiseRewind(user_item)`.
    pub fn request_rewind(&self, item: UserItem) {
        todo!("port: UiState.RaiseRewind")
    }
}

/// One session's store.
#[derive(Clone, Copy)]
pub struct SessionHandle {
    pub id: StoredValue<String>,
    /// False until the first snapshot arrived.
    pub loaded: RwSignal<bool>,
    /// None = unknown yet; Some(false) = no such session (SessionPage shows "not found").
    pub found: RwSignal<Option<bool>>,
    pub version: RwSignal<u64>,
    pub meta: RwSignal<SessionMeta>,
    pub items: RwSignal<Vec<Item>>,
    /// Streaming text only (StreamingTail).
    pub stream: RwSignal<String>,
    /// Memory panel edit (front-end only; survives the panel unmounting).
    pub memory: RwSignal<MemoryEdit>,
}

impl SessionHandle {
    /// Replace everything from a snapshot.
    pub fn apply_snapshot(&self, snap: SessionSnapshot) {
        todo!("port: (first render state)")
    }

    /// Apply a patch: ignore if not newer; truncate to len; write upserts; replace meta; bump version.
    pub fn apply_patch(&self, patch: SessionPatch) {
        todo!("port: LiveSession.Changed → re-render (version guard)")
    }

    pub fn id(&self) -> String {
        self.id.get_value()
    }
}

/// App-wide store.
#[derive(Clone, Copy)]
pub struct AppStore {
    pub boot: StoredValue<BootInfo>,
    pub summary: RwSignal<SessionsSummary>,
    /// Last `TranscriptsInvalidated` generation (the Rail reloads recents when it moves).
    pub transcripts_generation: RwSignal<u64>,
    pub worktree_count: RwSignal<Option<i32>>,
    pub bg_needs: RwSignal<i32>,
    pub prefs: RwSignal<Prefs>,
    pub flags: RwSignal<Flags>,
    /// Bumped per `Push::Bookmarks` (session id, sequence).
    pub bookmarks: RwSignal<Option<(u64, String)>>,
    /// Steps of the running worktree cleanup (Worktrees page) and the finished run id.
    pub wt_steps: RwSignal<Vec<WorktreeRunStep>>,
    pub wt_done: RwSignal<Option<u64>>,
    /// Navigation requested by the backend (notification click), with a sequence number.
    pub navigate: RwSignal<Option<(u64, String)>>,
    /// An update is installing.
    pub updating: RwSignal<bool>,
    sessions: StoredValue<HashMap<String, SessionHandle>>,
    owner: StoredValue<Owner>,
}

impl AppStore {
    pub fn new(boot: BootInfo) -> AppStore {
        todo!("port: (app-wide signals under the root owner)")
    }

    /// The store of session `id`, created (empty, loaded = false) under the root owner on first use.
    pub fn session(&self, id: &str) -> SessionHandle {
        todo!("port: (per-session store)")
    }

    /// Fetch `session_open(id)` into the store (SessionPage resolution: live or transcript, else found = false).
    pub fn load_session(&self, id: &str) {
        todo!("port: SessionPage.OnParametersSet resolve")
    }

    /// Dispatch one backend push to the signals.
    pub fn apply_push(&self, push: Push) {
        todo!("port: (every C# Changed subscription)")
    }
}

/// Create both stores, provide them as context, subscribe to pushes, load the initial summary. Call once in App.
pub fn provide_stores(boot: BootInfo) {
    todo!("port: Program/App service wiring")
}

pub fn use_ui() -> UiState {
    expect_context::<UiState>()
}

pub fn use_app() -> AppStore {
    expect_context::<AppStore>()
}
