//! Port of `Components/Session/ExtraDirs.razor` — spec 07 §20. Owner: ui-session-inspector.

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

/// A local (non-Send) future, as the wasm UI produces them.
pub type LocalFut<T> = Pin<Box<dyn Future<Output = T>>>;

/// `OnAdd(d)` (async, may fail): Ok = added (ExtraDirs clears its draft and confirm step), Err(msg) = shown as
/// `Dirs.AddFailed(msg)`. NewSession adds to its list (always Ok); ContextPanel calls `session_add_dir`.
pub type AddDir = Callback<String, LocalFut<Result<(), String>>>;

/// The parent's handle on an ExtraDirs (NewSession: "a typed folder is not dropped" — its Start awaits
/// `commit()` before starting). ExtraDirs installs its commit closure on mount.
#[derive(Clone, Copy)]
pub struct ExtraDirsCtl {
    commit: StoredValue<Option<Rc<dyn Fn() -> LocalFut<bool>>>, LocalStorage>,
}

impl ExtraDirsCtl {
    pub fn new() -> ExtraDirsCtl {
        ExtraDirsCtl { commit: StoredValue::new_local(None) }
    }

    /// `Commit()`: draft blank → true; else run Add and return whether the draft is now empty (false = the reason
    /// is shown). True when no ExtraDirs is mounted.
    pub async fn commit(&self) -> bool {
        todo!("port: ExtraDirs.Commit")
    }

    /// Called by ExtraDirs on mount (and cleared on cleanup).
    pub fn install(&self, commit: Option<Rc<dyn Fn() -> LocalFut<bool>>>) {
        self.commit.set_value(commit);
    }
}

/// `--add-dir` list + input; used by NewSession and ContextPanel. Add: `ipc::validate_extra_dir(draft, cwd, dirs)`
/// (Err = the message to show), then the `confirm` step, then busy + `on_add(d).await`.
#[component]
pub fn ExtraDirs(
    #[prop(into)] id: String,
    #[prop(into)] dirs: Signal<Vec<String>>,
    #[prop(into)] cwd: Signal<String>,
    on_add: AddDir,
    #[prop(optional)] on_remove: Option<Callback<String>>,
    #[prop(optional, into)] disabled: Option<Signal<bool>>,
    #[prop(optional, into)] confirm: Option<String>,
    #[prop(optional)] ctl: Option<ExtraDirsCtl>,
) -> impl IntoView {
    // port: ExtraDirs.razor markup + behaviour (placeholder root only)
    view! { <ul class="dirs" aria-label=t!("Dirs.Label")></ul> }
}
