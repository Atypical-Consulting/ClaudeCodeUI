//! Port of `Components/Pages/Extensions.razor` (page shell: session selector, tabs, query `tab` / `server` /
//! `session`, inspector) — spec 06 §6.7. Owner: ui-extensions. One file per tab beside it.
//! The selected live session is loaded with `AppStore::load_live` (never opens a transcript; `release` on change /
//! cleanup); the "Past" session is looked up with `ipc::transcript_by_id`; MCP sign-in uses
//! `ipc::session_mcp_authenticate`.

pub mod agents;
pub mod hooks;
pub mod mcp;
pub mod permissions;
pub mod plugins;
pub mod skills;

use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::t;
use leptos::prelude::*;

/// Tabs, in display order (query `tab`; default mcp).
pub const TABS: [&str; 6] = ["mcp", "skills", "agents", "hooks", "permissions", "plugins"];

/// The page: picks the live session (`?session=` else the current one, URL replaced with it), shows one tab;
/// armed confirms auto-disarm after 5 s.
#[component]
pub fn Extensions() -> impl IntoView {
    // port: Extensions.razor page shell (placeholder root only)
    view! { <div class="pane"></div> }
}

/// `SignsInHere(m)`: transport != "claudeai-proxy" && tag != "claude.ai".
pub fn signs_in_here(transport: Option<&str>, tag: &str) -> bool {
    todo!("port: Extensions.SignsInHere")
}

/// `Split(name)`: `plugin:P:rest` → (rest, "plugin:P", "plugin"); `claude.ai X` → (X, "claude.ai", "claude.ai");
/// else (name, scope label, source tag).
pub fn split_server(name: &str, scope: Option<&str>, source: Option<&str>) -> (String, String, String) {
    todo!("port: Extensions.Split")
}
