//! Port of `Components/App.razor` + `Routes.razor` — spec 06 §1. Owner: ui-shell.
//! Routes (all inside MainLayout): `/` NewSession (?cwd), `/overview`, `/session/:id`, `/settings/appearance`,
//! `/background`, `/extensions` (?tab&server&session), `/worktrees` (?path), fallback NotFound.
//! After every navigation focus the page's h1 (`interop::focus_h1`).

use crate::layout::main_layout::MainLayout;
use crate::pages::{
    appearance::Appearance, background::Background, extensions::Extensions, new_session::NewSession,
    not_found::NotFound, overview::Overview, session_page::SessionPage, worktrees::Worktrees,
};
use ccui_shared::api::BootInfo;
use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

#[component]
pub fn App(boot: BootInfo) -> impl IntoView {
    crate::state::provide_stores(boot);
    view! {
        <Router>
            <MainLayout>
                <Routes fallback=|| view! { <NotFound/> }>
                    <Route path=path!("/") view=NewSession/>
                    <Route path=path!("/overview") view=Overview/>
                    <Route path=path!("/session/:id") view=SessionPage/>
                    <Route path=path!("/settings/appearance") view=Appearance/>
                    <Route path=path!("/background") view=Background/>
                    <Route path=path!("/extensions") view=Extensions/>
                    <Route path=path!("/worktrees") view=Worktrees/>
                </Routes>
            </MainLayout>
        </Router>
    }
}
