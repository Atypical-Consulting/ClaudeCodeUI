//! Port of `Components/Pages/Extensions.razor (Plugins tab)` — spec 06 §6.7, 05 §1. Owner: ui-extensions.

use super::{dot, plugin_state, plural, scope_label, servers, source_tag, state_view, stop_of, tf, ti, use_ext, Ext, PState, Srv};
use crate::common::icon::Icon;
use crate::state::SessionHandle;
use ccui_shared::plugins::{ListedPlugin, Market, PluginRow, SCOPES};
use ccui_shared::t;
use leptos::prelude::*;
use leptos::task::spawn_local;

/// The catalogue holds thousands of entries: the datalist offers the 30 most installed that match what is typed.
pub fn catalog_matches(available: &[ListedPlugin], draft: &str) -> Vec<ListedPlugin> {
    let d = draft.to_lowercase();
    let mut m: Vec<ListedPlugin> = available.iter().filter(|a| d.is_empty() || a.id.to_lowercase().contains(&d)).cloned().collect();
    m.sort_by(|a, b| b.installs.cmp(&a.installs));
    m.truncate(30);
    m
}

/// Skills a plugin brings (`name:` prefix).
pub fn plugin_skills(skills: &[String], name: &str) -> usize {
    let p = format!("{name}:");
    skills.iter().filter(|k| k.starts_with(&p)).count()
}

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

/// The plugin's server, opened on the MCP tab with its inspector.
fn to_server(ext: Ext, m: Srv, state: AnyView, rt: impl Fn() -> &'static str + Send + Sync + 'static) -> AnyView {
    let name = m.name.clone();
    view! {
        <button type="button" class="linkish" tabindex=rt title=t!("Ext.Plugins.ShowServer", m.short)
            on:click=move |_| { ext.show("mcp"); ext.select(name.clone()); }>{state}</button>
    }
    .into_any()
}

fn state_cell(ext: Ext, st: PState, rt: impl Fn() -> &'static str + Send + Sync + 'static) -> AnyView {
    match st {
        PState::Off => state_view("disabled"),
        PState::Failed => dot("fail", t!("Ext.Plugins.State.Failed")),
        PState::ServerFailing(n, m) => to_server(ext, m, dot("fail", plural(n, "Ext.Plugins.State.ServerFailing")), rt),
        PState::NeedsAuth(m) => to_server(ext, m, dot("auth", t!("Ext.State.NeedsAuth")), rt),
        PState::ServersOff(n, m) => to_server(ext, m, dot("off", plural(n, "Ext.Plugins.State.ServersOff")), rt),
        PState::Loaded => dot("ok", t!("Ext.Plugins.State.Loaded")),
        PState::Reload => dot("wait", t!("Ext.Plugins.State.Reload")),
        PState::On => dot("wait", t!("Ext.Plugins.State.On")),
    }
}

