//! Port of `Components/Pages/Extensions.razor (MCP tab)` — spec 06 §6.7. Owner: ui-extensions.

use super::{filter_chip, row_act, servers, state_view, tf, ti, use_ext, RowAct, Srv};
use crate::common::icon::Icon;
use crate::state::SessionHandle;
use ccui_shared::t;
use leptos::prelude::*;

const FILTERS: [(&str, &str); 4] = [
    ("connected", "Ext.Filter.Connected"),
    ("needs-auth", "Ext.Filter.Auth"),
    ("failed", "Ext.Filter.Failed"),
    ("disabled", "Ext.Filter.Disabled"),
];

/// `failed` → its tool count or 0; `connected` → its tool count or —; else —.
pub fn tools_cell(m: &Srv) -> String {
    let n = m.tools.as_ref().map(Vec::len);
    match m.status.as_str() {
        "failed" => n.unwrap_or(0).to_string(),
        "connected" => n.map_or_else(|| "—".to_string(), |n| n.to_string()),
        _ => "—".to_string(),
    }
}

/// Servers (mcp_status / init), toggle, reconnect (all), sign in (auth poll 2 s × 150), tools inspector.
#[component]
pub fn McpTab(session: SessionHandle) -> impl IntoView {
    let ext = use_ext();
    let all = Memo::new(move |_| session.heavy.with(servers));
    let count = move |st: &str| all.with(|v| v.iter().filter(|x| x.status == st).count());
    let shown = Memo::new(move |_| {
        let f = ext.filter.get();
        all.with(|v| v.iter().filter(|x| f.is_empty() || x.status == f).cloned().collect::<Vec<_>>())
    });
    let stop = Memo::new(move |_| {
        let sel = ext.selected.get();
        shown.with(|v| v.iter().find(|x| Some(&x.name) == sel.as_ref()).or(v.first()).map(|x| x.name.clone()))
    });
    let busy = move || ext.busy.get();

    let row = move |first: Srv| {
        let name = first.name.clone();
        let m = {
            let name = name.clone();
            Memo::new(move |prev: Option<&Srv>| {
                shown.with(|v| v.iter().find(|x| x.name == name).cloned()).or_else(|| prev.cloned()).unwrap_or_else(|| first.clone())
            })
        };
        let n = StoredValue::new(name);
        let is_sel = move || ext.selected.with(|s| s.as_deref() == Some(n.read_value().as_str()));
        let rt = move || ti(stop.with(|s| s.as_deref() == Some(n.read_value().as_str())));
        let act = Memo::new(move |_| m.with(row_act));
        let short = move || m.with(|m| m.short.clone());
        let label = move |key: &str| format!("{}: {}", t!(key), short());
        let actions = move || match act.get() {
            RowAct::Retry => view! {
                <button class="btn" tabindex=rt disabled=busy aria-label=move || label("Ext.Retry")
                    on:click=move |_| ext.reconnect(session, n.get_value())>{t!("Ext.Retry")}</button>
            }
            .into_any(),
            RowAct::SignIn => view! {
                <button class="btn" tabindex=rt disabled=busy aria-label=move || label("Ext.SignIn")
                    on:click=move |_| ext.sign_in(session, n.get_value())>{t!("Ext.SignIn")}</button>
            }
            .into_any(),
            RowAct::Toggle => {
                let on = move || m.with(|m| m.status == "connected");
                view! {
                    <button class=move || if on() { "toggle on" } else { "toggle" } tabindex=rt role="switch" aria-checked=move || tf(on())
                        aria-label=move || label("Ext.EnableHere") title=t!("Ext.EnableHereHint") disabled=busy
                        on:click=move |_| ext.toggle(session, n.get_value(), !on())><span class="sw"></span></button>
                }
                .into_any()
            }
            RowAct::None => ().into_any(),
        };
        view! {
            <tr data-r="" class=move || is_sel().then_some("sel") tabindex=rt aria-selected=move || tf(is_sel())
                on:click=move |_| ext.select(n.get_value())
                on:keydown=move |e| if e.key() == "Enter" || e.key() == " " { ext.select(n.get_value()) }>
                <td><div class="n" title=short>{short}</div><div class="d" title=move || m.with(|m| m.origin.clone())>{move || m.with(|m| m.origin.clone())}</div></td>
                <td>{move || state_view(&m.with(|m| m.status.clone()))}</td>
                <td class="num tools">{move || m.with(tools_cell)}</td>
                <td class="num rowacts" on:click=|e| e.stop_propagation()>{actions}</td>
            </tr>
        }
    };

    view! {
        {move || ext.note.get().map(|n| view! { <div class="errbox" role="alert">{n}</div> })}
        <div class="filters">
            {filter_chip(ext, "", t!("Filter.All"), Signal::derive(move || all.with(Vec::len)))}
            {FILTERS.into_iter().map(|(st, lbl)| view! {
                <Show when=move || count(st) != 0 || ext.filter.with(|f| f == st)>
                    {filter_chip(ext, st, t!(lbl), Signal::derive(move || count(st)))}
                </Show>
            }).collect_view()}
            <Show when=move || !matches!(count("failed"), 0 | 1)>
                <button class="btn" disabled=busy on:click=move |_| ext.reconnect_all(session, all.get_untracked())>
                    <Icon name="refresh" sm=true/>{t!("Ext.RetryAll")}</button>
            </Show>
        </div>
        {move || session.heavy.with(|h| h.init.as_ref().and_then(|i| i.mcp_errors.clone()).unwrap_or_default())
            .into_iter()
            .map(|s| view! { <div class="errbox" role="status">{t!("Ext.Skipped", s.name, s.message)}</div> })
            .collect_view()}
        <Show when=move || all.with(|v| v.iter().any(|x| x.status == "connected" || x.status == "disabled"))>
            <p class="d hint">{t!("Ext.SwitchesHint")}</p>
        </Show>
        <table class="mcp servers" aria-label=t!("Ext.McpTable")>
            <thead><tr><th>{t!("Ext.Col.Server")}</th><th>{t!("Col.State")}</th><th class="num tools" title=t!("Ext.ToolsHint")>{t!("Col.Tools")}</th>
                <th class="num"><span class="vh">{t!("Ext.Col.Actions")}</span></th></tr></thead>
            <tbody>
                <For each=move || shown.get() key=|m| m.name.clone() children=row/>
            </tbody>
        </table>
        <Show when=move || shown.with(Vec::is_empty) fallback=|| view! { <p class="keys">{t!("Ext.KeysMcp")}</p> }>
            <p class="lead">{t!("Ext.FilterEmpty")}</p>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_counts() {
        let s = |st: &str, n: Option<usize>| Srv { status: st.into(), tools: n.map(|n| vec![Default::default(); n]), ..Srv::default() };
        assert_eq!(tools_cell(&s("failed", None)), "0");
        assert_eq!(tools_cell(&s("connected", None)), "—");
        assert_eq!(tools_cell(&s("connected", Some(3))), "3");
        assert_eq!(tools_cell(&s("needs-auth", Some(3))), "—");
    }
}
