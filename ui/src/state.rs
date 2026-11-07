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

use crate::ipc;
use ccui_shared::api::{self as api, BootInfo, Flags, Prefs, WorktreeRunStep};
use ccui_shared::memory::MemoryEdit;
use ccui_shared::model::{Item, ToolItem, UserItem};
use ccui_shared::push::Push;
use ccui_shared::session::{
    SessionDraft, SessionHeavy, SessionMeta, SessionPatch, SessionSnapshot, SessionStream, SessionsSummary,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
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

/// Next value of a sequenced event signal: identical consecutive events still differ by their number.
fn bump<T: Send + Sync + 'static>(sig: RwSignal<Option<(u64, T)>>, value: T) {
    sig.update(|v| {
        let n = v.as_ref().map_or(1, |(n, _)| n + 1);
        *v = Some((n, value));
    });
}

impl UiState {
    pub fn new() -> UiState {
        UiState {
            inspector_open: RwSignal::new(true),
            palette_open: RwSignal::new(false),
            diff_open: RwSignal::new(false),
            selected_tool_id: RwSignal::new(None),
            selected_phase: RwSignal::new(None),
            key: RwSignal::new(None),
            rewind: RwSignal::new(None),
        }
    }

    /// `Ui.RaiseKey(key)`.
    pub fn raise_key(&self, key: &str) {
        bump(self.key, key.to_string());
    }

    /// `Ui.RaiseRewind(user_item)`.
    pub fn request_rewind(&self, item: UserItem) {
        bump(self.rewind, item);
    }
}

impl Default for UiState {
    fn default() -> Self {
        UiState::new()
    }
}

// ------------------------------------------------------------------------------------------- pure sync rules

/// What a patch does to a handle holding `held` with `len_now` rows (ARCHITECTURE §2 rule).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PatchStep {
    /// `version <= held`: stale.
    Ignore,
    /// A patch was missed (`base_version > held`), or the upserts leave a hole: re-fetch the snapshot.
    Gap,
    Apply,
}

pub(crate) fn patch_step(held: u64, len_now: usize, p: &SessionPatch) -> PatchStep {
    if p.version <= held {
        return PatchStep::Ignore;
    }
    if p.base_version > held {
        return PatchStep::Gap;
    }
    // After the truncate to `len`, every upsert lands on an existing row or right after the last one, and the rows
    // end exactly at `len`.
    let mut n = len_now.min(p.len);
    for (i, _) in &p.items {
        if *i > n {
            return PatchStep::Gap;
        }
        if *i == n {
            n += 1;
        }
    }
    if n != p.len {
        return PatchStep::Gap;
    }
    PatchStep::Apply
}

/// The streaming text after an increment, None = gap (re-fetch).
pub(crate) fn stream_next(held: &str, s: &SessionStream) -> Option<String> {
    if s.offset == 0 {
        return Some(s.append.clone());
    }
    if s.offset <= held.len() && held.is_char_boundary(s.offset) {
        let mut t = String::with_capacity(s.offset + s.append.len());
        t.push_str(&held[..s.offset]);
        t.push_str(&s.append);
        return Some(t);
    }
    None
}

/// Whether an incoming draft replaces the held one: a new rev (backend write: cut_back), or a handle that holds
/// nothing yet (first snapshot of a fresh handle, whose rev may still be 0).
pub(crate) fn adopt_draft(held: &SessionDraft, incoming: &SessionDraft) -> bool {
    incoming.rev != held.rev || (*held == SessionDraft::default() && incoming != held)
}

/// A placeholder ToolItem for `tool_at` before its row exists (never shown: rows are keyed on existing indexes).
fn empty_tool() -> ToolItem {
    ToolItem {
        id: String::new(),
        name: String::new(),
        input: serde_json::Value::Null,
        parent_tool_use_id: None,
        state: Default::default(),
        result_text: None,
        structured: None,
        started_at: Default::default(),
        ended_at: None,
        task_id: None,
        task_status: None,
        progress: None,
        tokens: 0,
        sub_tool_uses: 0,
        background: false,
        model: None,
    }
}

