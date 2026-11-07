//! Port of `Components/Session/ElicitForm.razor` — spec 07 §28, 02 §12. Owner: ui-session-inspector.
//! An MCP elicitation, in place of the Allow/Deny body of the PermissionCard. Form mode: one field per requested_schema
//! property, Submit accepts with the content, Decline declines. URL mode, as the CLI's dialog: the full URL, "Open in
//! browser" (a link: no user gesture survives a round-trip), then "I'm done, continue" accepts; the server's
//! elicitation_complete accepts on its own (backend). ⏎ outside a field submits, Del declines (relayed by the card).

use super::permission_card::on_relayed_key;
use crate::common::icon::Icon;
use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::api::AnswerArgs;
use ccui_shared::elicit::{self, Field, FieldKind};
use ccui_shared::model::*;
use ccui_shared::{json, t};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

/// Initial text per field: booleans "true"/"false" from their default, others their default or "".
pub fn initial(fields: &[Field]) -> HashMap<String, String> {
    fields
        .iter()
        .map(|f| {
            let v = match f.kind {
                FieldKind::Boolean => if f.default.as_deref() == Some("true") { "true" } else { "false" }.to_string(),
                _ => f.default.clone().unwrap_or_default(),
            };
            (f.key.clone(), v)
        })
        .collect()
}

/// `display_name ?? mcp_server_name ?? "MCP"`.
pub fn server(input: &Value) -> String {
    json::str(input, "display_name").or_else(|| json::str(input, "mcp_server_name")).unwrap_or("MCP").to_string()
}

fn num(v: Option<f64>) -> Option<String> {
    v.map(|v| v.to_string())
}

