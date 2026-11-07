//! Port of `Components/Session/ModeMenu.razor` — spec 07 §14. Owner: ui-session-thread-composer.

use crate::interop;
use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::session_rules::{risky, MODES};
use ccui_shared::{i18n, list_nav, t};
use leptos::prelude::*;

/// What activating `mode` does: a risky mode (auto / acceptEdits) outside an isolated worktree needs a second
/// activation (`Confirm`), else it is set.
#[derive(Debug, PartialEq, Eq)]
pub enum PickStep {
    Confirm,
    Set,
}

pub fn pick_step(mode: &str, current: &str, isolated: bool, confirm: Option<&str>) -> PickStep {
    if mode != current && risky(mode) && !isolated && confirm != Some(mode) {
        PickStep::Confirm
    } else {
        PickStep::Set
    }
}

/// Permission mode menu (MODES, risky confirm when not isolated).
/// set_permission_mode mid-session (the CLI's Shift+Tab). Same shape as ModelMenu.
#[component]
pub fn ModeMenu(session: SessionHandle, on_close: Callback<()>, on_escape: Callback<()>) -> impl IntoView {
    let error = RwSignal::new(None::<String>);
    let confirm = RwSignal::new(None::<String>);
    let current = Memo::new(move |_| session.meta.with(|m| m.mode.clone()));
    let sel = RwSignal::new(MODES.iter().position(|m| *m == current.get_untracked()).unwrap_or(0) as i32);
    let list = NodeRef::<leptos::html::Div>::new();
    let focused = StoredValue::new(false);
    Effect::new(move |_| {
        let s = sel.get();
        let Some(el) = list.get() else { return };
        if !focused.get_value() {
            focused.set_value(true);
            interop::focus(&el);
        }
        interop::reveal(&el, &format!("pm-opt-{s}"), "nearest", false);
    });
    let pick = move |mode: &'static str| {
        let isolated = session.meta.with_untracked(|m| m.isolated);
        if pick_step(mode, &current.get_untracked(), isolated, confirm.get_untracked().as_deref()) == PickStep::Confirm {
            confirm.set(Some(mode.to_string()));
            return;
        }
        let id = session.id();
        leptos::task::spawn_local(async move {
            match ipc::session_set_mode(&id, mode).await {
                Ok(()) => on_close.run(()),
                Err(e) => {
                    let _ = error.try_set(Some(e));
                }
            }
        });
    };
    let key = move |e: leptos::ev::KeyboardEvent| {
        e.prevent_default();
        match e.key().as_str() {
            "Enter" | " " => pick(MODES[sel.get_untracked().clamp(0, MODES.len() as i32 - 1) as usize]),
            "Escape" | "Tab" => on_escape.run(()),
            k => sel.set(list_nav::next(k, sel.get_untracked(), MODES.len() as i32)),
        }
    };
    let status = move || {
        error.get().unwrap_or_else(|| confirm.get().map(|c| t!("Mode.ConfirmAgain", i18n::name("Mode.Name.", Some(&c)))).unwrap_or_default())
    };
    let options = move || {
        let cur = current.get();
        let s = sel.get();
        MODES
            .iter()
            .enumerate()
            .map(|(i, &m)| {
                let on = m == cur;
                let class = match (on, i as i32 == s) {
                    (true, true) => "si on kb",
                    (true, false) => "si on",
                    (false, true) => "si kb",
                    (false, false) => "si",
                };
                view! {
                    <div id=format!("pm-opt-{i}") class=class role="option" aria-selected=if on { "true" } else { "false" }
                        on:click=move |_| pick(m)>
                        <b class=risky(m).then_some("risk")>{i18n::name("Mode.Name.", Some(m))}</b>
                        <span>{t!(&format!("Mode.{m}"))}</span>
                        {if on { view! { <span class="tag acc">{t!("Model.Current")}</span> }.into_any() } else { view! { <span></span> }.into_any() }}
                    </div>
                }
            })
            .collect_view()
    };
    view! {
        <div class="pop slash menu">
            <div class="sh"><span>{t!("Composer.PermissionMode")}</span>
                <span role="status" class=move || confirm.with(Option::is_some).then_some("risk")>{status}</span></div>
            <div role="listbox" aria-label=t!("Composer.PermissionMode") node_ref=list tabindex="0"
                aria-activedescendant=move || format!("pm-opt-{}", sel.get()) on:keydown=key>
                {options}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risky_needs_two() {
        assert_eq!(pick_step("auto", "default", false, None), PickStep::Confirm);
        assert_eq!(pick_step("auto", "default", false, Some("auto")), PickStep::Set);
        assert_eq!(pick_step("auto", "default", true, None), PickStep::Set);
        assert_eq!(pick_step("plan", "default", false, None), PickStep::Set);
        assert_eq!(pick_step("acceptEdits", "acceptEdits", false, None), PickStep::Set);
    }
}