// ------------------------------------------------------------------------------------------- session store

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
    /// Stream increments received while `loaded == false`, replayed in order after the snapshot (the backend's
    /// increments chain on what it last pushed, so replaying them by the rule converges on its text).
    pub stream_buffer: StoredValue<Vec<SessionStream>>,
    /// Components holding this session watched (`AppStore::load_*` +1, `release` −1).
    pub watchers: StoredValue<u32>,
}

impl SessionHandle {
    fn new(id: &str) -> SessionHandle {
        SessionHandle {
            id: StoredValue::new(id.to_string()),
            loaded: RwSignal::new(false),
            found: RwSignal::new(None),
            version: RwSignal::new(0),
            meta: RwSignal::new(SessionMeta::default()),
            heavy: RwSignal::new(SessionHeavy::default()),
            draft: RwSignal::new(SessionDraft::default()),
            items: RwSignal::new(Vec::new()),
            stream: RwSignal::new(String::new()),
            memory: RwSignal::new(MemoryEdit::default()),
            buffer: StoredValue::new(Vec::new()),
            stream_buffer: StoredValue::new(Vec::new()),
            watchers: StoredValue::new(0),
        }
    }

    /// Replace everything from a snapshot (meta, heavy, items, stream, version; draft only on a new rev), set
    /// loaded, then drain `buffer` through `apply_patch` (patches with `version <= snapshot.version` are dropped).
    pub fn apply_snapshot(&self, snap: SessionSnapshot) {
        if self.watchers.get_value() == 0 {
            return; // released while it was in flight: the backend no longer watches it
        }
        let SessionSnapshot { version, meta, heavy, draft, items, streaming_text } = snap;
        let len = items.len();
        self.write_rows(len, items.into_iter().enumerate().collect());
        self.meta.set(meta);
        if self.heavy.with_untracked(|h| *h != heavy) {
            self.heavy.set(heavy);
        }
        if self.draft.with_untracked(|d| adopt_draft(d, &draft)) {
            self.draft.set(draft);
        }
        self.stream.set(streaming_text);
        self.version.set(version);
        self.found.set(Some(true));
        self.loaded.set(true);
        let patches = self.buffer.try_update_value(std::mem::take).unwrap_or_default();
        for p in patches {
            self.apply_patch(p);
        }
        let streams = self.stream_buffer.try_update_value(std::mem::take).unwrap_or_default();
        for s in streams {
            self.apply_stream(s);
        }
    }

    /// Apply a patch by the `SessionPatch` rule. While `loaded == false`, push it to `buffer` instead (a snapshot
    /// is in flight; everything emitted after the backend took it must survive). `version <= held` → ignore;
    /// `base_version > held`, or an upsert index > current length → gap: buffer it and re-fetch `session_snapshot`
    /// (loaded = false meanwhile). Else truncate to `len`, upsert rows (existing row: `set`; new index: push a new
    /// `ArcRwSignal`), replace meta, replace heavy / draft (draft only on a new rev) when present, version = patch.
    pub fn apply_patch(&self, patch: SessionPatch) {
        if !self.loaded.get_untracked() {
            self.buffer.update_value(|b| b.push(patch));
            return;
        }
        let held = self.version.get_untracked();
        let len_now = self.items.with_untracked(Vec::len);
        match patch_step(held, len_now, &patch) {
            PatchStep::Ignore => {}
            PatchStep::Gap => {
                self.buffer.update_value(|b| b.push(patch));
                self.refetch();
            }
            PatchStep::Apply => {
                let SessionPatch { version, meta, heavy, draft, items, len, .. } = patch;
                self.write_rows(len, items);
                self.meta.set(meta);
                if let Some(h) = heavy {
                    self.heavy.set(h);
                }
                if let Some(d) = draft {
                    if self.draft.with_untracked(|cur| cur.rev != d.rev) {
                        self.draft.set(d);
                    }
                }
                self.version.set(version);
            }
        }
    }

