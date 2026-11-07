//! Front-end stores (Leptos context). Ports `Core/UiState.cs` (per window — spec 03 §7, 06 §3, 07 §0.2) and replaces
//! the Blazor `Changed` subscriptions by one push dispatcher (spec 06 §11). Owner: ui-shell.
//!
//! - `UiState`: per-window UI signals + the relayed key / rewind events (sequence numbers, so identical consecutive
//!   events re-fire).
//! - `AppStore`: app-wide data pushed by the backend (session summary, counts, prefs) and the per-session stores.
//! - `SessionHandle`: one session's read model (meta, heavy, draft, items, version, streaming text) + front-end-only
//!   per-session state that must outlive components (memory panel edit). Created under the root owner, keyed by
//!   session id, so it survives SessionPage remounts; dropped on `Push::SessionRemoved`. Patches apply in place
//!   (rule: `ccui_shared::session::SessionPatch`). Items are one `ArcRwSignal` per row: an upsert touches only its
//!   own row, rows freed with the list (never arena-leaked).

use ccui_shared::api::{BootInfo, Flags, Prefs, WorktreeRunStep};
use ccui_shared::memory::MemoryEdit;
use ccui_shared::model::{Item, ToolItem, UserItem};
use ccui_shared::push::Push;
use ccui_shared::session::{
    SessionDraft, SessionHeavy, SessionMeta, SessionPatch, SessionSnapshot, SessionStream, SessionsSummary,
};
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
    /// False until the first snapshot arrived (and again after `AppStore::release` unwatched it).
    pub loaded: RwSignal<bool>,
    /// None = unknown yet; Some(false) = no such session (SessionPage shows "not found").
    pub found: RwSignal<Option<bool>>,
    pub version: RwSignal<u64>,
    pub meta: RwSignal<SessionMeta>,
    pub heavy: RwSignal<SessionHeavy>,
    /// The Composer's draft. Snapshot / patch write it only when the incoming `rev` differs from the held one (a
    /// backend-originated write: cut_back); the Composer writes its own edits here (same rev) on every change, so
    /// the text survives remounts, and sends them debounced with `session_set_draft_text(text, rev)` /
    /// `session_set_draft_images(images, rev)`. On adopting a new rev it drops any pending debounced write.
    pub draft: RwSignal<SessionDraft>,
    /// One signal per item (index = item index). Read whole-list helpers through `with_items`.
    pub items: RwSignal<Vec<ArcRwSignal<Item>>>,
    /// Streaming text only (StreamingTail), accumulated from `SessionStream` increments.
    pub stream: RwSignal<String>,
    /// Memory panel edit (front-end only; survives the panel unmounting).
    pub memory: RwSignal<MemoryEdit>,
    /// Patches received while `loaded == false` (or while a gap re-fetch runs), applied after the snapshot.
    pub buffer: StoredValue<Vec<SessionPatch>>,
    /// Components holding this session watched (`AppStore::load_*` +1, `release` −1).
    pub watchers: StoredValue<u32>,
}

impl SessionHandle {
    /// Replace everything from a snapshot (meta, heavy, items, stream, version; draft only on a new rev), set
    /// loaded, then drain `buffer` through `apply_patch` (patches with `version <= snapshot.version` are dropped).
    pub fn apply_snapshot(&self, snap: SessionSnapshot) {
        todo!("port: (first render state)")
    }

    /// Apply a patch by the `SessionPatch` rule. While `loaded == false`, push it to `buffer` instead (a snapshot
    /// is in flight; everything emitted after the backend took it must survive). `version <= held` → ignore;
    /// `base_version > held`, or an upsert index > current length → gap: buffer it and re-fetch `session_snapshot`
    /// (loaded = false meanwhile). Else truncate to `len`, upsert rows (existing row: `set`; new index: push a new
    /// `ArcRwSignal`), replace meta, replace heavy / draft (draft only on a new rev) when present, version = patch.
    pub fn apply_patch(&self, patch: SessionPatch) {
        todo!("port: LiveSession.Changed → re-render (version guard)")
    }

    /// Apply a stream increment by the `SessionStream` rule (gap → re-fetch like `apply_patch`).
    pub fn apply_stream(&self, stream: SessionStream) {
        todo!("port: (streamed text render)")
    }

    pub fn id(&self) -> String {
        self.id.get_value()
    }

    /// The row signal of item `index` (None past the end). Tracks the list (a truncate drops it).
    pub fn row(&self, index: usize) -> Option<ArcRwSignal<Item>> {
        todo!("port: (per-row item signal)")
    }

    /// The ToolItem at `index` as a signal for a keyed row component (`Signal<ToolItem>` props): re-runs only when
    /// that row changes. A non-tool / missing row reads as the last value it had (rows are replaced, not retyped,
    /// while a component keyed on them is alive).
    pub fn tool_at(&self, index: usize) -> Signal<ToolItem> {
        todo!("port: (row → ToolItem)")
    }

    /// The last ToolItem with id `id` (inspector selection), None when absent.
    pub fn tool_by_id(&self, id: String) -> Signal<Option<ToolItem>> {
        todo!("port: LiveSession.Tool (reactive)")
    }

    /// Run `f` over every item without cloning (`&[&Item]` fits the `I: Borrow<Item>` helpers: thread_blocks,
    /// todo_list, session_rules). Tracks every row.
    pub fn with_items<R>(&self, f: impl FnOnce(&[&Item]) -> R) -> R {
        todo!("port: (whole-list read)")
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

    /// SessionPage resolution: `session_open(id)` (live, else its transcript opened as history; the backend
    /// watches it and returns the snapshot of that same lock) → `apply_snapshot`, else found = false. watchers += 1.
    /// Pair with `release(id)` in the page's `on_cleanup`.
    pub fn load_session(&self, id: &str) {
        todo!("port: SessionPage.OnParametersSet resolve")
    }

    /// Live only (Extensions: "Live = Sessions.Get only", never opens a transcript): `session_watch(id)` →
    /// `apply_snapshot`, else found = false. watchers += 1; pair with `release(id)`.
    pub fn load_live(&self, id: &str) {
        todo!("port: Extensions session resolve (Sessions.Get)")
    }

    /// watchers −= 1; at 0: `session_unwatch(id)`, loaded = false, items cleared (draft / memory kept: they must
    /// survive navigation).
    pub fn release(&self, id: &str) {
        todo!("port: (component disposed)")
    }

    /// Dispatch one backend push to the signals. `SessionRemoved` drops the handle from the map (its page then
    /// re-resolves); patches / streams for an id without a handle are ignored.
    pub fn apply_push(&self, push: Push) {
        todo!("port: (every C# Changed subscription)")
    }
}

/// Create both stores and provide them as context, then (spawned) `ipc::listen_push(apply_push).await` FIRST, and
/// only after it resolved: `sessions_summary`, `bg_needs_you`, `worktrees_cleanable_count`, `app_boot` (→
/// `ipc::save_boot_mirror`), so no push can fall between a read and the subscription. Call once in App.
pub fn provide_stores(boot: BootInfo) {
    todo!("port: Program/App service wiring")
}

pub fn use_ui() -> UiState {
    expect_context::<UiState>()
}

pub fn use_app() -> AppStore {
    expect_context::<AppStore>()
}
