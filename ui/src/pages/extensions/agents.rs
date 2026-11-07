//! Port of `Components/Pages/Extensions.razor (Agents tab)` — spec 06 §6.7. Owner: ui-extensions.

use super::skills::expand_row;
use super::{agents, find_box, matches, stop_of, use_ext};
use crate::state::SessionHandle;
use ccui_shared::t;
use leptos::prelude::*;

/// Agents (initialize_info.agents or init.agents) with FindBox.
#[component]
pub fn AgentsTab(session: SessionHandle) -> impl IntoView {
    let ext = use_ext();
    let all = Memo::new(move |_| session.heavy.with(agents));
    let all_n = Signal::derive(move || all.with(Vec::len));
    let rows = Memo::new(move |_| {
        let q = ext.query.get();
        all.with(|v| v.iter().filter(|a| matches(&q, &a.0, a.1.as_deref())).cloned().collect::<Vec<_>>())
    });
    let shown_n = Signal::derive(move || rows.with(Vec::len));
    let stop = Memo::new(move |_| {
        let keys: Vec<String> = rows.with(|r| r.iter().map(|x| x.0.clone()).collect());
        ext.cursor.with(|c| stop_of(c.as_deref(), &keys))
    });
    let row = move |(name, desc, model): (String, Option<String>, Option<String>)| {
        let cells = view! {
            <td><div class="n id">{name.clone()}</div></td><td><span class="last prose clamp">{desc.unwrap_or_else(|| "—".into())}</span></td>
            <td><span class="tag">{model.unwrap_or_else(|| "—".into())}</span></td>
        }
        .into_any();
        expand_row(ext, stop, name.clone(), format!("a:{name}"), cells)
    };
    view! {
        {find_box(ext, shown_n, all_n)}
        <Show when=move || shown_n.get() != 0 fallback=|| view! { <p class="lead">{t!("Ext.FindEmpty")}</p> }>
            <table class="mcp" aria-label=t!("Ext.Tab.Agents")>
                <thead><tr><th>{t!("Agent.Title")}</th><th>{t!("Ext.Col.Description")}</th><th>{t!("Model")}</th></tr></thead>
                <tbody><For each=move || rows.get() key=|r| format!("{r:?}") children=row/></tbody>
            </table>
            <p class="keys">{t!("Ext.KeysExpand")}</p>
        </Show>
    }
}