    /// Apply a stream increment by the `SessionStream` rule (gap → re-fetch like `apply_patch`).
    pub fn apply_stream(&self, stream: SessionStream) {
        if !self.loaded.get_untracked() {
            self.stream_buffer.update_value(|b| b.push(stream));
            return;
        }
        match self.stream.with_untracked(|t| stream_next(t, &stream)) {
            Some(t) => self.stream.set(t),
            None => self.refetch(),
        }
    }

    /// Gap: loaded = false (patches buffer meanwhile) and read the snapshot again. One fetch per gap: later patches
    /// buffer instead of fetching. A failed read gives the handle back as it was; the next patch finds the gap again.
    fn refetch(&self) {
        if !self.loaded.get_untracked() {
            return;
        }
        self.loaded.set(false);
        let h = *self;
        spawn_local(async move {
            match ipc::session_snapshot(&h.id()).await {
                Ok(Some(s)) => h.apply_snapshot(s),
                // Gone: `SessionRemoved` follows and the page re-resolves.
                Ok(None) => {}
                Err(e) => {
                    leptos::logging::error!("session_snapshot: {e}");
                    if h.watchers.get_value() > 0 {
                        h.loaded.set(true);
                    }
                }
            }
        });
    }

    /// Truncate to `len`, then write the upserts (ascending; checked contiguous by `patch_step`). Existing rows are
    /// set only when they differ, and the list signal moves only when its length does.
    fn write_rows(&self, len: usize, upserts: Vec<(usize, Item)>) {
        let cur = self.items.with_untracked(Vec::len);
        let keep = cur.min(len);
        let mut fresh = Vec::new();
        for (i, item) in upserts {
            if i < keep {
                let row = self.items.with_untracked(|v| v[i].clone());
                if row.with_untracked(|x| *x != item) {
                    row.set(item);
                }
            } else {
                fresh.push(ArcRwSignal::new(item));
            }
        }
        if keep != cur || !fresh.is_empty() {
            self.items.update(|v| {
                v.truncate(keep);
                v.extend(fresh);
            });
        }
    }

    pub fn id(&self) -> String {
        self.id.get_value()
    }

    /// The row signal of item `index` (None past the end). Tracks the list (a truncate drops it).
    pub fn row(&self, index: usize) -> Option<ArcRwSignal<Item>> {
        self.items.with(|v| v.get(index).cloned())
    }

    /// The ToolItem at `index` as a signal for a keyed row component (`Signal<ToolItem>` props): re-runs only when
    /// that row changes. A non-tool / missing row reads as the last value it had (rows are replaced, not retyped,
    /// while a component keyed on them is alive).
    pub fn tool_at(&self, index: usize) -> Signal<ToolItem> {
        let h = *self;
        Memo::new(move |prev: Option<&ToolItem>| {
            h.row(index)
                .and_then(|r| r.with(|i| i.as_tool().cloned()))
                .or_else(|| prev.cloned())
                .unwrap_or_else(empty_tool)
        })
        .into()
    }

    /// The last ToolItem with id `id` (inspector selection), None when absent.
    pub fn tool_by_id(&self, id: String) -> Signal<Option<ToolItem>> {
        let h = *self;
        Memo::new(move |_| {
            h.with_items(|items| items.iter().rev().find_map(|i| i.as_tool().filter(|t| t.id == id).cloned()))
        })
        .into()
    }

    /// Run `f` over every item without cloning (`&[&Item]` fits the `I: Borrow<Item>` helpers: thread_blocks,
    /// todo_list, session_rules). Tracks every row.
    pub fn with_items<R>(&self, f: impl FnOnce(&[&Item]) -> R) -> R {
        self.items.with(|rows| {
            let guards: Vec<_> = rows.iter().map(|r| r.read()).collect();
            let refs: Vec<&Item> = guards.iter().map(|g| &**g).collect();
            f(&refs)
        })
    }
}

