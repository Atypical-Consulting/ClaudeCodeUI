//! Port of `Components/Session/ModelMenu.razor` — spec 07 §30. Owner: ui-session-thread-composer.

use crate::interop;
use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::session::{SessionHeavy, SessionMeta};
use ccui_shared::{i18n, json, list_nav, t};
use leptos::prelude::*;
use serde_json::Value;

/// `ModelMenu.Models(s)`: `initialize_info.models[]` (`{value, displayName, description, resolvedModel,
/// supportsEffort, supportedEffortLevels[], supportsFastMode}`).
pub fn models(info: Option<&Value>) -> Vec<Value> {
    match info.and_then(|i| json::prop(i, "models")) {
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    }
}

/// `ModelMenu.Advisors(s)`: the /advisor command's argumentHint ("[fable|opus|sonnet|off]" on 2.1.296); no command,
/// no advisor (a gate the CLI decides).
pub fn advisors(info: Option<&Value>) -> Vec<String> {
    let Some(Value::Array(cmds)) = info.and_then(|i| json::prop(i, "commands")) else { return Vec::new() };
    let Some(hint) = cmds.iter().find(|c| json::str(c, "name") == Some("advisor")).and_then(|c| json::str(c, "argumentHint")) else {
        return Vec::new();
    };
    hint.trim_matches(['[', ']', '<', '>', ' ']).split('|').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect()
}

/// `ModelMenu.Current(s)` over the raw model entries: `model` is a launch alias ("haiku"), a set_model value or the
/// resolved id from init ("claude-haiku-5-5"). By value, else the models whose resolvedModel is a case-insensitive
/// prefix of it, preferring one whose value is not "default".
pub fn current_model(model: Option<&str>, info: Option<&Value>) -> Option<Value> {
    let ms = models(info);
    if let Some(m) = ms.iter().find(|m| model.is_some() && json::str(m, "value") == model) {
        return Some(m.clone());
    }
    let x = model?.to_lowercase();
    let resolved: Vec<&Value> =
        ms.iter().filter(|m| json::str(m, "resolvedModel").is_some_and(|r| x.starts_with(&r.to_lowercase()))).collect();
    resolved.iter().find(|m| json::str(m, "value") != Some("default")).or(resolved.first()).map(|m| (*m).clone())
}

/// The current model entry of initialize_info.models (`ModelMenu.Current`): (value, displayName ?? value).
/// CONTRACT CHANGE: takes the heavy part too (initialize_info lives there, not in the meta).
pub fn current(meta: &SessionMeta, heavy: &SessionHeavy) -> Option<(String, String)> {
    let m = current_model(meta.model.as_deref(), heavy.initialize_info.as_ref())?;
    let value = json::string(&m, "value");
    let name = json::str(&m, "displayName").map_or_else(|| value.clone(), str::to_string);
    Some((value, name))
}

/// `Label(value)`: the model's displayName, else the value.
fn label(models: &[Value], value: &str) -> String {
    models
        .iter()
        .find(|m| json::str(m, "value") == Some(value))
        .and_then(|m| json::str(m, "displayName"))
        .unwrap_or(value)
        .to_string()
}

fn resolved(models: &[Value], value: &str) -> Option<String> {
    models.iter().find(|m| json::str(m, "value") == Some(value)).and_then(|m| json::str(m, "resolvedModel")).map(str::to_string)
}

fn class(on: bool, kb: bool) -> &'static str {
    match (on, kb) {
        (true, true) => "si on kb",
        (true, false) => "si on",
        (false, true) => "si kb",
        (false, false) => "si",
    }
}

fn tag(on: bool) -> AnyView {
    if on {
        view! { <span class="tag acc">{t!("Model.Current")}</span> }.into_any()
    } else {
        view! { <span></span> }.into_any()
    }
}

