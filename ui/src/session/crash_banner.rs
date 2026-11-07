//! Port of `Components/Session/CrashBanner.razor` — spec 07 §3. Owner: ui-session-thread-composer.

use crate::common::duck::{Duck, Mood};
use crate::common::icon::Icon;
use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::model::SessionStatus;
use ccui_shared::t;
use leptos::prelude::*;

/// Crashed / start failed banner with exit text and Restart.
#[component]
pub fn CrashBanner(session: SessionHandle) -> impl IntoView {
    let restarting = RwSignal::new(false);
    let crashed = Memo::new(move |_| session.meta.with(|m| m.status == SessionStatus::Crashed));
    let failed = move || session.meta.with(|m| m.start_failed);
    let exit_text = move || session.meta.with(|m| m.exit_text.clone().unwrap_or_default());
    let restart = move |_| {
        restarting.set(true);
        let id = session.id();
        leptos::task::spawn_local(async move {
            // The session shows Crashed with the reason: the error only goes to the log.
            if let Err(e) = ipc::session_restart(&id).await {
                leptos::logging::error!("[{id}] relance : {e}");
            }
            let _ = restarting.try_set(false);
        });
    };
    view! {
        <Show when=move || crashed.get()>
            <div class="crash" role="alert">
                <div class="duck" aria-hidden="true"><Duck mood=Mood::Ko size=72/></div>
                <div>
                    <h2>{move || t!(if failed() { "Crash.StartFailed.Title" } else { "Crash.Title" })}</h2>
                    <p>{move || t!(if failed() { "Crash.StartFailed.Text" } else { "Crash.Text" })}</p>
                    <pre>{exit_text}</pre>
                    <div class="acts">
                        <button class="btn primary" disabled=move || restarting.get()
                            aria-busy=move || restarting.get().then_some("true") on:click=restart>
                            {move || if restarting.get() {
                                view! { <span class="spin" aria-hidden="true"></span> }.into_any()
                            } else {
                                view! { <Icon name="refresh" sm=true/> }.into_any()
                            }}
                            {move || t!(if session.meta.with(|m| m.crashed_mid_turn) { "Crash.RestartContinue" } else { "Crash.Restart" })}
                        </button>
                        <button class="btn" data-copy=exit_text>{t!("Error.Copy")}</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}