// ------------------------------------------------------------------------------------------- app store

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

/// Apply a `PrefChanged` to the prefs / flags records (flags: value present = on). False for an unknown name.
pub(crate) fn apply_pref(prefs: &mut Prefs, flags: &mut Flags, name: &str, value: Option<String>) -> bool {
    let on = value.is_some();
    match name {
        api::PREF_THEME => prefs.theme = value,
        api::PREF_CODE_SIZE => prefs.code_size = value,
        api::PREF_RAIL_WIDTH => prefs.rail_width = value,
        api::PREF_INSPECTOR_WIDTH => prefs.inspector_width = value,
        api::FLAG_THINKING => flags.thinking = on,
        api::FLAG_TIMESTAMPS => flags.timestamps = on,
        api::FLAG_NOTIFY => flags.notify = on,
        _ => return false,
    }
    true
}

impl AppStore {
    pub fn new(boot: BootInfo) -> AppStore {
        let owner = Owner::current().unwrap_or_default();
        AppStore {
            prefs: RwSignal::new(boot.prefs.clone()),
            flags: RwSignal::new(boot.flags.clone()),
            boot: StoredValue::new(boot),
            summary: RwSignal::new(SessionsSummary::default()),
            transcripts_generation: RwSignal::new(0),
            worktree_count: RwSignal::new(None),
            bg_needs: RwSignal::new(0),
            bookmarks: RwSignal::new(None),
            wt_steps: RwSignal::new(Vec::new()),
            wt_done: RwSignal::new(None),
            navigate: RwSignal::new(None),
            updating: RwSignal::new(false),
            sessions: StoredValue::new(HashMap::new()),
            owner: StoredValue::new(owner),
        }
    }

    /// The store of session `id`, created (empty, loaded = false) under the root owner on first use.
    pub fn session(&self, id: &str) -> SessionHandle {
        if let Some(h) = self.existing(id) {
            return h;
        }
        let h = self.owner.with_value(|o| o.clone()).with(|| SessionHandle::new(id));
        self.sessions.update_value(|m| {
            m.insert(id.to_string(), h);
        });
        h
    }

    fn existing(&self, id: &str) -> Option<SessionHandle> {
        self.sessions.with_value(|m| m.get(id).copied())
    }

    /// SessionPage resolution: `session_open(id)` (live, else its transcript opened as history; the backend
    /// watches it and returns the snapshot of that same lock) → `apply_snapshot`, else found = false. watchers += 1.
    /// Pair with `release(id)` in the page's `on_cleanup`.
    pub fn load_session(&self, id: &str) {
        self.load(id, false);
    }

    /// Live only (Extensions: "Live = Sessions.Get only", never opens a transcript): `session_watch(id)` →
    /// `apply_snapshot`, else found = false. watchers += 1; pair with `release(id)`.
    pub fn load_live(&self, id: &str) {
        self.load(id, true);
    }

    /// The first holder asks the backend (which marks it watched); later holders share that answer.
    fn load(&self, id: &str, live_only: bool) {
        let h = self.session(id);
        let before = h.watchers.get_value();
        h.watchers.set_value(before + 1);
        if before > 0 {
            return;
        }
        h.loaded.set(false);
        let id = id.to_string();
        spawn_local(async move {
            let got = if live_only { ipc::session_watch(&id).await } else { ipc::session_open(&id).await };
            match got {
                Ok(Some(snap)) => h.apply_snapshot(snap),
                Ok(None) => h.found.set(Some(false)),
                Err(e) => {
                    leptos::logging::error!("session_open({id}): {e}");
                    h.found.set(Some(false));
                }
            }
        });
    }

