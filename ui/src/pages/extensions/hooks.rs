//! Port of `Components/Pages/Extensions.razor (Hooks tab)` — spec 06 §6.7, 04 §8. Owner: ui-extensions.

use super::{breaks, find_box, hook_key, hook_name, matches, slash, source_tag, stop_of, ti, use_ext};
use crate::common::icon::Icon;
use crate::state::SessionHandle;
use ccui_shared::hooks::HookRow;
use ccui_shared::permission_rules::FILES;
use ccui_shared::{fmt, t};
use leptos::prelude::*;
use leptos::task::spawn_local;

/// The row can be edited here: it names its stored entry, hooks are not locked, and it lives in a writable file.
pub fn editable(h: &HookRow, locked: bool) -> bool {
    h.edit.is_some() && !locked && FILES.contains(&h.source.as_str())
}

/// `type · "{timeout} s" · display`, empty parts dropped.
pub fn meta(h: &HookRow) -> String {
    [Some(h.kind.clone()), h.timeout.map(|t| format!("{t} s")), h.display.clone()]
        .into_iter()
        .flatten()
        .filter(|x| !x.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// What the Find box matches a hook on: event, matcher (or "any"), plugin or source; and the command.
fn haystack(h: &HookRow) -> String {
    let m = if h.matcher.is_empty() { t!("Ext.Hooks.AnyMatcher") } else { h.matcher.clone() };
    format!("{} {m} {}", h.event, h.plugin_name.clone().unwrap_or_else(|| source_tag(&h.source)))
}

/// Hook listing (session_hooks), add/edit/remove form via cli_edit_hook, post-write poll 500 ms × 20.
#[component]
pub fn HooksTab(session: SessionHandle) -> impl IntoView {
    let ext = use_ext();
    let home = ext.home.get_value();
    let busy = move || ext.busy.get();
    let locked = move || ext.hooks.with(|l| l.as_ref().is_some_and(|l| l.locked));
    let all_n = Signal::derive(move || ext.hooks.with(|l| l.as_ref().map_or(0, |l| l.rows.len())));
    let rows = Memo::new(move |_| {
        let q = ext.query.get();
        ext.hooks.with(|l| {
            l.as_ref().map(|l| l.rows.iter().filter(|h| matches(&q, &haystack(h), Some(&h.command))).cloned().collect::<Vec<_>>())
        })
        .unwrap_or_default()
    });
    let shown_n = Signal::derive(move || rows.with(Vec::len));
    let stop = Memo::new(move |_| {
        let keys: Vec<String> = rows.with(|r| r.iter().map(hook_key).collect());
        ext.cursor.with(|c| stop_of(c.as_deref(), &keys))
    });
    let any_edit = move || rows.with(|r| r.iter().any(|h| editable(h, locked())));

    let row = move |h: HookRow| {
        let key = hook_key(&h);
        let rt = {
            let key = key.clone();
            move || ti(stop.with(|s| s.as_deref() == Some(key.as_str())))
        };
        let k2 = key.clone();
        let h2 = h.clone();
        let editing = move || ext.hook_editing.with(|e| e.as_ref() == Some(&h2));
        let tag_title = h.plugin_name.is_none().then(|| h.source_label.clone()).flatten();
        let cmd = h.command.clone();
        let actions = {
            let h = h.clone();
            let rt = rt.clone();
            move || {
                if editable(&h, locked()) {
                    let hk = format!("h:{}", hook_key(&h));
                    let armed = {
                        let hk = hk.clone();
                        move || ext.confirm.with(|c| c.as_deref() == Some(hk.as_str()))
                    };
                    let (name, cmd) = (hook_name(&h), h.command.clone());
                    let edit = (h.kind == "command").then(|| {
                        let h = h.clone();
                        let rt = rt.clone();
                        view! {
                            <button class="btn ghost" tabindex=rt disabled=busy aria-label=t!("Ext.Hooks.EditRow", name, cmd)
                                on:click=move |_| ext.edit_hook(h.clone())>{t!("Ext.Hooks.Edit")}</button>
                        }
                    });
                    let h = h.clone();
                    let armed2 = armed.clone();
                    let armed3 = armed.clone();
                    let label = move || {
                        if armed2() { t!("Ext.ConfirmRemoveOf", format!("{name} {cmd}")) } else { t!("Ext.Hooks.RemoveRow", name, cmd) }
                    };
                    Some(
                        view! {
                            {edit}
                            <button class=move || if armed() { "btn arm danger" } else { "btn arm ghost" } tabindex=rt.clone() disabled=busy
                                aria-label=label on:blur=move |_| ext.disarm()
                                on:click=move |_| {
                                    let h = h.clone();
                                    ext.armed(hk.clone(), move || spawn_local(ext.save_hook(session, Some(h), true)))
                                }>
                                {move || if armed3() { t!("Ext.ConfirmRemove") } else { t!("Ext.Perm.Remove") }}</button>
                        }
                        .into_any(),
                    )
                } else if h.plugin_name.is_some() {
                    Some(view! { <span class="d">{t!("Ext.Hooks.FromPlugin")}</span> }.into_any())
                } else {
                    None
                }
            }
        };
        view! {
            <tr data-r="" tabindex=rt on:focusin=move |_| ext.cursor.set(Some(k2.clone())) class=move || editing().then_some("sel")>
                <td><div class="n">{h.event.clone()}</div><span class="tag" title=tag_title>{source_tag(&h.source)}</span>
                    {h.plugin_name.clone().map(|p| view! { <span class="d id" title=p.clone()>{p.clone()}</span> })}
                    {h.disabled.then(|| view! { <span class="tag">{t!("Ext.Hooks.RowDisabled")}</span> })}</td>
                <td>{if h.matcher.is_empty() {
                    view! { <span class="d">{t!("Ext.Hooks.AnyMatcher")}</span> }.into_any()
                } else {
                    view! { <code inner_html=breaks(&h.matcher, '|')></code> }.into_any()
                }}</td>
                <td><code class="hookcmd" inner_html=breaks(&cmd, '/')></code><div class="d">{meta(&h)}</div>
                    {h.blocks_on_failure().then(|| view! { <span class="tag" title=t!("Ext.Hooks.BlocksHint")>{t!("Ext.Hooks.Blocks")}</span> })}</td>
                <td class="num rowacts">{actions}</td>
            </tr>
        }
    };

    // The event the form is on (else the catalog's first).
    let ev = Memo::new(move |_| {
        let name = ext.hook_event.get();
        ext.hooks.with(|l| l.as_ref().and_then(|l| l.catalog.iter().find(|x| x.name == name).or(l.catalog.first()).cloned()))
    });
    let ev_matcher = move || ev.with(|e| e.as_ref().is_some_and(|e| e.matcher));
    let save = move || spawn_local(ext.save_hook(session, ext.hook_editing.get_untracked(), false));
    let form = move || {
        view! {
            <div class="field">
                <span class="lbl" id="hook-form-cap">{move || if ext.hook_editing.with(Option::is_none) { t!("Ext.Hooks.AddTitle") } else { t!("Ext.Hooks.EditTitle") }}</span>
                <div class="picks" role="group" aria-labelledby="hook-form-cap">
                    <select aria-label=t!("Ext.Col.Event") disabled=busy title=move || ev.with(|e| e.as_ref().map(|e| e.summary.clone()))
                        on:change=move |e| ext.hook_event.set(event_target_value(&e))>
                        {move || ext.hooks.with(|l| l.as_ref().map(|l| l.catalog.clone()).unwrap_or_default()).into_iter().map(|c| {
                            let n = c.name.clone();
                            view! { <option value=c.name.clone() selected=move || ext.hook_event.with(|e| *e == n)>{c.name.clone()}</option> }
                        }).collect_view()}
                    </select>
                    <div class="input"><input aria-label=t!("Ext.Col.Matcher") prop:value=move || ext.hook_matcher.get()
                        on:input=move |e| ext.hook_matcher.set(event_target_value(&e))
                        placeholder=move || if ev_matcher() { "Bash|Write".to_string() } else { t!("Ext.Hooks.NoMatcher") }
                        disabled=move || busy() || !ev_matcher() spellcheck="false" autocomplete="off"/></div>
                    <Show when=move || ext.hook_editing.with(Option::is_none)>
                        <select aria-label=t!("Perm.SaveIn") disabled=busy on:change=move |e| ext.hook_file.set(event_target_value(&e))>
                            {FILES.into_iter().map(|src| view! {
                                <option value=src selected=move || ext.hook_file.with(|f| f == src)>{t!(&format!("Perm.Dest.{src}"))}</option>
                            }).collect_view()}
                        </select>
                    </Show>
                </div>
                <div class="input"><input aria-label=t!("Ext.Col.Command") prop:value=move || ext.hook_command.get()
                    on:input=move |e| ext.hook_command.set(event_target_value(&e)) placeholder="npm run lint" spellcheck="false"
                    autocomplete="off" disabled=busy on:keydown=move |e| if e.key() == "Enter" { save() }/></div>
                <div class="picks">
                    <div class="input"><input type="number" min="1" aria-label=t!("Ext.Hooks.Timeout") placeholder=t!("Ext.Hooks.Timeout")
                        prop:value=move || ext.hook_timeout.get() on:input=move |e| ext.hook_timeout.set(event_target_value(&e)) disabled=busy/></div>
                    <label class="toggle"><input type="checkbox" prop:checked=move || ext.hook_block.get()
                        on:change=move |e| ext.hook_block.set(event_target_checked(&e)) disabled=busy/>{t!("Ext.Hooks.Block")}</label>
                    <button class="btn primary" disabled=move || busy() || ext.hook_command.with(|c| c.trim().is_empty()) on:click=move |_| save()>
                        {move || if ext.hook_editing.with(Option::is_none) {
                            view! { <Icon name="plus" sm=true/>{t!("Ext.Hooks.AddButton")} }.into_any()
                        } else {
                            view! { <Icon name="check" sm=true/>{t!("Ext.Hooks.SaveButton")} }.into_any()
                        }}</button>
                    <Show when=move || ext.hook_editing.with(Option::is_some)>
                        <button class="btn" on:click=move |_| ext.reset_hook_form()>{t!("Action.Cancel")}</button>
                    </Show>
                </div>
            </div>
        }
    };

    let lead = move || {
        let cwd = ext.cwd(session);
        t!("Ext.Hooks.Lead", format!("<code title=\"{}\">{}</code>", fmt::enc(&slash(&cwd)), fmt::enc(&fmt::home(&cwd, &home))))
    };
    view! {
        <p class="lead" inner_html=lead></p>
        {move || match (ext.hooks_note.get(), ext.hooks.with(|l| l.as_ref().is_some_and(|l| l.disabled))) {
            (Some(n), _) => Some(view! { <div class="errbox" role="alert">{n}</div> }.into_any()),
            (None, true) => Some(view! { <div class="errbox" role="status">{t!("Ext.Hooks.Disabled")}</div> }.into_any()),
            _ => None,
        }}
        <Show when=locked><div class="errbox" role="status">{t!("Ext.Hooks.Locked")}</div></Show>
        <Show when=move || ext.hooks.with(|l| l.as_ref().is_some_and(|l| l.rows.is_empty()))>
            <p class="lead">{t!("Ext.Hooks.None")}</p>
        </Show>
        <Show when=move || ext.hooks.with(|l| l.as_ref().is_some_and(|l| !l.rows.is_empty()))>
            {find_box(ext, shown_n, all_n)}
            <Show when=move || shown_n.get() != 0 fallback=|| view! { <p class="lead">{t!("Ext.FindEmpty")}</p> }>
                <table class=move || if any_edit() { "mcp hooktab" } else { "mcp hooktab ro" } aria-label=t!("Ext.Tab.Hooks")>
                    <thead><tr><th>{t!("Ext.Col.Event")}</th><th>{t!("Ext.Col.Matcher")}</th><th>{t!("Ext.Col.Command")}</th>
                        <th class="num"><span class="vh">{t!("Ext.Col.Actions")}</span></th></tr></thead>
                    <tbody><For each=move || rows.get() key=|h| format!("{h:?}") children=row/></tbody>
                </table>
                <p class="keys">{t!("Ext.KeysRows")}</p>
            </Show>
        </Show>
        <Show when=move || ext.hooks.with(|l| l.as_ref().is_some_and(|l| !l.locked && !l.catalog.is_empty()))>{form}</Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccui_shared::hooks::HookEdit;

    fn row(source: &str, edit: bool) -> HookRow {
        HookRow {
            event: "Stop".into(),
            matcher: String::new(),
            kind: "command".into(),
            command: "npm run lint".into(),
            display: Some("Lint".into()),
            source: source.into(),
            source_label: None,
            plugin_name: None,
            timeout: Some(30),
            disabled: false,
            edit: edit.then(|| HookEdit { matcher: String::new(), config: serde_json::json!({}) }),
        }
    }

    #[test]
    fn editable_rows() {
        assert!(editable(&row("projectSettings", true), false));
        assert!(!editable(&row("projectSettings", true), true), "locked");
        assert!(!editable(&row("policySettings", true), false), "not a writable file");
        assert!(!editable(&row("localSettings", false), false), "no stored entry");
    }

    #[test]
    fn meta_line() {
        assert_eq!(meta(&row("x", false)), "command · 30 s · Lint");
        let mut r = row("x", false);
        r.timeout = None;
        r.display = None;
        assert_eq!(meta(&r), "command");
    }
}