/// MCP elicitation form (`elicit::parse` / `content`) or URL mode link.
#[component]
pub fn ElicitForm(
    session: SessionHandle,
    permission: PendingPermission,
) -> impl IntoView {
    let url = elicit::is_url(&permission.input);
    let link = elicit::url(&permission.input);
    let fields = StoredValue::new(elicit::parse(&permission.input));
    let text = RwSignal::new(fields.with_value(|f| initial(f)));
    let picks = RwSignal::new(fields.with_value(|f| f.iter().map(|f| (f.key.clone(), BTreeSet::<String>::new())).collect::<HashMap<_, _>>()));
    let opened = RwSignal::new(false);
    let answered = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let rid = StoredValue::new(permission.request_id.clone());

    let send = move |d: Decision, content: Option<Value>| {
        if answered.get_untracked() {
            return;
        }
        answered.set(true);
        error.set(None);
        let args = AnswerArgs { request_id: rid.get_value(), decision: d, mode: None, message: None, input: content, destination: None };
        let id = session.id();
        spawn_local(async move {
            if let Err(e) = ipc::session_answer(&id, &args).await {
                error.set(Some(t!("Perm.AnswerFailed", e)));
                answered.set(false);
            }
        });
    };
    let submit = move || {
        if answered.get_untracked() {
            return;
        }
        let mut content = None;
        if !url {
            let (c, invalid) = fields.with_value(|f| text.with_untracked(|t| picks.with_untracked(|p| elicit::content(f, t, p))));
            match c {
                Some(c) => content = Some(Value::Object(c)),
                None => {
                    error.set(Some(t!("Elicit.Invalid", invalid.unwrap_or_default())));
                    return;
                }
            }
        }
        send(Decision::Allow, content);
    };
    let decline = move || send(Decision::Deny, None);
    on_relayed_key(move |key| match key {
        "Enter" => submit(),
        "Delete" => decline(),
        _ => {}
    });

    let title = permission
        .info
        .as_ref()
        .and_then(|i| i.title.clone())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| t!("Elicit.Asks", server(&permission.input)));
    let msg = permission.description.clone().filter(|m| !m.is_empty()).map(|m| view! { <p class="prose">{m}</p> });
    let req = |f: &Field| f.required.then(|| view! { <span aria-hidden="true">" *"</span> });

    let body = if url {
        match link {
            Some(href) => view! {
                <pre class="cmd">{href.clone()}</pre>
                <div class="acts">
                    <a class="btn" href=href target="_blank" rel="noopener noreferrer" on:click=move |_| opened.set(true)>
                        <Icon name="right" sm=true />{move || t!(if opened.get() { "Elicit.OpenAgain" } else { "Elicit.Open" })}
                    </a>
                </div>
                {move || opened.get().then(|| view! { <p role="status">{t!("Elicit.Waiting")}</p> })}
            }
            .into_any(),
            None => view! { <div class="errbox">{t!("Elicit.BadUrl")}</div> }.into_any(),
        }
    } else {
        fields
            .get_value()
            .into_iter()
            .map(|f| {
                let id = format!("el-{}-{}", rid.get_value(), f.key);
                let key = f.key.clone();
                let val = {
                    let key = key.clone();
                    move || text.with(|t| t.get(&key).cloned().unwrap_or_default())
                };
                let set = {
                    let key = key.clone();
                    move |v: String| text.update(|t| {
                        t.insert(key.clone(), v);
                    })
                };
                match f.kind {
                    FieldKind::Boolean => {
                        let on = {
                            let val = val.clone();
                            move || val() == "true"
                        };
                        let on2 = on.clone();
                        let desc = f.description.clone().map(|d| view! { <span>{d}</span> });
                        view! {
                            <div class=move || if on() { "opt on" } else { "opt" }>
                                <input id=id.clone() type="checkbox" prop:checked=on2 disabled=answered
                                    on:change=move |e| set(if event_target_checked(&e) { "true" } else { "false" }.to_string()) />
                                <label for=id><b>{f.label.clone()}</b>{desc}</label>
                            </div>
                        }
                        .into_any()
                    }
                    FieldKind::Multi => {
                        let desc = f.description.clone().map(|d| view! { <span class="cap">{d}</span> });
                        let opts = f
                            .options
                            .iter()
                            .map(|(value, label)| {
                                let oid = format!("{id}-{value}");
                                let on = {
                                    let (key, value) = (key.clone(), value.clone());
                                    move || picks.with(|p| p.get(&key).is_some_and(|s| s.contains(&value)))
                                };
                                let on2 = on.clone();
                                let (key, value) = (key.clone(), value.clone());
                                view! {
                                    <div class=move || if on() { "opt on" } else { "opt" }>
                                        <input id=oid.clone() type="checkbox" prop:checked=on2 on:change=move |e| {
                                            let checked = event_target_checked(&e);
                                            picks.update(|p| {
                                                let s = p.entry(key.clone()).or_default();
                                                if checked {
                                                    s.insert(value.clone());
                                                } else {
                                                    s.remove(&value);
                                                }
                                            });
                                        } />
                                        <label for=oid><b>{label.clone()}</b></label>
                                    </div>
                                }
                            })
                            .collect_view();
                        view! {
                            <fieldset disabled=answered>
                                <legend><span class="qt">{f.label.clone()}{req(&f)}</span>{desc}</legend>
                                {opts}
                            </fieldset>
                        }
                        .into_any()
                    }
                    FieldKind::Enum => {
                        let opts = f
                            .options
                            .iter()
                            .map(|(value, label)| {
                                let (v, v2) = (value.clone(), value.clone());
                                let val = val.clone();
                                view! { <option value=v selected=move || val() == v2>{label.clone()}</option> }
                            })
                            .collect_view();
                        view! {
                            <div class="field">
                                <label for=id.clone()>{f.label.clone()}{req(&f)}</label>
                                <select id=id prop:value=val disabled=answered aria-required=f.required.then_some("true")
                                    on:change=move |e| set(event_target_value(&e))>
                                    <option value="">"—"</option>
                                    {opts}
                                </select>
                            </div>
                        }
                        .into_any()
                    }
                    _ => {
                        let ty = match f.kind {
                            FieldKind::Number | FieldKind::Integer => "number".to_string(),
                            _ => f.input_type.clone(),
                        };
                        let did = format!("{id}-d");
                        let described = f.description.as_ref().map(|_| did.clone());
                        let desc = f.description.clone().map(|d| view! { <p id=did class="cap">{d}</p> });
                        view! {
                            <div class="field">
                                <label for=id.clone()>{f.label.clone()}{req(&f)}</label>
                                <div class="input"><input id=id type=ty step=(f.kind == FieldKind::Number).then_some("any")
                                    min=num(f.min) max=num(f.max) prop:value=val disabled=answered
                                    aria-required=f.required.then_some("true") aria-describedby=described
                                    on:input=move |e| set(event_target_value(&e)) /></div>
                                {desc}
                            </div>
                        }
                        .into_any()
                    }
                }
            })
            .collect_view()
            .into_any()
    };

    view! {
        <form class="ask" on:submit=move |e| {
            e.prevent_default();
            submit();
        }>
            <h3>{title}</h3>
            {msg}
            {body}
            <div class="acts">
                <button type="submit" class="btn primary" disabled=answered>
                    {move || if url { t!(if opened.get() { "Elicit.Done" } else { "Elicit.Accept" }) } else { t!("Ask.Submit") }}" "<kbd>"⏎"</kbd>
                </button>
                <button type="button" class="btn danger" disabled=answered on:click=move |_| decline()>{t!("Elicit.Decline")}</button>
            </div>
            {move || error.get().map(|e| view! { <div class="errbox" role="alert">{e}</div> })}
            <p class="keys"><kbd>"⏎"</kbd>" "{t!(if url { "Elicit.HintAccept" } else { "Ask.Hint.Submit" })}" · "<kbd>{t!("Key.Del")}</kbd>" "{t!("Elicit.HintDecline")}<span class="perm-off">" · "{t!("Perm.HintFocus")}</span></p>
        </form>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn defaults_and_server() {
        let req = json!({"mcp_server_name":"gh","requested_schema":{"properties":{
            "ok":{"type":"boolean","default":true},"n":{"type":"string","default":"x"},"m":{"type":"string"}}}});
        let t = initial(&elicit::parse(&req));
        assert_eq!(t["ok"], "true");
        assert_eq!(t["n"], "x");
        assert_eq!(t["m"], "");
        assert_eq!(server(&req), "gh");
        assert_eq!(server(&json!({"display_name":"GitHub","mcp_server_name":"gh"})), "GitHub");
        assert_eq!(server(&json!({})), "MCP");
        assert_eq!(num(Some(1.5)).as_deref(), Some("1.5"));
        assert_eq!(num(Some(3.0)).as_deref(), Some("3"));
    }
}
