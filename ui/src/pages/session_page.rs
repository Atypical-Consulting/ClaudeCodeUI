//! Port of `Components/Pages/SessionPage.razor` — spec 06 §6.2, 07 §0.1. Owner: ui-session-thread-composer.

use crate::common::icon::Icon;
use crate::ipc;
use crate::session::composer::Composer;
use crate::session::inspector::Inspector;
use crate::session::session_header::SessionHeader;
use crate::session::session_title::SessionTitle;
use crate::session::thread::Thread;
use crate::session::todo_panel::TodoPanel;
use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::*;
use ccui_shared::{i18n, t};
use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

/// What the page shows for a handle: still resolving, the session, or "not found".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resolved {
    Loading,
    Found,
    NotFound,
}

pub fn resolved(found: Option<bool>) -> Resolved {
    match found {
        Some(true) => Resolved::Found,
        Some(false) => Resolved::NotFound,
        None => Resolved::Loading,
    }
}

/// Resolve `/session/:id` (live or transcript via AppStore::load_session — which watches it; `release` in
/// on_cleanup — else not found), then the
/// pane keyed by id (SessionHeader, Thread, TodoPanel, Composer) + Inspector. Relayed Escape interrupts a running turn.
#[component]
pub fn SessionPage() -> impl IntoView {
    let app = use_app();
    let ui = use_ui();
    let params = use_params_map();
    let id = Memo::new(move |_| params.with(|p| p.get("id").unwrap_or_default()));
    let handle = RwSignal::new(None::<SessionHandle>);
    // The id this page holds a watch on (released when it changes or the page goes).
    let held = StoredValue::new(None::<String>);

    Effect::new(move |_| {
        let id = id.get();
        if held.get_value().as_deref() == Some(id.as_str()) {
            return;
        }
        if let Some(old) = held.get_value() {
            app.release(&old);
        }
        app.load_session(&id);
        held.set_value(Some(id.clone()));
        handle.set(Some(app.session(&id)));
    });
    // Ended elsewhere (Push::SessionRemoved dropped the handle: found None, nobody watching it any more): resolve the
    // id again, which re-opens its transcript here instead of driving the dead session. That handle's watch is gone
    // with it, so nothing is released for it.
    Effect::new(move |_| {
        let Some(h) = handle.get() else { return };
        if h.found.get().is_none() && !h.loaded.get() && h.watchers.get_value() == 0 {
            let id = h.id();
            if held.get_value().as_deref() != Some(id.as_str()) {
                return;
            }
            app.load_session(&id);
            handle.set(Some(app.session(&id)));
        }
    });
    on_cleanup(move || {
        if let Some(id) = held.try_get_value().flatten() {
            app.release(&id);
        }
    });

    // Relayed Escape (from the composer without a popover, or nothing focused) interrupts a running turn.
    let seen_key = StoredValue::new(ui.key.get_untracked().map_or(0, |(n, _)| n));
    Effect::new(move |_| {
        let Some((n, key)) = ui.key.get() else { return };
        if n <= seen_key.get_value() {
            return;
        }
        seen_key.set_value(n);
        let Some(h) = handle.get_untracked() else { return };
        if key != "Escape" || !h.meta.with_untracked(|m| matches!(m.status, SessionStatus::Running | SessionStatus::Waiting)) {
            return;
        }
        let id = h.id();
        leptos::task::spawn_local(async move {
            if let Err(e) = ipc::session_interrupt(&id).await {
                leptos::logging::error!("[{id}] interruption : {e}");
            }
        });
    });

    // Keyed by handle: another session (or the same id re-opened) mounts a fresh pane, whose children carry no state
    // over and whose log's initial content is not announced.
    move || {
        let h = handle.get()?;
        let state = Memo::new(move |_| resolved(h.found.get()));
        Some(view! {
            <SessionTitle session=h/>
            {move || match state.get() {
                Resolved::Found => view! {
                    <div class="pane">
                        <SessionHeader session=h/>
                        <Thread session=h/>
                        <TodoPanel session=h/>
                        <Composer session=h/>
                    </div>
                    <Inspector session=h/>
                }
                .into_any(),
                Resolved::NotFound => view! {
                    <div class="pane">
                        <div class="ph"><h1 class="ttl" tabindex="-1">{t!("Session.NotFound")}</h1></div>
                        <div class="thread">
                            <p class="prose">{t!("Session.NotFoundText")}</p>
                            <div class="acts">
                                <a class="btn primary" href=""><Icon name="plus" sm=true/>{t!("Rail.NewSession")}</a>
                                <button type="button" class="btn" on:click=move |_| ui.palette_open.set(true)>
                                    <Icon name="search" sm=true/>{t!("Rail.Search")}<kbd>{i18n::kbd("Ctrl K")}</kbd></button>
                            </div>
                        </div>
                    </div>
                }
                .into_any(),
                Resolved::Loading => view! { <div class="pane"></div> }.into_any(),
            }}
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution() {
        assert_eq!(resolved(None), Resolved::Loading);
        assert_eq!(resolved(Some(true)), Resolved::Found);
        assert_eq!(resolved(Some(false)), Resolved::NotFound);
    }
}
