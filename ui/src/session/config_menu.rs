//! Port of `Components/Session/ConfigMenu.razor` — spec 07 §16. Owner: ui-session-thread-composer.

use crate::interop;
use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::model::Item;
use ccui_shared::session_rules::{config_keys, CONFIG_USAGE};
use ccui_shared::t;
use leptos::prelude::*;

/// The latest answer to a bare /config in the thread (top-level text starting with the usage line), parsed.
pub fn keys(items: &[&Item]) -> Option<Vec<(String, Vec<String>)>> {
    items
        .iter()
        .rev()
        .find_map(|i| match i {
            Item::Text(t) if t.parent_tool_use_id.is_none() && t.markdown.starts_with(CONFIG_USAGE) => Some(&t.markdown),
            _ => None,
        })
        .and_then(|md| config_keys(md))
}

/// `/config key=value` menu from `session_rules::config_keys`.
/// The CLI's settings by key, from its own reply to a bare "/config" (sent once if the thread has none). A pick is
/// sent as "/config key=value", as typed in the terminal: the CLI checks and saves it, its answer lands in the
/// thread. No current values: the CLI's reply lists none. Escape is the Composer's (closes, focus to the prompt).
#[component]
pub fn ConfigMenu(session: SessionHandle, on_close: Callback<()>) -> impl IntoView {
    let _ = on_close; // the Composer's shield and Escape close it
    let error = RwSignal::new(None::<String>);
    let note = RwSignal::new(None::<String>);
    let busy = RwSignal::new(false);
    let found = Memo::new(move |_| session.with_items(keys));
    let id = session.id();
    if found.get_untracked().is_none() {
        let id = id.clone();
        leptos::task::spawn_local(async move {
            if let Err(e) = ipc::session_send(&id, "/config", &[]).await {
                let _ = error.try_set(Some(t!("Composer.ActionFailed", e)));
            }
        });
    }
    let el = NodeRef::<leptos::html::Div>::new();
    let focused = StoredValue::new(false);
    Effect::new(move |_| {
        if focused.get_value() || found.with(Option::is_none) {
            return;
        }
        if let Some(b) = el.get() {
            focused.set_value(true);
            interop::focus(&b);
        }
    });
    let set = move |key: String, value: String| {
        if value.trim().is_empty() || busy.get_untracked() {
            return;
        }
        busy.set(true);
        error.set(None);
        note.set(None);
        let id = session.id();
        leptos::task::spawn_local(async move {
            match ipc::session_send(&id, &format!("/config {key}={}", value.trim()), &[]).await {
                Ok(_) => {
                    let _ = note.try_set(Some(t!("Config.Sent", key)));
                }
                Err(e) => {
                    let _ = error.try_set(Some(t!("Composer.ActionFailed", e)));
                }
            }
            let _ = busy.try_set(false);
        });
    };
    let rows = move || {
        found.get().map(|keys| {
            let rows = keys
                .into_iter()
                .map(|(key, choices)| {
                    let k = key.clone();
                    let field = if choices.is_empty() {
                        view! {
                            <span class="input"><input placeholder=t!("Config.Value") disabled=move || busy.get() spellcheck="false"
                                autocomplete="off" on:change=move |e| set(k.clone(), event_target_value(&e))/></span>
                        }
                        .into_any()
                    } else {
                        let options = choices.into_iter().map(|c| { let v = c.clone(); view! { <option value=v>{c}</option> } }).collect_view();
                        view! {
                            <span class="picks"><select disabled=move || busy.get() on:change=move |e| {
                                let sel = event_target::<web_sys::HtmlSelectElement>(&e);
                                let v = sel.value();
                                sel.set_value(""); // always back to "—" after a pick
                                set(k.clone(), v);
                            }>
                                <option value="">"—"</option>
                                {options}
                            </select></span>
                        }
                        .into_any()
                    };
                    let title = key.clone();
                    view! { <label class="si"><b title=title>{key}</b>{field}<span></span></label> }
                })
                .collect_view();
            view! { <div class="cfg">{rows}</div> }
        })
    };
    view! {
        <div class="pop slash menu" role="dialog" aria-labelledby="cfg-cap" tabindex="-1" node_ref=el>
            <div class="sh"><span id="cfg-cap">{t!("Config.Title")}</span>
                <span role="status" class=move || error.with(Option::is_some).then_some("c-err-fg")>
                    {move || error.get().or_else(|| note.get())}</span></div>
            {move || found.with(Option::is_none).then(|| view! { <p class="mm-cache">{t!("Config.Loading")}</p> })}
            {rows}
            <p class="mm-cache">{t!("Config.Hint")}</p>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccui_shared::model::TextItem;

    #[test]
    fn latest_usage() {
        let text = |md: &str, parent: Option<&str>| {
            Item::Text(TextItem { markdown: md.into(), parent_tool_use_id: parent.map(Into::into), uuid: None, at: None })
        };
        let items = [text("hello", None), text(&format!("{CONFIG_USAGE}\n"), Some("agent"))];
        let refs: Vec<&Item> = items.iter().collect();
        assert_eq!(keys(&refs), None);
        let items = [text(&format!("{CONFIG_USAGE}\n"), None), text("after", None)];
        let refs: Vec<&Item> = items.iter().collect();
        assert_eq!(keys(&refs), config_keys(&format!("{CONFIG_USAGE}\n")));
    }
}