/// Installed / catalogue / marketplaces, act (install, enable…, accept-command flow), options inspector,
/// reload the session after a change.
#[component]
pub fn PluginsTab(#[prop(into)] session: Signal<Option<SessionHandle>>, #[prop(into)] cwd: Signal<String>) -> impl IntoView {
    let ext = use_ext();
    let busy = move || ext.busy.get();
    let srv = Memo::new(move |_| session.get().map(|h| h.heavy.with(servers)));
    let skills = Memo::new(move |_| session.get().and_then(|h| ext.skills(h)));
    let init_none = move || session.get().is_some_and(|h| h.heavy.with(|x| x.init.is_none()));
    let act = move |a: Vec<String>| spawn_local(ext.plugin_act(session.get_untracked(), cwd.get_untracked(), a));
    let installed = Memo::new(move |_| ext.installed.get().unwrap_or_default());
    let stop = Memo::new(move |_| {
        let keys: Vec<String> = installed.with(|v| v.iter().map(|x| format!("{}|{}", x.id, x.scope)).collect());
        ext.cursor.with(|c| stop_of(c.as_deref(), &keys))
    });

    let row = move |first: PluginRow| {
        let key = StoredValue::new(format!("{}|{}", first.id, first.scope));
        let p = {
            let k = key.get_value();
            Memo::new(move |prev: Option<&PluginRow>| {
                installed
                    .with(|v| v.iter().find(|x| format!("{}|{}", x.id, x.scope) == k).cloned())
                    .or_else(|| prev.cloned())
                    .unwrap_or_else(|| first.clone())
            })
        };
        let rt = move || ti(stop.with(|s| s.as_deref() == Some(key.read_value().as_str())));
        let name = move || p.with(|p| p.name().to_string());
        let editable = p.with_untracked(|p| matches!(p.scope.as_str(), "user" | "project" | "local"));
        let state = Memo::new(move |_| {
            let s = session.get();
            p.with(|p| {
                srv.with(|sv| {
                    skills.with(|sk| {
                        ext.hooks.with(|hk| {
                            let init = s.and_then(|h| h.heavy.with(|x| x.init.clone()));
                            plugin_state(p, init.as_ref(), sv.as_deref(), sk.as_deref(), hk.as_ref(), s.is_some())
                        })
                    })
                })
            })
        });
        let pact = move |verb: &str| {
            let (id, scope) = p.with_untracked(|p| (p.id.clone(), p.scope.clone()));
            act(vec![verb.to_string(), id, "--scope".into(), scope]);
        };
        let actions = editable.then(|| {
            let on = move || p.with(|p| p.enabled);
            let uk = move || format!("u:{}", key.read_value());
            let armed = move || ext.confirm.with(|c| c.as_deref() == Some(uk().as_str()));
            view! {
                <button class=move || if on() { "toggle on" } else { "toggle" } tabindex=rt role="switch" aria-checked=move || tf(on())
                    aria-label=move || format!("{}: {}", t!("Ext.Plugins.Enable"), name()) disabled=busy
                    on:click=move |_| pact(if on() { "disable" } else { "enable" })><span class="sw"></span></button>
                <Show when=move || p.with(|p| p.has_options)>
                    <button class="btn" tabindex=rt disabled=busy aria-label=move || format!("{}: {}", t!("Ext.Plugins.Options"), name())
                        on:click=move |_| ext.configure(cwd.get_untracked(), p.with_untracked(|p| p.id.clone()))>{t!("Ext.Plugins.Options")}</button>
                </Show>
                <button class="btn ghost" tabindex=rt disabled=busy aria-label=move || format!("{}: {}", t!("Ext.Plugins.Update"), name())
                    on:click=move |_| pact("update")>{t!("Ext.Plugins.Update")}</button>
                <button class=move || if armed() { "btn arm danger" } else { "btn arm ghost" } tabindex=rt disabled=busy
                    aria-label=move || if armed() { t!("Ext.ConfirmUninstallOf", name()) } else { format!("{}: {}", t!("Ext.Plugins.Uninstall"), name()) }
                    on:blur=move |_| ext.disarm()
                    on:click=move |_| {
                        let (id, scope) = p.with_untracked(|p| (p.id.clone(), p.scope.clone()));
                        ext.confirmed(session.get_untracked(), cwd.get_untracked(), uk(), vec!["uninstall".into(), id, "--scope".into(), scope]);
                    }>{move || if armed() { t!("Ext.Plugins.ConfirmUninstall") } else { t!("Ext.Plugins.Uninstall") }}</button>
            }
        });
        view! {
            <tr data-r="" tabindex=rt on:focusin=move |_| ext.cursor.set(Some(key.get_value()))>
                <td>{move || p.with(|p| view! {
                    <div class="n">{p.name().to_string()}</div><div class="d id" title=format!("{} · {}", p.id, p.version)>{format!("{} · {}", p.id, p.version)}</div>
                    {p.errors.iter().map(|e| view! { <div class="errbox">{e.clone()}</div> }).collect_view()}
                    {p.notes.iter().map(|n| view! { <div class="d">{n.clone()}</div> }).collect_view()}
                })}</td>
                <td>{move || state_cell(ext, state.get(), rt)}</td>
                <td class="opt"><span class="tag">{move || p.with(|p| scope_label(&p.scope))}</span></td>
                <td class="num opt">{move || p.with(|p| p.mcp_servers.len())}</td>
                {move || skills.get().map(|sk| view! {
                    <td class="num opt">{p.with(|p| if p.enabled { plugin_skills(&sk, p.name()).to_string() } else { "—".into() })}</td>
                })}
                <td class="num rowacts">{actions}</td>
            </tr>
        }
    };

    let markets = Memo::new(move |_| ext.markets.get().unwrap_or_default());
    let mstop = Memo::new(move |_| {
        let keys: Vec<String> = markets.with(|v| v.iter().map(|m| format!("m:{}", m.name)).collect());
        ext.cursor.with(|c| stop_of(c.as_deref(), &keys))
    });
    let market_row = move |m: Market| {
        let mk = StoredValue::new(format!("m:{}", m.name));
        let name = StoredValue::new(m.name.clone());
        let rt = move || ti(mstop.with(|s| s.as_deref() == Some(mk.read_value().as_str())));
        let armed = move || ext.confirm.with(|c| c.as_deref() == Some(mk.read_value().as_str()));
        view! {
            <tr data-r="" tabindex=rt on:focusin=move |_| ext.cursor.set(Some(mk.get_value()))>
                <td><div class="n">{m.name.clone()}</div></td>
                <td><code>{m.location.clone()}</code></td>
                <td class="num rowacts">
                    <button class="btn ghost" tabindex=rt disabled=busy aria-label=format!("{}: {}", t!("Ext.Plugins.Update"), m.name)
                        on:click=move |_| act(args(&["marketplace", "update", &name.get_value()]))>{t!("Ext.Plugins.Update")}</button>
                    <button class=move || if armed() { "btn arm danger" } else { "btn arm ghost" } tabindex=rt disabled=busy
                        aria-label=move || if armed() { t!("Ext.ConfirmRemoveOf", name.get_value()) } else { format!("{}: {}", t!("Ext.Plugins.Remove"), name.get_value()) }
                        on:blur=move |_| ext.disarm()
                        on:click=move |_| ext.confirmed(session.get_untracked(), cwd.get_untracked(), mk.get_value(), args(&["marketplace", "remove", &name.get_value()]))>
                        {move || if armed() { t!("Ext.Plugins.ConfirmRemove") } else { t!("Ext.Plugins.Remove") }}</button>
                </td>
            </tr>
        }
    };

    let install = move || {
        let d = ext.install_draft.get_untracked().trim().to_string();
        if !d.is_empty() {
            act(vec!["install".into(), d, "--scope".into(), ext.install_scope.get_untracked()]);
        }
    };
    let add_market = move || {
        let d = ext.market_draft.get_untracked().trim().to_string();
        if d.is_empty() {
            return;
        }
        spawn_local(async move {
            ext.plugin_act(session.get_untracked(), cwd.get_untracked(), vec!["marketplace".into(), "add".into(), d]).await;
            if !ext.market_draft.is_disposed() && ext.plugins_note.get_untracked().is_none() {
                ext.market_draft.set(String::new());
            }
        });
    };

    view! {
        {move || session.get().map(|h| {
            let id = h.id();
            view! {
                <div class="filters">
                    <button class="btn" disabled=move || busy() || !ext.has_process(&id) on:click=move |_| ext.reload_now(h)>
                        <Icon name="refresh" sm=true/>{t!("Ext.Plugins.Reload")}</button>
                </div>
            }
        })}
        <div role="status">{move || ext.plugins_done.get().map(|d| view! { <p class="lead">{d}</p> })}</div>
        {move || ext.plugins_note.get().map(|n| view! { <div class="errbox" role="alert">{n}</div> })}
        {move || session.get().and_then(|h| h.heavy.with(|x| x.init.as_ref().and_then(|i| i.plugin_errors.clone()))).unwrap_or_default()
            .into_iter()
            .map(|pe| view! { <div class="errbox" role="status" title=pe.path.clone()>{t!("Ext.Plugins.LoadError", pe.plugin, pe.message)}</div> })
            .collect_view()}
        {move || ext.to_accept.get().map(|(o, a)| view! {
            <div class="errbox" role="alert">{t!("Ext.Plugins.Command", a.get(1).cloned().unwrap_or_default())}<br/><code>{o.command.clone()}</code></div>
            <div class="acts">
                <button class="btn primary" disabled=busy on:click=move |_| ext.accept(session.get_untracked(), cwd.get_untracked())>{t!("Ext.Plugins.Accept")}</button>
                <button class="btn" on:click=move |_| ext.to_accept.set(None)>{t!("Action.Cancel")}</button>
            </div>
        })}
        <Show when=move || ext.installed.with(Option::is_some) fallback=|| view! { <p class="lead" role="status">{t!("Ext.Plugins.Loading")}</p> }>
            <Show when=move || installed.with(|v| !v.is_empty()) fallback=|| view! { <p class="lead">{t!("Ext.Plugins.None")}</p> }>
                <Show when=init_none><p class="lead">{t!("Ext.Plugins.Lifecycle")}</p></Show>
                <table class="mcp plugins" aria-label=t!("Ext.Tab.Plugins")>
                    <thead><tr><th>{t!("Ext.Col.Plugin")}</th><th>{t!("Col.State")}</th><th class="opt">{t!("Ext.Col.Scope")}</th>
                        <th class="num opt">{t!("Ext.Col.McpServers")}</th>
                        <Show when=move || skills.with(Option::is_some)><th class="num opt">{t!("Ext.Tab.Skills")}</th></Show>
                        <th class="num"><span class="vh">{t!("Ext.Col.Actions")}</span></th></tr></thead>
                    <tbody><For each=move || installed.get() key=|p| format!("{}|{}", p.id, p.scope) children=row/></tbody>
                </table>
                <p class="keys">{t!("Ext.KeysRows")}</p>
            </Show>
        </Show>
        <div class="field">
            <label for="plugin-install">{t!("Ext.Plugins.Install")}</label>
            <div class="input"><input id="plugin-install" list="plugin-catalog" prop:value=move || ext.install_draft.get()
                on:input=move |e| ext.install_draft.set(event_target_value(&e)) placeholder="name@marketplace" spellcheck="false" autocomplete="off"
                disabled=busy on:keydown=move |e| if e.key() == "Enter" { install() }/></div>
            <datalist id="plugin-catalog">
                {move || ext.available.with(|a| ext.install_draft.with(|d| catalog_matches(a, d))).into_iter().map(|a| view! {
                    <option value=a.id.clone()>{a.description.clone()}</option>
                }).collect_view()}
            </datalist>
        </div>
        <div class="picks">
            <select aria-label=t!("Ext.Col.Scope") disabled=busy on:change=move |e| ext.install_scope.set(event_target_value(&e))>
                {SCOPES.into_iter().map(|sc| view! {
                    <option value=sc selected=move || ext.install_scope.with(|s| s == sc)>{source_tag(&format!("{sc}Settings"))}</option>
                }).collect_view()}
            </select>
            <button class="btn primary" disabled=move || busy() || ext.install_draft.with(|d| d.trim().is_empty()) on:click=move |_| install()>
                <Icon name="plus" sm=true/>{t!("Ext.Plugins.InstallButton")}</button>
        </div>
        <h3>{t!("Ext.Plugins.Markets")}</h3>
        <Show when=move || markets.with(|m| !m.is_empty())>
            <table class="mcp" aria-label=t!("Ext.Plugins.Markets")>
                <thead><tr><th>{t!("Ext.Plugins.Market")}</th><th>{t!("Ext.Col.Source")}</th><th class="num"><span class="vh">{t!("Ext.Col.Actions")}</span></th></tr></thead>
                <tbody><For each=move || markets.get() key=|m| format!("{m:?}") children=market_row/></tbody>
            </table>
            <p class="keys">{t!("Ext.KeysRows")}</p>
        </Show>
        <div class="field">
            <label for="market-add">{t!("Ext.Plugins.AddMarket")}</label>
            <div class="input"><input id="market-add" prop:value=move || ext.market_draft.get() on:input=move |e| ext.market_draft.set(event_target_value(&e))
                placeholder="owner/repo" spellcheck="false" autocomplete="off" disabled=busy
                on:keydown=move |e| if e.key() == "Enter" { add_market() }/></div>
        </div>
        <div class="picks">
            <button class="btn primary" disabled=move || busy() || ext.market_draft.with(|d| d.trim().is_empty()) on:click=move |_| add_market()>
                <Icon name="plus" sm=true/>{t!("Ext.Plugins.AddMarketButton")}</button>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_top_30() {
        let a: Vec<ListedPlugin> =
            (0..40).map(|i| ListedPlugin { id: format!("p{i}@mk"), installs: i, ..ListedPlugin::default() }).collect();
        let m = catalog_matches(&a, "");
        assert_eq!((m.len(), m[0].id.as_str()), (30, "p39@mk"));
        let m = catalog_matches(&a, "P3");
        assert_eq!(m.iter().map(|x| x.id.as_str()).collect::<Vec<_>>()[..2], ["p39@mk", "p38@mk"]);
        assert_eq!(m.len(), 11);
    }

    #[test]
    fn skills_of_a_plugin() {
        let k: Vec<String> = ["gh:a", "gh:b", "ghx:c", "gh"].map(String::from).to_vec();
        assert_eq!(plugin_skills(&k, "gh"), 2);
    }
}
