//! Port of `Components/Layout/MainLayout.razor` — spec 06 §2. Owner: ui-shell.
//! DOM is load-bearing (app.css grid): `.app` > Rail, `main` (pages render `.pane` [+ `aside.pane.insp`]), then the
//! two splitters as direct children after `main`; CommandPalette, IconSprite and the error toast follow `.app`.
//! Behaviour to port: shortcuts (interop::register_shortcuts → palette / inspector / new / overview / next
//! decision / relay keys to UiState), splitter release → app_pref_set(rail-width | inspector-width),
//! backend Navigate pushes → router, focus h1 after navigation. (Notifications moved to the backend.)

use crate::common::duck::{Duck, Mood};
use crate::common::icon::Icon;
use crate::layout::command_palette::CommandPalette;
use crate::layout::icon_sprite::IconSprite;
use crate::layout::rail::Rail;
use ccui_shared::t;
use leptos::prelude::*;

#[component]
pub fn MainLayout(children: Children) -> impl IntoView {
    view! {
        <div class="app">
            <Rail/>
            <main>{children()}</main>
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
