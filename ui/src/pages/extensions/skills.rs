//! Port of `Components/Pages/Extensions.razor (Skills tab)` — spec 06 §6.7. Owner: ui-extensions.

use super::{describe, find_box, matches, split_source, stop_of, tf, ti, use_ext, Ext};
use crate::state::SessionHandle;
use ccui_shared::t;
use leptos::prelude::*;

/// One expandable row (Skills: `k:`, Agents: `a:`): click / Enter / Space opens its whole description.
pub(super) fn expand_row(ext: Ext, stop: Memo<Option<String>>, key: String, exp: String, cells: AnyView) -> impl IntoView {
    let (k1, k2) = (key.clone(), key);
    let ex = StoredValue::new(exp.clone());
    let open = move || ext.expanded.with(|e| e.contains(&*ex.read_value()));
    let (e1, e2) = (exp.clone(), exp);
    view! {
        <tr data-r="" tabindex=move || ti(stop.with(|s| s.as_deref() == Some(k1.as_str()))) class=move || open().then_some("open")
            aria-expanded=move || tf(open()) on:focusin=move |_| ext.cursor.set(Some(k2.clone()))
            on:click=move |_| ext.expand(e1.clone())
            on:keydown=move |e| if e.key() == "Enter" || e.key() == " " { ext.expand(e2.clone()) }>{cells}</tr>
    }
}

/// Skills table (init or reload_skills) with FindBox.
#[component]
pub fn SkillsTab(session: SessionHandle) -> impl IntoView {
    let ext = use_ext();
    let skills = Memo::new(move |_| ext.skills(session));
    let all_n = Signal::derive(move || skills.with(|s| s.as_ref().map_or(0, Vec::len)));
    let rows = Memo::new(move |_| {
        let q = ext.query.get();
        let list = skills.get().unwrap_or_default();
        session.heavy.with(|h| {
            list.into_iter()
                .map(|k| {
                    let d = describe(h, &k);
                    (k, d)
                })
                .filter(|(k, d)| matches(&q, k, d.as_deref()))
                .collect::<Vec<_>>()
        })
    });
    let shown_n = Signal::derive(move || rows.with(Vec::len));
    let stop = Memo::new(move |_| {
        let keys: Vec<String> = rows.with(|r| r.iter().map(|x| x.0.clone()).collect());
        ext.cursor.with(|c| stop_of(c.as_deref(), &keys))
    });
    let row = move |(name, d): (String, Option<String>)| {
        let (text, origin) = split_source(d.as_deref());
        let tag = origin.filter(|_| !name.contains(':')).map(|o| view! { <span class="tag">{o}</span> });
        let cells = view! {
            <td><div class="n id">{name.clone()}</div>{tag}</td><td><span class="last prose clamp">{text.unwrap_or_else(|| "—".into())}</span></td>
        }
        .into_any();
        expand_row(ext, stop, name.clone(), format!("k:{name}"), cells)
    };
    view! {
        <Show when=move || skills.with(Option::is_some) fallback=|| view! { <p class="lead" role="status">{t!("Ext.Plugins.Loading")}</p> }>
            {find_box(ext, shown_n, all_n)}
            <Show when=move || shown_n.get() != 0 fallback=|| view! { <p class="lead">{t!("Ext.FindEmpty")}</p> }>
                <table class="mcp" aria-label=t!("Ext.Tab.Skills")>
                    <thead><tr><th>{t!("Ext.Col.Skill")}</th><th>{t!("Ext.Col.Description")}</th></tr></thead>
                    <tbody><For each=move || rows.get() key=|r| format!("{r:?}") children=row/></tbody>
                </table>
                <p class="keys">{t!("Ext.KeysExpand")}</p>
            </Show>
        </Show>
    }
}