    /// watchers −= 1; at 0: `session_unwatch(id)`, loaded = false, items cleared (draft / memory kept: they must
    /// survive navigation).
    pub fn release(&self, id: &str) {
        let Some(h) = self.existing(id) else { return };
        let n = h.watchers.get_value().saturating_sub(1);
        h.watchers.set_value(n);
        if n > 0 {
            return;
        }
        h.loaded.set(false);
        h.items.set(Vec::new());
        h.stream.set(String::new());
        h.version.set(0);
        h.buffer.set_value(Vec::new());
        h.stream_buffer.set_value(Vec::new());
        let id = id.to_string();
        spawn_local(async move {
            if let Err(e) = ipc::session_unwatch(&id).await {
                leptos::logging::error!("session_unwatch({id}): {e}");
            }
        });
    }

    /// Dispatch one backend push to the signals. `SessionRemoved` drops the handle from the map (its page then
    /// re-resolves: the dropped handle reads found = None, loaded = false, and its watchers are zeroed, so a later
    /// `release` of that id is a no-op); patches / streams for an id without a handle are ignored.
    pub fn apply_push(&self, push: Push) {
        match push {
            Push::SessionPatch(p) => {
                if let Some(h) = self.existing(&p.id) {
                    h.apply_patch(p);
                }
            }
            Push::SessionStream(s) => {
                if let Some(h) = self.existing(&s.id) {
                    h.apply_stream(s);
                }
            }
            Push::Sessions(s) => self.summary.set(s),
            Push::SessionRemoved { id } => {
                if let Some(h) = self.sessions.try_update_value(|m| m.remove(&id)).flatten() {
                    h.watchers.set_value(0);
                    h.loaded.set(false);
                    h.found.set(None);
                }
            }
            Push::TranscriptsInvalidated { generation } => self.transcripts_generation.set(generation),
            Push::WorktreeCount { count } => self.worktree_count.set(count),
            Push::WorktreeRunStep(step) => self.wt_steps.update(|v| v.push(step)),
            Push::WorktreeRunDone { run_id } => self.wt_done.set(Some(run_id)),
            Push::BgAgents { needs_you } => self.bg_needs.set(needs_you),
            Push::Bookmarks { session_id } => bump(self.bookmarks, session_id),
            Push::PrefChanged { name, value } => self.pref_changed(&name, value),
            Push::Navigate { path } => bump(self.navigate, path),
            Push::Updating => self.updating.set(true),
        }
    }

    /// A pref written elsewhere (palette, another window): records, `<html>` and the boot mirror follow.
    fn pref_changed(&self, name: &str, value: Option<String>) {
        match name {
            api::PREF_THEME => crate::interop::set_theme(value.as_deref().unwrap_or("graphite")),
            api::PREF_CODE_SIZE => crate::interop::set_code_size(value.as_deref().unwrap_or("13px")),
            api::PREF_RAIL_WIDTH => crate::interop::set_pane_width("rail", value.as_deref()),
            api::PREF_INSPECTOR_WIDTH => crate::interop::set_pane_width("insp", value.as_deref()),
            _ => {}
        }
        let (mut prefs, mut flags) = (self.prefs.get_untracked(), self.flags.get_untracked());
        if !apply_pref(&mut prefs, &mut flags, name, value) {
            return;
        }
        self.prefs.set(prefs.clone());
        self.flags.set(flags.clone());
        self.boot.update_value(|b| {
            b.prefs = prefs;
            b.flags = flags;
        });
        ipc::save_boot_mirror(&self.boot.get_value());
    }
}

