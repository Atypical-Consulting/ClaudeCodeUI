//! Port of `Components/Shared/PickFolder.razor` — spec 06 §7, 04 §2. Owner: ui-shell.
//! The folder icon of a path field: opens the OS folder dialog. Busy while it is open, so a second click never
//! stacks a dialog.

use crate::common::icon::Icon;
use crate::ipc;
use ccui_shared::t;
use leptos::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Folder button: busy while the native dialog is open (ipc::pick_folder); cancel changes nothing.
#[component]
pub fn PickFolder(
    #[prop(optional, into)] start: Option<Signal<String>>,
    #[prop(optional, into)] disabled: Option<Signal<bool>>,
    on_pick: Callback<String>,
    on_error: Callback<String>,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    // Unmounted while the dialog is open: its answer goes nowhere.
    let alive = Arc::new(AtomicBool::new(true));
    on_cleanup({
        let alive = alive.clone();
        move || alive.store(false, Ordering::Relaxed)
    });
    let pick = move |_| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        // The field's value: the dialog opens there when it is an existing folder, else in ~.
        let from = start.map(|s| s.get_untracked()).filter(|s| !s.is_empty());
        let alive = alive.clone();
        leptos::task::spawn_local(async move {
            let got = ipc::pick_folder(from.as_deref(), &t!("Pick.Folder")).await;
            if !alive.load(Ordering::Relaxed) {
                return;
            }
            busy.set(false);
            match got {
                Ok(Some(d)) => on_pick.run(d),
                Ok(None) => {} // cancel changes nothing
                // No dialog on this machine: the parent shows the message in its field's error area.
                Err(_) => on_error.run(t!("Pick.None")),
            }
        });
    };
    view! {
        <button type="button" class="pickdir" disabled=move || busy.get() || disabled.is_some_and(|d| d.get())
            aria-busy=move || busy.get().then_some("true") aria-label=t!("Pick.Folder") title=t!("Pick.Folder")
            on:click=pick><Icon name="folder"/></button>
    }
}