/// Models (initialize_info.models), effort (capped), advisor, fast, thinking, make default.
/// One keyboard list: models (set_model), then the advisor group (/advisor, when offered), the output styles
/// (/output-style) and "set as default". The cursor is keyboard-only (hover does not move it).
#[component]
pub fn ModelMenu(session: SessionHandle, on_close: Callback<()>, on_escape: Callback<()>) -> impl IntoView {
    let error = RwSignal::new(None::<String>);
    let note = RwSignal::new(None::<String>);
    // The lists are computed once, when the menu opens.
    let (models, advisors, styles) = session.heavy.with_untracked(|h| {
        let info = h.initialize_info.as_ref();
        (models(info), advisors(info), info.map(|i| json::names(i, "available_output_styles")).unwrap_or_default())
    });
    let models = StoredValue::new(models);
    let advisors = StoredValue::new(advisors);
    let styles = StoredValue::new(styles);
    let cur = Memo::new(move |_| {
        let m = session.meta.with(|m| m.model.clone());
        session.heavy.with(|h| current_model(m.as_deref(), h.initialize_info.as_ref())).map(|c| json::string(&c, "value"))
    });
    let (nm, na, ns) = (models.with_value(Vec::len), advisors.with_value(Vec::len), styles.with_value(Vec::len));
    let count = move || (nm + na + ns + usize::from(cur.with(Option::is_some))) as i32;
    let first = cur.with_untracked(|c| models.with_value(|ms| ms.iter().position(|m| json::str(m, "value") == c.as_deref())));
    let sel = RwSignal::new(first.unwrap_or(0) as i32);
    let list = NodeRef::<leptos::html::Div>::new();
    let focused = StoredValue::new(false);
    Effect::new(move |_| {
        let s = sel.get();
        let Some(el) = list.get() else { return };
        if !focused.get_value() {
            focused.set_value(true);
            interop::focus(&el);
        }
        interop::reveal(&el, &format!("mm-opt-{s}"), "nearest", false);
    });

    let id = session.id();
    let id = StoredValue::new(id);
    let pick = move |value: String| {
        let id = id.get_value();
        leptos::task::spawn_local(async move {
            match ipc::session_set_model(&id, &value).await {
                Ok(()) => on_close.run(()),
                Err(e) => {
                    let _ = error.try_set(Some(t!("Model.SetFailed", e)));
                }
            }
        });
    };
    let pick_advisor = move |a: String| {
        let id = id.get_value();
        leptos::task::spawn_local(async move {
            let model = (a != "off").then_some(a.as_str());
            match ipc::session_set_advisor(&id, model).await {
                Ok(()) => {
                    // Weaker than the model: the CLI keeps no advisor. Say so and stay open.
                    if a != "off" && session.meta.try_with_untracked(|m| m.advisor.is_none()).unwrap_or(false) {
                        let l = models.try_with_value(|ms| label(ms, &a)).unwrap_or_else(|| a.clone());
                        let _ = error.try_set(Some(t!("Model.AdvisorRefused", l)));
                        return;
                    }
                    on_close.run(());
                }
                Err(e) => {
                    let _ = error.try_set(Some(t!("Model.SetFailed", e)));
                }
            }
        });
    };
    let pick_style = move |style: String| {
        let id = id.get_value();
        leptos::task::spawn_local(async move {
            match ipc::session_set_output_style(&id, &style).await {
                Ok(()) => on_close.run(()),
                Err(e) => {
                    let _ = error.try_set(Some(t!("Model.SetFailed", e)));
                }
            }
        });
    };
    let set_default = move || {
        let Some(v) = cur.get_untracked() else { return };
        error.set(None);
        note.set(None);
        let effort = session.meta.with_untracked(|m| m.effort.clone());
        let id = id.get_value();
        leptos::task::spawn_local(async move {
            match ipc::session_set_default(&id, &v, effort.as_deref()).await {
                Ok(()) => {
                    let _ = note.try_set(Some(t!(if effort.as_deref() == Some("max") { "Model.DefaultSetNoMax" } else { "Model.DefaultSet" })));
                }
                Err(e) => {
                    let _ = error.try_set(Some(t!("Model.DefaultFailed", e)));
                }
            }
        });
    };
    let activate = move |n: usize| {
        if n < nm {
            pick(models.with_value(|ms| json::string(&ms[n], "value")));
        } else if n < nm + na {
            pick_advisor(advisors.with_value(|a| a[n - nm].clone()));
        } else if n < nm + na + ns {
            pick_style(styles.with_value(|s| s[n - nm - na].clone()));
        } else {
            set_default();
        }
    };
    let key = move |e: leptos::ev::KeyboardEvent| {
        e.prevent_default();
        match e.key().as_str() {
            "Enter" | " " => activate(sel.get_untracked().max(0) as usize),
            "Escape" | "Tab" => on_escape.run(()),
            k => sel.set(list_nav::next(k, sel.get_untracked(), count())),
        }
    };

    // Memos: the meta is replaced on every patch, and options rebuilt under the pointer would swallow its click.
    let advisor_m = Memo::new(move |_| session.meta.with(|m| m.advisor.clone()));
    let style_m = Memo::new(move |_| session.meta.with(|m| m.output_style.clone()).unwrap_or_else(|| "default".into()));
    let effort_m = Memo::new(move |_| session.meta.with(|m| m.effort.clone()));
    let options = move || {
        let s = sel.get();
        let c = cur.get();
        let advisor = advisor_m.get();
        let output_style = style_m.get();
        let model_rows = models.with_value(|ms| {
            ms.iter()
                .enumerate()
                .map(|(i, m)| {
                    let value = json::string(m, "value");
                    let on = c.as_deref() == Some(value.as_str());
                    let name = json::str(m, "displayName").map_or_else(|| value.clone(), str::to_string);
                    let desc = json::str(m, "description").map(str::to_string);
                    let (name_t, desc_t) = (name.clone(), desc.clone());
                    view! {
                        <div id=format!("mm-opt-{i}") class=class(on, i as i32 == s) role="option" aria-selected=if on { "true" } else { "false" }
                            on:click=move |_| pick(value.clone())>
                            <b title=name_t>{name}</b>
                            <span title=desc_t>{desc}</span>
                            {tag(on)}
                        </div>
                    }
                })
                .collect_view()
        });
        let advisor_rows = (na > 0).then(|| {
            let rows = advisors.with_value(|a| {
                a.iter()
                    .enumerate()
                    .map(|(j, a)| {
                        let n = nm + j;
                        let on = if a == "off" {
                            advisor.is_none()
                        } else {
                            models.with_value(|ms| resolved(ms, a)).is_some_and(|r| Some(&r) == advisor.as_ref())
                        };
                        let text = if a == "off" { t!("Model.AdvisorOff") } else { models.with_value(|ms| label(ms, a)) };
                        let a = a.clone();
                        view! {
                            <div id=format!("mm-opt-{n}") class=class(on, n as i32 == s) role="option" aria-selected=if on { "true" } else { "false" }
                                on:click=move |_| pick_advisor(a.clone())>
                                <b>{text}</b><span></span>{tag(on)}
                            </div>
                        }
                    })
                    .collect_view()
            });
            view! {
                <div role="group" aria-labelledby="mm-adv">
                    <div class="gh" id="mm-adv" role="presentation" title=t!("Model.AdvisorHint")>{t!("Model.Advisor")}</div>
                    {rows}
                </div>
            }
        });
        let style_rows = (ns > 0).then(|| {
            let rows = styles.with_value(|st| {
                st.iter()
                    .enumerate()
                    .map(|(j, st)| {
                        let n = nm + na + j;
                        let on = *st == output_style;
                        let st = st.clone();
                        let shown = st.clone();
                        view! {
                            <div id=format!("mm-opt-{n}") class=class(on, n as i32 == s) role="option" aria-selected=if on { "true" } else { "false" }
                                on:click=move |_| pick_style(st.clone())>
                                <b>{shown}</b><span></span>{tag(on)}
                            </div>
                        }
                    })
                    .collect_view()
            });
            view! {
                <div role="group" aria-labelledby="mm-style">
                    <div class="gh" id="mm-style" role="presentation" title=t!("Model.StyleHint")>{t!("Model.Style")}</div>
                    {rows}
                </div>
            }
        });
        let default_row = c.map(|cv| {
            let d = nm + na + ns;
            let l = models.with_value(|ms| label(ms, &cv));
            let sub = match effort_m.get() {
                Some(e) => format!("{l} · {}", i18n::name("Effort.", Some(&e))),
                None => l,
            };
            view! {
                <div id=format!("mm-opt-{d}") class=if d as i32 == s { "si kb" } else { "si" } role="option" aria-selected="false"
                    on:click=move |_| set_default()>
                    <b>{t!("Model.SetDefault")}</b>
                    <span>{sub}</span><span></span>
                </div>
            }
        });
        view! { {model_rows}{advisor_rows}{style_rows}{default_row} }
    };
    view! {
        <div class="pop slash menu">
            <div class="sh"><span>{t!("Model")}</span>
                <span role="status" class=move || error.with(Option::is_some).then_some("c-err-fg")>{move || error.get().or_else(|| note.get())}</span></div>
            <div role="listbox" aria-label=t!("Model") node_ref=list tabindex="0"
                aria-activedescendant=move || format!("mm-opt-{}", sel.get()) on:keydown=key>
                {options}
            </div>
            // The prompt cache is per model: the next turn re-reads the whole conversation at full price.
            {move || (session.meta.with(|m| m.context_tokens) > 0).then(|| view! { <p class="mm-cache">{t!("Model.CacheHint")}</p> })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn info() -> Value {
        json!({
            "models": [
                {"value":"default","displayName":"Default","resolvedModel":"claude-opus-5"},
                {"value":"opus","displayName":"Opus","resolvedModel":"claude-opus-5"},
                {"value":"haiku","displayName":"Haiku","resolvedModel":"claude-haiku-5-5"}
            ],
            "commands": [{"name":"advisor","argumentHint":"[fable|opus| sonnet |off]"}]
        })
    }

    #[test]
    fn current_and_advisors() {
        let i = info();
        let v = |m: Option<&str>| current_model(m, Some(&i)).map(|c| json::string(&c, "value"));
        assert_eq!(v(Some("haiku")).as_deref(), Some("haiku"));
        assert_eq!(v(Some("claude-opus-5-20261101")).as_deref(), Some("opus"));
        assert_eq!(v(Some("CLAUDE-HAIKU-5-5")).as_deref(), Some("haiku"));
        assert_eq!(v(Some("gpt")), None);
        assert_eq!(v(None), None);
        assert_eq!(advisors(Some(&i)), ["fable", "opus", "sonnet", "off"]);
        assert!(advisors(Some(&json!({"commands":[]}))).is_empty());
        let meta = SessionMeta { model: Some("haiku".into()), ..Default::default() };
        let heavy = SessionHeavy { initialize_info: Some(i.clone()), ..Default::default() };
        assert_eq!(current(&meta, &heavy), Some(("haiku".into(), "Haiku".into())));
    }
}