/// Create both stores and provide them as context, then (spawned) `ipc::listen_push(apply_push).await` FIRST, and
/// only after it resolved: `sessions_summary`, `bg_needs_you`, `worktrees_cleanable_count`, `app_boot` (→
/// `ipc::save_boot_mirror`), so no push can fall between a read and the subscription. Call once in App.
pub fn provide_stores(boot: BootInfo) {
    let ui = UiState::new();
    let app = AppStore::new(boot);
    provide_context(ui);
    provide_context(app);
    spawn_local(async move {
        if let Err(e) = ipc::listen_push(move |p| app.apply_push(p)).await {
            leptos::logging::error!("listen_push: {e}");
        }
        match ipc::sessions_summary().await {
            Ok(s) => app.summary.set(s),
            Err(e) => leptos::logging::error!("sessions_summary: {e}"),
        }
        if let Ok(n) = ipc::bg_needs_you().await {
            app.bg_needs.set(n);
        }
        if let Ok(n) = ipc::worktrees_cleanable_count().await {
            app.worktree_count.set(n);
        }
        if let Ok(b) = ipc::app_boot().await {
            ipc::save_boot_mirror(&b);
            app.prefs.set(b.prefs.clone());
            app.flags.set(b.flags.clone());
            app.boot.set_value(b);
        }
    });
}

pub fn use_ui() -> UiState {
    expect_context::<UiState>()
}

