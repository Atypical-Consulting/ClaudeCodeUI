//! Port of `Components/Pages/Extensions.razor (Permissions tab)` — spec 06 §6.7, 02 §9. Owner: ui-extensions.

use super::{filter_chip, slash, source_tag, stop_of, ti, use_ext};
use crate::common::icon::Icon;
use crate::state::SessionHandle;
use ccui_shared::permission_rules::{visible, PermissionRule, BEHAVIORS, FILES};
use ccui_shared::{fmt, t};
use leptos::prelude::*;
use leptos::task::spawn_local;

pub fn rule_key(r: &PermissionRule) -> String {
    format!("r:{}|{}|{}", r.source, r.behavior, r.rule)
}

/// Rules (session_permission_rules), add/remove via permission_rule_edit, post-write poll.
#[component]
pub fn PermissionsTab(session: SessionHandle) -> impl IntoView {
    let ext = use_ext();
    let home = ext.home.get_value();
    let busy = move || ext.busy.get();
    let all = Memo::new(move |_| ext.perms.with(|p| p.as_ref().map(|p| p.rules.clone()).unwrap_or_default()));
    let count = move |b: &str| all.with(|v| v.iter().filter(|r| r.behavior == b).count());
    let shown = Memo::new(move |_| {
        let f = ext.filter.get();
        all.with(|v| v.iter().filter(|r| f.is_empty() || r.behavior == f).cloned().collect::<Vec<_>>())
    });
    let stop = Memo::new(move |_| {
        let keys: Vec<String> = shown.with(|v| v.iter().map(rule_key).collect());
        ext.cursor.with(|c| stop_of(c.as_deref(), &keys))
    });

    let row = move |r: PermissionRule| {
        let rk = rule_key(&r);
        let rt = {
            let rk = rk.clone();
            move || ti(stop.with(|s| s.as_deref() == Some(rk.as_str())))
        };
        let k2 = rk.clone();
        let remove = r.editable().then(|| {
            let armed = {
                let rk = rk.clone();
                move || ext.confirm.with(|c| c.as_deref() == Some(rk.as_str()))
            };
            let (a2, a3) = (armed.clone(), armed.clone());
            let rule = r.rule.clone();
            let label = move || if a2() { t!("Ext.ConfirmRemoveOf", rule) } else { t!("Ext.Perm.RemoveRule", rule) };
            let r = r.clone();
            let rt = rt.clone();
            view! {
                <button class=move || if armed() { "btn arm danger" } else { "btn arm ghost" } tabindex=rt disabled=busy aria-label=label
                    on:blur=move |_| ext.disarm()
                    on:click=move |_| {
                        let r = r.clone();
                        ext.armed(rk.clone(), move || spawn_local(ext.edit_rule(session, r.source, r.behavior, r.rule, false)))
                    }>{move || if a3() { t!("Ext.ConfirmRemove") } else { t!("Ext.Perm.Remove") }}</button>
            }
        });
        view! {
            <tr data-r="" tabindex=rt on:focusin=move |_| ext.cursor.set(Some(k2.clone()))>
                <td><div class="n"><code>{visible(&r.rule)}</code></div>
                    {r.text.clone().map(|x| view! { <div class="d">{x.prefix}<b>{visible(&x.emphasis)}</b>{x.suffix}</div> })}</td>
                <td><span class="tag">{t!(&format!("Ext.Perm.{}", r.behavior))}</span></td>
                <td><span class="tag">{source_tag(&r.source)}</span>
                    {r.not_in_effect.then(|| view! { <div class="d">{t!("Ext.Perm.NotInEffect")}</div> })}</td>
                <td class="num rowacts">{remove}</td>
            </tr>
        }
    };

    let add = move || {
        let rule = ext.rule_draft.get_untracked().trim().to_string();
        spawn_local(ext.edit_rule(session, ext.rule_file.get_untracked(), ext.rule_behavior.get_untracked(), rule, true));
    };
    let lead = move || {
        let cwd = ext.cwd(session);
        t!("Ext.Perm.Lead", format!("<code title=\"{}\">{}</code>", fmt::enc(&slash(&cwd)), fmt::enc(&fmt::home(&cwd, &home))))
    };
    let home2 = ext.home.get_value();
    view! {
        <p class="lead" inner_html=lead></p>
        {move || ext.perms_note.get().map(|n| view! { <div class="errbox" role="alert">{n}</div> })}
        <Show when=move || ext.perms.with(|p| p.as_ref().is_some_and(|p| p.managed_only))>
            <div class="errbox" role="status">{t!("Ext.Perm.ManagedOnly")}</div>
        </Show>
        {move || ext.perms.with(|p| p.as_ref().map(|p| p.errors.clone()).unwrap_or_default())
            .into_iter()
            .map(|e| view! { <div class="errbox" role="status">{t!("Ext.Perm.FileError", e)}</div> })
            .collect_view()}
        <Show when=move || ext.perms.with(Option::is_some)>
            <div class="filters">
                {filter_chip(ext, "", t!("Filter.All"), Signal::derive(move || all.with(Vec::len)))}
                {BEHAVIORS.into_iter().map(|b| view! {
                    <Show when=move || count(b) != 0 || ext.filter.with(|f| f == b)>
                        {filter_chip(ext, b, t!(&format!("Ext.Perm.{b}")), Signal::derive(move || count(b)))}
                    </Show>
                }).collect_view()}
            </div>
            <Show when=move || all.with(|v| !v.is_empty()) fallback=|| view! { <p class="lead">{t!("Ext.Perm.None")}</p> }>
                <table class="mcp" aria-label=t!("Ext.Tab.Permissions")>
                    <thead><tr><th>{t!("Ext.Col.Rule")}</th><th>{t!("Ext.Col.Behavior")}</th><th>{t!("Ext.Col.Source")}</th>
                        <th class="num"><span class="vh">{t!("Ext.Col.Actions")}</span></th></tr></thead>
                    <tbody><For each=move || shown.get() key=|r| format!("{r:?}") children=row/></tbody>
                </table>
                <p class="keys">{t!("Ext.KeysRows")}</p>
            </Show>
            <div class="field">
                <label for="perm-rule">{t!("Ext.Perm.Add")}</label>
                <div class="input"><input id="perm-rule" prop:value=move || ext.rule_draft.get() on:input=move |e| ext.rule_draft.set(event_target_value(&e))
                    on:keydown=move |e| if e.key() == "Enter" { add() } placeholder="Bash(npm run test:*)" spellcheck="false" autocomplete="off"
                    disabled=busy/></div>
            </div>
            <div class="picks">
                <select aria-label=t!("Ext.Col.Behavior") disabled=busy on:change=move |e| ext.rule_behavior.set(event_target_value(&e))>
                    {BEHAVIORS.into_iter().map(|b| view! {
                        <option value=b selected=move || ext.rule_behavior.with(|x| x == b)>{t!(&format!("Ext.Perm.{b}"))}</option>
                    }).collect_view()}
                </select>
                <select aria-label=t!("Perm.SaveIn") disabled=busy on:change=move |e| ext.rule_file.set(event_target_value(&e))>
                    {FILES.into_iter().map(|src| view! {
                        <option value=src selected=move || ext.rule_file.with(|f| f == src)>{t!(&format!("Perm.Dest.{src}"))}</option>
                    }).collect_view()}
                </select>
                <button class="btn primary" disabled=move || busy() || ext.rule_draft.with(|d| d.trim().is_empty()) on:click=move |_| add()>
                    <Icon name="plus" sm=true/>{t!("Ext.Perm.AddButton")}</button>
            </div>
            {
                let home = home2.clone();
                move || {
                    let dirs = ext.perms.with(|p| p.as_ref().map(|p| p.workspace_dirs.clone()).unwrap_or_default());
                    (!dirs.is_empty()).then(|| {
                        let home = home.clone();
                        view! {
                            <div><span class="cap">{t!("Dirs.Label")}</span>
                                <ul class="dirs">
                                    {dirs.into_iter().map(|d| view! {
                                        <li class="chip" title=slash(&d.path)><Icon name="folder" sm=true/>{fmt::home(&d.path, &home)}" "
                                            <span class="tag">{source_tag(&d.source)}</span></li>
                                    }).collect_view()}
                                </ul>
                            </div>
                        }
                    })
                }
            }
        </Show>
    }
}
