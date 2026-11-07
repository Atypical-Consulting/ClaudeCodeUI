//! Port of `Components/Session/StatusPanel.razor` — spec 07 §15. Owner: ui-session-inspector.
//! /status (not a stream-json command): get_status's rows as the CLI renders them, asked when shown with a process,
//! then the account: sign in through the session, sign out with `claude auth logout`.

use super::sign_in::SignIn;
use crate::common::icon::Icon;
use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// /status sections (session_refresh_status), account, sign out (claude_auth_logout).
#[component]
pub fn StatusPanel(
    session: SessionHandle,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    let confirm_out = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let has_process = Memo::new(move |_| session.meta.with(|m| m.has_process));

    let refresh = move || {
        busy.set(true);
        let id = session.id();
        spawn_local(async move {
            if let Err(e) = ipc::session_refresh_status(&id).await {
                note.set(Some(t!("Session.ActionFailed", e.trim_end())));
            }
            busy.set(false);
        });
    };
    // Asked once, as soon as there is a process that finished starting.
    let asked = StoredValue::new(false);
    Effect::new(move |_| {
        let ready = session.meta.with(|m| m.has_process && m.status != SessionStatus::Starting);
        if ready && !asked.get_value() {
            asked.set_value(true);
            refresh();
        }
    });
    // Two clicks: the account leaves every Claude Code on this computer, terminals included.
    let sign_out = move |_| {
        if !confirm_out.get_untracked() {
            confirm_out.set(true);
            return;
        }
        confirm_out.set(false);
        busy.set(true);
        note.set(None);
        let cwd = session.meta.with_untracked(|m| m.cwd.clone());
        spawn_local(async move {
            note.set(Some(match ipc::claude_auth_logout(&cwd).await {
                Ok(o) if o.code == 0 => t!("Account.SignedOut"),
                Ok(o) => t!("Session.ActionFailed", format!("{}{}", o.stderr, o.stdout).trim()),
                Err(e) => t!("Session.ActionFailed", e.trim_end()),
            }));
            busy.set(false);
            refresh();
        });
    };

    let rows = move || match session.heavy.with(|h| h.status_rows.clone()) {
        Some(sections) => sections
            .into_iter()
            .map(|sec| {
                let rows = sec
                    .rows
                    .into_iter()
                    .map(|r| view! { <dt>{r.label}</dt><dd>{r.value}</dd> })
                    .collect_view();
                view! { <dl class="kv" aria-label=sec.title>{rows}</dl> }
            })
            .collect_view()
            .into_any(),
        None => {
            view! { <p>{t!(if has_process.get() { "Status.Loading" } else { "Status.NoProcess" })}</p> }.into_any()
        }
    };

    view! {
        <div class="section">
            <span class="cap" id="st-cap">{t!("Status.Title")}</span>
            {rows}
            <Show when=move || has_process.get()>
                <div class="acts">
                    <button type="button" class="btn" disabled=busy on:click=move |_| refresh()><Icon name="refresh" sm=true />{t!("Wt.Refresh")}</button>
                    <button type="button" class="btn ghost" disabled=busy on:click=sign_out>
                        {move || t!(if confirm_out.get() { "Account.SignOutConfirm" } else { "Account.SignOut" })}
                    </button>
                </div>
                <SignIn session=session />
            </Show>
            {move || note.get().map(|n| view! { <p role="status">{n}</p> })}
        </div>
    }
}
