//! Port of `Components/Session/SignIn.razor` — spec 07 §8. Owner: ui-session-inspector.
//! /login over stream-json: claude_authenticate hands out the sign-in page and the CLI receives its callback;
//! FinishSignIn returns once the browser is done.

use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::t;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// Claude.ai sign-in flow (session_start_sign_in → link → session_finish_sign_in).
#[component]
pub fn SignIn(
    session: SessionHandle,
    #[prop(optional)] primary: bool,
) -> impl IntoView {
    let busy = RwSignal::new(false);
    let url = RwSignal::new(None::<String>);
    let note = RwSignal::new(None::<String>);
    let start = move |_| {
        busy.set(true);
        note.set(None);
        let id = session.id();
        spawn_local(async move {
            let res = async {
                let u = ipc::session_start_sign_in(&id).await?.ok_or_else(|| t!("Account.NoUrl"))?;
                url.set(Some(u));
                ipc::session_finish_sign_in(&id).await
            }
            .await;
            note.set(Some(match res {
                Ok(()) => t!("Account.SignedIn"),
                Err(e) => t!("Session.ActionFailed", e.trim_end()),
            }));
            url.set(None);
            busy.set(false);
        });
    };
    let class = if primary { "btn primary" } else { "btn" };
    view! {
        {move || match url.get() {
            Some(u) => view! {
                <p role="status">{t!("Account.Waiting")}</p>
                <div class="acts"><a class="btn primary" href=u target="_blank" rel="noopener noreferrer">{t!("Ext.OpenSignIn")}</a></div>
            }
            .into_any(),
            None => view! {
                <div class="acts">
                    <button type="button" class=class disabled=busy aria-busy=move || busy.get().then_some("true") on:click=start>
                        {t!("Account.SignIn")}
                    </button>
                </div>
            }
            .into_any(),
        }}
        {move || note.get().map(|n| view! { <p role="status">{n}</p> })}
    }
}
