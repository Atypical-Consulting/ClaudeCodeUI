//! Port of `Components/Session/ApiErrorCard.razor` — spec 07 §2, 02 §13. Owner: ui-session-thread-composer.

use crate::common::duck::{Duck, Mood};
use crate::session::sign_in::SignIn;
use crate::state::SessionHandle;
use ccui_shared::api_errors::ApiErrorKind;
use ccui_shared::t;
use leptos::prelude::*;

/// Synthetic API error: title, hint, raw (copyable), retry when the session can.
/// A failed API turn: same look as the crash card (.crash), but the session is still alive, so no restart.
#[component]
pub fn ApiErrorCard(
    error: ccui_shared::api_errors::ApiError,
    #[prop(optional)] session: Option<SessionHandle>,
) -> impl IntoView {
    let auth = error.kind == ApiErrorKind::Auth;
    // An auth error offers to sign in again through the session, while it still has a process.
    // A memo: the meta is replaced on every patch, and SignIn keeps its own state (link, busy) while it waits.
    let live = Memo::new(move |_| auth && session.is_some_and(|s| s.meta.with(|m| m.has_process)));
    let sign_in = move || live.get().then(|| session.map(|s| view! { <SignIn session=s primary=true/> })).flatten();
    view! {
        <div class="crash" role="alert">
            <div class="duck" aria-hidden="true"><Duck mood=Mood::Sad size=56/></div>
            <div>
                <h2>{error.title()}</h2>
                <p>{error.hint()}</p>
                <pre>{error.raw.clone()}</pre>
                <div class="acts">
                    <button type="button" class="btn" data-copy=error.raw.clone()>{t!("Error.Copy")}</button>
                </div>
                {sign_in}
            </div>
        </div>
    }
}
