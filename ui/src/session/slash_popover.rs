//! Port of `Components/Session/SlashPopover.razor` — spec 07 §12. Owner: ui-session-thread-composer.

use crate::state::SessionHandle;
use ccui_shared::model::*;
use ccui_shared::{json, t, text};
use leptos::prelude::*;
use serde_json::Value;

/// One "/" command: name, description, tag (Built-in, or the plugin of "plugin:cmd").
#[derive(Clone, Debug, PartialEq)]
pub struct Cmd {
    pub name: String,
    pub description: String,
    pub tag: Option<String>,
}

impl Cmd {
    /// The part after the last ':' (plugin commands are matched and shown by their command part).
    pub fn show(&self) -> &str {
        self.name.rfind(':').map_or(&self.name, |i| &self.name[i + 1..])
    }
}

fn plugin(name: &str) -> Option<String> {
    name.find(':').map(|i| name[..i].to_string())
}

/// `initialize.commands` (`{name, description, builtin}`), else init's slash_commands names.
pub fn commands(info: Option<&Value>, init_slash: &[String], builtin: &str) -> Vec<Cmd> {
    if let Some(Value::Array(cmds)) = info.and_then(|i| json::prop(i, "commands")) {
        return cmds
            .iter()
            .filter_map(|c| {
                let name = json::str(c, "name")?;
                Some(Cmd {
                    name: name.to_string(),
                    description: json::string(c, "description"),
                    tag: if matches!(json::prop(c, "builtin"), Some(Value::Bool(true))) { Some(builtin.to_string()) } else { plugin(name) },
                })
            })
            .collect();
    }
    init_slash.iter().map(|n| Cmd { name: n.clone(), description: String::new(), tag: plugin(n) }).collect()
}

/// Commands whose shown part starts with `q` (case-insensitive), built-ins first, then plain, then plugin ones
/// (stable), at most 8.
pub fn matches(cmds: Vec<Cmd>, q: &str, builtin: &str) -> Vec<Cmd> {
    let n = q.chars().count();
    let mut m: Vec<Cmd> = cmds
        .into_iter()
        .filter(|c| {
            let head: String = c.show().chars().take(n).collect();
            head.chars().count() == n && head.chars().zip(q.chars()).all(|(a, b)| text::eq_ci(a, b))
        })
        .collect();
    m.sort_by_key(|c| if c.tag.as_deref() == Some(builtin) { 0 } else if c.name.contains(':') { 2 } else { 1 });
    m.truncate(8);
    m
}

/// Slash command list (initialize_info.commands ∪ init slash_commands) filtered by the query; keyboard via ctl.
/// The Composer relays ↑ ↓ Tab: ArrowUp / ArrowDown move, Tab / Enter pick.
#[component]
pub fn SlashPopover(
    session: SessionHandle,
    #[prop(into)] query: Signal<Option<String>>,
    on_pick: Callback<String>,
    ctl: super::PopoverCtl,
) -> impl IntoView {
    let q = Memo::new(move |_| query.with(|q| q.as_deref().unwrap_or("").trim_start_matches('/').to_string()));
    let list = Memo::new(move |_| {
        let builtin = t!("Slash.Builtin");
        let cmds = session.heavy.with(|h| {
            let init = h.init.as_ref().map(|i| i.slash_commands.as_slice()).unwrap_or(&[]);
            commands(h.initialize_info.as_ref(), init, &builtin)
        });
        q.with(|q| matches(cmds, q, &builtin))
    });
    // The cursor, with the query it belongs to: a new query starts at the top.
    let cursor = RwSignal::new((String::new(), 0usize));
    let sel = Memo::new(move |_| {
        let len = list.with(Vec::len);
        cursor.with(|(cq, s)| q.with(|q| if cq == q && *s < len { *s } else { 0 }))
    });
    Effect::new(move |_| {
        ctl.open.set(!list.read().is_empty());
        ctl.active_id.set(Some(format!("slash-opt-{}", sel.get())));
    });
    ctl.reset_on_cleanup();
    let pick = move || {
        if let Some(c) = list.with_untracked(|l| l.get(sel.get_untracked()).cloned()) {
            on_pick.run(c.name);
        }
    };
    ctl.on_key(move |k| {
        let len = list.with_untracked(Vec::len);
        match k {
            "ArrowDown" | "ArrowUp" if len > 0 => {
                let d = if k == "ArrowDown" { 1 } else { len - 1 };
                cursor.set((q.get_untracked(), (sel.get_untracked() + d) % len));
            }
            "Tab" | "Enter" => pick(),
            _ => {}
        }
    });
    let starting = Memo::new(move |_| session.meta.with(|m| m.status == SessionStatus::Starting));
    move || {
        let items = list.get();
        if items.is_empty() {
            // The list comes with the CLI's initialize answer: a cold start (worktree, hooks, MCP) takes a few seconds.
            return starting.get()
                .then(|| view! { <div class="pop slash" role="status"><div class="sh"><span>{t!("Slash.Loading")}</span></div></div> }.into_any());
        }
        let n = q.with(|q| q.chars().count());
        let s = sel.get();
        let rows = items
            .into_iter()
            .enumerate()
            .map(|(i, c)| {
                let show = c.show().to_string();
                let head: String = show.chars().take(n).collect();
                let rest: String = show.chars().skip(n).collect();
                let on = i == s;
                let name = c.name.clone();
                view! {
                    <div id=format!("slash-opt-{i}") class=if on { "si on" } else { "si" } role="option"
                        aria-selected=if on { "true" } else { "false" }
                        on:mousedown=|e| e.prevent_default() on:click=move |_| on_pick.run(name.clone())>
                        <b title=format!("/{}", c.name)>"/"<mark>{head}</mark>{rest}</b>
                        <span title=c.description.clone()>{c.description.clone()}</span>
                        {match c.tag {
                            Some(tag) => view! { <span class="tag">{tag}</span> }.into_any(),
                            None => view! { <span></span> }.into_any(),
                        }}
                    </div>
                }
            })
            .collect_view();
        Some(
            view! {
                <div class="pop slash">
                    <div class="sh"><span>{t!("Slash.Title")}</span><span><kbd>"↑"</kbd>" "<kbd>"↓"</kbd>" "<kbd>"Tab"</kbd></span></div>
                    <div id="slash-list" role="listbox" aria-label=t!("Slash.Title")>{rows}</div>
                </div>
            }
            .into_any(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn filter_and_order() {
        let info = json!({"commands":[
            {"name":"pr:commit","description":"plugin"},
            {"name":"compact","description":"Compact","builtin":true},
            {"name":"commit-all","description":"user"},
            {"name":"clear","builtin":true},
            {"description":"no name"}
        ]});
        let cmds = commands(Some(&info), &[], "Built-in");
        assert_eq!(cmds.len(), 4);
        let m = matches(cmds.clone(), "CO", "Built-in");
        let names: Vec<&str> = m.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["compact", "commit-all", "pr:commit"]);
        assert_eq!(m[2].tag.as_deref(), Some("pr"));
        assert_eq!(m[2].show(), "commit");
        assert_eq!(matches(cmds, "", "Built-in").len(), 4);
        let fallback = commands(None, &["a:b".into(), "c".into()], "Built-in");
        assert_eq!(fallback[0].tag.as_deref(), Some("a"));
        assert_eq!(fallback[1].tag, None);
    }
}
