//! Port of `Components/Session/RewindDialog.razor` — spec 07 §18. Owner: ui-session-thread-composer.

use crate::common::icon::Icon;
use crate::interop;
use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::model::*;
use ccui_shared::{t, tool_kinds};
use leptos::prelude::*;

/// Native modal: preview (session_preview_rewind), rewind conversation [+ files]. Opens on `target`.
/// "Rewind to here": rewind_files {dry_run} lists what goes back on disk, then the rewind restores the files and cuts
/// the conversation. Focus trap, Escape and focus return come from the browser's `<dialog>`. Every notification of
/// `target` with Some(u) opens it (the Thread sets it per click, the same message twice included).
#[component]
pub fn RewindDialog(session: SessionHandle, #[prop(into)] target: Signal<Option<UserItem>>) -> impl IntoView {
    let el = NodeRef::<leptos::html::Dialog>::new();
    let shown = RwSignal::new(None::<UserItem>);
    let preview = RwSignal::new(None::<RewindPreview>);
    let error = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    // Bumped per opening: a preview that lands after the dialog moved on to another message is dropped.
    let opening = StoredValue::new(0u64);

    Effect::new(move |first: Option<()>| {
        let t = target.get();
        if first.is_none() {
            return; // a target left from before this mount is not a request
        }
        let Some(u) = t else { return };
        let Some(uuid) = u.uuid.clone() else { return };
        shown.set(Some(u));
        preview.set(None);
        error.set(None);
        busy.set(false);
        let n = opening.get_value() + 1;
        opening.set_value(n);
        request_animation_frame(move || {
            if let Some(d) = el.get_untracked() {
                interop::dialog(&d, true, None);
            }
        });
        let id = session.id();
        leptos::task::spawn_local(async move {
            let got = ipc::session_preview_rewind(&id, &uuid).await;
            if opening.try_get_value() != Some(n) {
                return;
            }
            match got {
                Ok(p) => {
                    let _ = preview.try_set(Some(p));
                }
                Err(e) => {
                    let _ = error.try_set(Some(e));
                }
            }
        });
    });

    let confirm = move |_| {
        let (Some(u), Some(p)) = (shown.get_untracked(), preview.get_untracked()) else { return };
        let Some(uuid) = u.uuid else { return };
        busy.set(true);
        error.set(None);
        let id = session.id();
        leptos::task::spawn_local(async move {
            match ipc::session_rewind(&id, &uuid, p.changes()).await {
                Ok(()) => {
                    // The message (and its button, where focus would return) is gone: its text waits in the Composer.
                    if let Some(d) = el.get_untracked() {
                        interop::dialog(&d, false, Some(".composer textarea"));
                    }
                }
                Err(e) => {
                    let _ = error.try_set(Some(e));
                }
            }
            let _ = busy.try_set(false);
        });
    };
    let close = move |_| {
        if let Some(d) = el.get_untracked() {
            interop::dialog(&d, false, None);
        }
    };

    let body = move || {
        shown.get().map(|u| {
            let state = move || match (preview.get(), error.with(Option::is_some)) {
                (None, false) => view! { <p role="status">{t!("Rewind.Loading")}</p> }.into_any(),
                (None, true) => ().into_any(),
                (Some(p), _) if p.changes() => {
                    let cwd = session.meta.with_untracked(|m| m.cwd.clone());
                    let files = (!p.files.is_empty()).then(|| {
                        let rows = p
                            .files
                            .iter()
                            .map(|f| view! { <li><code>{tool_kinds::relative(f, &cwd)}</code></li> })
                            .collect_view();
                        view! { <ul class="files">{rows}</ul> }
                    });
                    view! {
                        <div class="cap">{t!("Rewind.Files")}" · "<span class="n">{t!("Rewind.Lines", p.insertions, p.deletions)}</span></div>
                        {files}
                    }
                    .into_any()
                }
                (Some(p), _) if p.can_rewind => view! { <p>{t!("Rewind.NoFiles")}</p> }.into_any(),
                (Some(p), _) => view! { <p>{t!("Rewind.FilesUnavailable", p.error.unwrap_or_default())}</p> }.into_any(),
            };
            view! {
                <div class="rw">
                    <h2 id="rw-title">{t!("Rewind.Title")}</h2>
                    <p id="rw-text">{t!("Rewind.Text")}</p>
                    <blockquote>{u.text}</blockquote>
                    {state}
                    {move || error.get().map(|e| view! { <p class="errbox" role="alert">{e}</p> })}
                    <div class="acts">
                        <button type="button" class="btn" autofocus on:click=close>{t!("Action.Cancel")}</button>
                        <button type="button" class="btn danger" disabled=move || busy.get() || preview.with(Option::is_none)
                            on:click=confirm><Icon name="undo" sm=true/>{t!("Rewind.Confirm")}</button>
                    </div>
                </div>
            }
        })
    };
    view! {
        // Escape, Cancel or after a rewind.
        <dialog node_ref=el class="modal" aria-labelledby="rw-title" aria-describedby="rw-text" on:close=move |_| shown.set(None)>
            {body}
        </dialog>
    }
}
