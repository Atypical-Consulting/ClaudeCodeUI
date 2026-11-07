//! Port of `Components/Layout/MainLayout.razor` — spec 06 §2. Owner: ui-shell.
//! DOM is load-bearing (app.css grid): `.app` > Rail, `main` (pages render `.pane` [+ `aside.pane.insp`]), then the
//! two splitters as direct children after `main`; CommandPalette, IconSprite and the error toast follow `.app`.
//! Behaviour: shortcuts (interop::register_shortcuts → palette / inspector / new / overview / next decision / relay
//! keys to UiState), splitter release → app_pref_set(rail-width | inspector-width), backend Navigate pushes →
//! router, focus h1 after navigation (FocusOnNavigate). Notifications moved to the backend.

use crate::common::duck::{Duck, Mood};
use crate::common::icon::Icon;
use crate::interop;
use crate::ipc;
use crate::layout::command_palette::CommandPalette;
use crate::layout::icon_sprite::IconSprite;
use crate::layout::rail::Rail;
use crate::state::{use_app, use_ui};
use ccui_shared::api::{PREF_INSPECTOR_WIDTH, PREF_RAIL_WIDTH};
use ccui_shared::t;
use leptos::prelude::*;
use leptos_router::hooks::{use_location, use_navigate};
use leptos_router::NavigateOptions;

/// What a global shortcut does (`OnShortcut`).
#[derive(Debug, PartialEq, Eq)]
pub enum Shortcut {
    TogglePalette,
    ToggleInspector,
    /// Close the palette and navigate (base-relative path).
    Go(String),
    /// Ctrl+Shift+A: the first queued decision's session, if any.
    NextDecision,
    ClosePalette,
    /// Escape / Enter / Shift+Enter / Delete → `UiState.key` (SessionPage interrupt, PermissionCard answers).
    Relay(String),
}

/// `OnShortcut(key)`: navigation and toggles here; Escape closes an open palette, else it is relayed.
pub fn shortcut(key: &str, palette_open: bool) -> Shortcut {
    match key {
        "Ctrl+K" => Shortcut::TogglePalette,
        "Ctrl+I" => Shortcut::ToggleInspector,
        "Ctrl+N" => Shortcut::Go(String::new()),
        "Ctrl+Shift+O" => Shortcut::Go("overview".into()),
        "Ctrl+Shift+A" => Shortcut::NextDecision,
        "Escape" if palette_open => Shortcut::ClosePalette,
        k => Shortcut::Relay(k.to_string()),
    }
}

#[component]
pub fn MainLayout(children: Children) -> impl IntoView {
    let ui = use_ui();
    let app = use_app();
    let navigate = use_navigate();
    let location = use_location();

    // Registered once, after the first render (the splitters must exist to sync their aria values).
    {
        let navigate = navigate.clone();
        Effect::new(move |done: Option<()>| {
            if done.is_some() {
                return;
            }
            let navigate = navigate.clone();
            interop::register_shortcuts(move |key| {
                let go = |path: &str| {
                    ui.palette_open.set(false);
                    navigate(&format!("/{path}"), NavigateOptions::default());
                };
                match shortcut(&key, ui.palette_open.get_untracked()) {
                    Shortcut::TogglePalette => ui.palette_open.update(|o| *o = !*o),
                    Shortcut::ToggleInspector => ui.inspector_open.update(|o| *o = !*o),
                    Shortcut::Go(p) => go(&p),
                    Shortcut::NextDecision => {
                        if let Some(id) = app.summary.with_untracked(|s| s.decision_queue.first().map(|d| d.session_id.clone())) {
                            go(&format!("session/{id}"));
                        }
                    }
                    Shortcut::ClosePalette => ui.palette_open.set(false),
                    Shortcut::Relay(k) => ui.raise_key(&k),
                }
            });
            // A splitter let go: kept in the backend (AppPref); None = back to the default (the pref file deleted).
            interop::register_splitters(|which, width| {
                let pref = if which == "rail" { PREF_RAIL_WIDTH } else { PREF_INSPECTOR_WIDTH };
                leptos::task::spawn_local(async move {
                    if let Err(e) = ipc::app_pref_set(pref, width.as_deref()).await {
                        leptos::logging::error!("app_pref_set({pref}): {e}");
                    }
                });
            });
        });
    }

    // Navigation asked by the backend (notification click).
    {
        let navigate = navigate.clone();
        Effect::new(move |_| {
            if let Some((_, path)) = app.navigate.get() {
                ui.palette_open.set(false);
                navigate(&format!("/{}", path.trim_start_matches('/')), NavigateOptions::default());
            }
        });
    }

    // FocusOnNavigate Selector="h1": after every navigation, once the page has rendered.
    Effect::new(move |_| {
        location.pathname.track();
        request_animation_frame(interop::focus_h1);
    });

    view! {
        <div class="app">
            <Rail/>
            <main>{children()}</main>
            // Splitters on the two seams (interop.js: drag, arrow keys, double-click resets).
            <div class="split" data-split="rail" role="separator" aria-orientation="vertical" tabindex="0"
                aria-label=t!("Layout.ResizeRail") title=t!("Layout.ResizeHint")></div>
            <div class="split" data-split="insp" role="separator" aria-orientation="vertical" tabindex="0"
                aria-label=t!("Layout.ResizeInspector") title=t!("Layout.ResizeHint")></div>
        </div>
        <CommandPalette/>
        <IconSprite/>
        <div id="blazor-error-ui" role="alert" data-nosnippet>
            <div class="beu">
                <div class="duck" aria-hidden="true"><Duck mood=Mood::Ko size=36/></div>
                <p><b>{t!("Layout.Error")}</b>{t!("Layout.ErrorHint")}</p>
                // rel="external": the router must not take it; main.rs wires reload / dismiss (no Leptos after a panic).
                <a href="" rel="external" class="reload btn primary"><Icon name="refresh" sm=true/>{t!("Layout.Reload")}</a>
                <button type="button" class="dismiss btn ghost" aria-label=t!("Layout.Dismiss")><Icon name="x"/></button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts() {
        assert_eq!(shortcut("Ctrl+K", false), Shortcut::TogglePalette);
        assert_eq!(shortcut("Ctrl+I", true), Shortcut::ToggleInspector);
        assert_eq!(shortcut("Ctrl+N", true), Shortcut::Go(String::new()));
        assert_eq!(shortcut("Ctrl+Shift+O", false), Shortcut::Go("overview".into()));
        assert_eq!(shortcut("Ctrl+Shift+A", false), Shortcut::NextDecision);
        assert_eq!(shortcut("Escape", true), Shortcut::ClosePalette);
        assert_eq!(shortcut("Escape", false), Shortcut::Relay("Escape".into()));
        assert_eq!(shortcut("Shift+Enter", false), Shortcut::Relay("Shift+Enter".into()));
    }
}