pub fn use_app() -> AppStore {
    expect_context::<AppStore>()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patch(base: u64, version: u64, items: Vec<usize>, len: usize) -> SessionPatch {
        SessionPatch {
            id: "s".into(),
            base_version: base,
            version,
            meta: SessionMeta::default(),
            heavy: None,
            draft: None,
            items: items.into_iter().map(|i| (i, Item::Reset)).collect(),
            len,
        }
    }

    #[test]
    fn patch_rule() {
        // stale
        assert_eq!(patch_step(5, 3, &patch(4, 5, vec![], 3)), PatchStep::Ignore);
        assert_eq!(patch_step(5, 3, &patch(2, 4, vec![], 3)), PatchStep::Ignore);
        // missed patch
        assert_eq!(patch_step(5, 3, &patch(6, 7, vec![], 3)), PatchStep::Gap);
        // base below held still applies (covers (base, version])
        assert_eq!(patch_step(5, 3, &patch(4, 6, vec![2], 3)), PatchStep::Apply);
        // append one row
        assert_eq!(patch_step(5, 3, &patch(5, 6, vec![3], 4)), PatchStep::Apply);
        // hole: index past the end
        assert_eq!(patch_step(5, 3, &patch(5, 6, vec![4], 5)), PatchStep::Gap);
        // rows missing up to len
        assert_eq!(patch_step(5, 3, &patch(5, 6, vec![], 4)), PatchStep::Gap);
        // truncate (rewind) then rewrite the last kept row
        assert_eq!(patch_step(5, 10, &patch(5, 6, vec![1], 2)), PatchStep::Apply);
        // truncate then append contiguous rows
        assert_eq!(patch_step(5, 10, &patch(5, 6, vec![2, 3], 4)), PatchStep::Apply);
        // meta-only patch
        assert_eq!(patch_step(0, 0, &patch(0, 1, vec![], 0)), PatchStep::Apply);
    }

    fn s(offset: usize, append: &str) -> SessionStream {
        SessionStream { id: "s".into(), offset, append: append.into() }
    }

    #[test]
    fn stream_rule() {
        assert_eq!(stream_next("abc", &s(0, "x")).as_deref(), Some("x"));
        assert_eq!(stream_next("abc", &s(0, "")).as_deref(), Some(""));
        assert_eq!(stream_next("abc", &s(3, "de")).as_deref(), Some("abcde"));
        assert_eq!(stream_next("abc", &s(2, "Z")).as_deref(), Some("abZ"));
        assert_eq!(stream_next("abc", &s(4, "e")), None);
        // never split a char
        assert_eq!(stream_next("é", &s(1, "x")), None);
    }

    #[test]
    fn draft_adoption() {
        let d = |rev, text: &str| SessionDraft { rev, text: text.into(), images: vec![] };
        assert!(!adopt_draft(&d(1, "mine"), &d(1, "old")));
        assert!(adopt_draft(&d(1, "mine"), &d(2, "restored")));
        // a fresh handle takes the backend's draft even at rev 0
        assert!(adopt_draft(&SessionDraft::default(), &d(0, "kept")));
        assert!(!adopt_draft(&SessionDraft::default(), &SessionDraft::default()));
    }

    #[test]
    fn prefs_and_flags() {
        let (mut p, mut f) = (Prefs::default(), Flags::default());
        assert!(apply_pref(&mut p, &mut f, "theme", Some("encre".into())));
        assert!(apply_pref(&mut p, &mut f, "notify-on", Some(String::new())));
        assert!(apply_pref(&mut p, &mut f, "rail-width", Some("240px".into())));
        assert!(!apply_pref(&mut p, &mut f, "culture", Some("fr".into())));
        assert_eq!(p.theme.as_deref(), Some("encre"));
        assert_eq!(p.rail_width.as_deref(), Some("240px"));
        assert!(f.notify);
        assert!(apply_pref(&mut p, &mut f, "notify-on", None));
        assert!(!f.notify);
    }

    #[test]
    fn sequenced_events_refire() {
        let owner = Owner::new();
        owner.with(|| {
            let ui = UiState::new();
            ui.raise_key("Escape");
            ui.raise_key("Escape");
            assert_eq!(ui.key.get_untracked(), Some((2, "Escape".to_string())));
        });
    }

    fn tool(id: &str) -> Item {
        let mut t = empty_tool();
        t.id = id.into();
        Item::Tool(t)
    }

    fn snap(version: u64, items: Vec<Item>, text: &str) -> SessionSnapshot {
        SessionSnapshot {
            version,
            meta: SessionMeta::default(),
            heavy: SessionHeavy::default(),
            draft: SessionDraft::default(),
            items,
            streaming_text: text.into(),
        }
    }

    #[test]
    fn handle_applies_rows_in_place() {
        let owner = Owner::new();
        owner.with(|| {
            let h = SessionHandle::new("s");
            h.watchers.set_value(1);
            h.apply_snapshot(snap(3, vec![Item::Reset, tool("a")], "hi"));
            assert!(h.loaded.get_untracked());
            let first = h.items.with_untracked(|v| v[1].clone());
            let mut p = patch(3, 4, vec![], 3);
            p.items = vec![(1, tool("b")), (2, tool("c"))];
            h.apply_patch(p);
            assert_eq!(h.version.get_untracked(), 4);
            let ids: Vec<String> = h.with_items(|i| i.iter().filter_map(|x| x.as_tool()).map(|t| t.id.clone()).collect());
            assert_eq!(ids, ["b", "c"]);
            // the existing row signal was written in place, not replaced
            assert!(first.with_untracked(|x| x.as_tool().is_some_and(|t| t.id == "b")));
            // stale ignored, truncation applied
            h.apply_patch(patch(1, 2, vec![], 0));
            assert_eq!(h.items.with_untracked(Vec::len), 3);
            h.apply_patch(patch(4, 5, vec![], 1));
            assert_eq!(h.items.with_untracked(Vec::len), 1);
            h.apply_stream(s(2, " there"));
            assert_eq!(h.stream.get_untracked(), "hi there");
            // while not loaded, patches and streams buffer and drain after the snapshot (stale ones dropped)
            h.loaded.set(false);
            h.apply_patch(patch(5, 6, vec![1], 2));
            h.apply_patch(patch(1, 2, vec![], 0));
            h.apply_stream(s(0, "new"));
            assert_eq!(h.buffer.with_value(Vec::len), 2);
            h.apply_snapshot(snap(5, vec![Item::Reset], ""));
            assert_eq!(h.version.get_untracked(), 6);
            assert_eq!(h.items.with_untracked(Vec::len), 2);
            assert_eq!(h.buffer.with_value(Vec::len), 0);
            assert_eq!(h.stream.get_untracked(), "new");
            // released: a late snapshot is dropped
            h.watchers.set_value(0);
            h.loaded.set(false);
            h.apply_snapshot(snap(9, vec![], ""));
            assert!(!h.loaded.get_untracked());
        });
    }
}
