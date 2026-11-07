//! Port of `Components/Session/MentionPopover.razor` — spec 07 §23, 04 §3. Owner: ui-session-thread-composer.

use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::api::FileIndexResult;
use ccui_shared::{file_index, json, list_nav, t, text};
use leptos::prelude::*;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};

static SCAN: AtomicU64 = AtomicU64::new(0);

/// `"agent-<name>"` → description, from initialize's agents (with descriptions), else init's names; only names the
/// CLI can read (`@agent-[\w:.@-]+`: "stripe:Company Researcher" cannot be mentioned). Order kept, duplicates dropped.
pub fn agents(info: Option<&Value>, init_agents: &[String]) -> Vec<(String, String)> {
    let raw: Vec<(String, String)> = match info.and_then(|i| json::prop(i, "agents")) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| json::str(x, "name").map(|n| (format!("agent-{n}"), json::string(x, "description"))))
            .collect(),
        _ => init_agents.iter().map(|n| (format!("agent-{n}"), String::new())).collect(),
    };
    let mut out: Vec<(String, String)> = Vec::new();
    for (k, d) in raw {
        let ok = k.len() > 6 && k[6..].chars().all(|c| c.is_alphanumeric() || c == '_' || matches!(c, ':' | '.' | '@' | '-'));
        if ok && !out.iter().any(|(x, _)| *x == k) {
            out.push((k, d));
        }
    }
    out
}

/// `Refilter`: no query → up to 3 agents, then the shallowest paths; a query ranks agents with the files (a long
/// agent name loosely matches many queries and must not take Tab from a real hit).
pub fn refilter(agents: &[String], paths: &[String], q: &str) -> Vec<String> {
    if q.is_empty() {
        let mut found = file_index::filter(agents, "", 3);
        let rest = file_index::MAX_RESULTS - found.len();
        found.extend(file_index::filter(paths, "", rest));
        found
    } else {
        let all: Vec<String> = agents.iter().chain(paths).cloned().collect();
        file_index::filter(&all, q, file_index::MAX_RESULTS)
    }
}

/// @ mentions: file_index_scan once per cwd, `file_index::filter` per query change, agents first.
/// The session's subagents ("@agent-<name>", which the CLI runs as that agent), then files and folders of the
/// session cwd. The Composer relays ↑ ↓ (ListNav) and Tab / Enter (pick). The cwd is scanned once per opening.
#[component]
pub fn MentionPopover(
    session: SessionHandle,
    #[prop(into)] query: Signal<Option<String>>,
    on_pick: Callback<String>,
    ctl: super::PopoverCtl,
) -> impl IntoView {
    let agents = StoredValue::new(session.heavy.with_untracked(|h| {
        agents(h.initialize_info.as_ref(), h.init.as_ref().map(|i| i.agents.as_slice()).unwrap_or(&[]))
    }));
    let index = RwSignal::new(None::<FileIndexResult>);
    let q = Memo::new(move |_| query.get().unwrap_or_default());
    // Only on a new query: a filter over 20k paths costs ms (the scan's arrival refilters too).
    let matches = Memo::new(move |_| {
        let keys: Vec<String> = agents.with_value(|a| a.iter().map(|(k, _)| k.clone()).collect());
        index.with(|i| q.with(|q| refilter(&keys, i.as_ref().map_or(&[][..], |i| &i.paths), q)))
    });
    let cursor = RwSignal::new((String::new(), 0i32));
    let sel = Memo::new(move |_| {
        let len = matches.with(Vec::len) as i32;
        cursor.with(|(cq, s)| q.with(|q| if cq == q && *s < len { *s } else { 0 }))
    });
    Effect::new(move |_| {
        let open = !matches.read().is_empty();
        ctl.open.set(open);
        ctl.active_id.set(open.then(|| format!("mention-opt-{}", sel.get())));
    });
    ctl.reset_on_cleanup();
    ctl.on_key(move |k| match k {
        "Tab" | "Enter" => {
            if let Some(p) = matches.with_untracked(|m| m.get(sel.get_untracked() as usize).cloned()) {
                on_pick.run(p);
            }
        }
        k => {
            let next = list_nav::next(k, sel.get_untracked(), matches.with_untracked(Vec::len) as i32);
            cursor.set((q.get_untracked(), next.max(0)));
        }
    });

    // The scan, off the UI thread in the backend; cancelled when the popover goes.
    let request = SCAN.fetch_add(1, Ordering::Relaxed) + 1;
    let cwd = session.meta.with_untracked(|m| m.cwd.clone());
    let id = session.id();
    leptos::task::spawn_local(async move {
        let got = match ipc::file_index_scan(request, &cwd).await {
            Ok(r) => r,
            Err(e) => {
                leptos::logging::error!("[{id}] file index: {e}");
                FileIndexResult { paths: Vec::new(), truncated: false }
            }
        };
        let _ = index.try_set(Some(got));
    });
    on_cleanup(move || {
        if index.try_with_untracked(Option::is_none).unwrap_or(true) {
            leptos::task::spawn_local(async move {
                let _ = ipc::file_index_cancel(request).await;
            });
        }
    });

    move || {
        let scanning = index.with(Option::is_none);
        let items = matches.get();
        if !scanning && items.is_empty() && q.with(String::is_empty) {
            return None;
        }
        let status = index.with(|i| match i {
            None => t!("Mention.Scanning"),
            Some(i) if i.truncated => t!("Mention.Truncated", i.paths.len()),
            Some(_) => String::new(),
        });
        let query = q.get();
        let s = sel.get();
        let empty = items.is_empty();
        let rows = items
            .into_iter()
            .enumerate()
            .map(|(n, p)| {
                let name = file_index::name(&p).to_string();
                let agent = agents.with_value(|a| a.iter().find(|(k, _)| *k == p).map(|(_, d)| d.clone()));
                let on = n as i32 == s;
                let label = match (!query.is_empty()).then(|| text::find_ci(&name, &query)).flatten() {
                    Some(hit) => {
                        let chars: Vec<char> = name.chars().collect();
                        let len = query.chars().count();
                        let (a, b, c) = (
                            chars[..hit].iter().collect::<String>(),
                            chars[hit..hit + len].iter().collect::<String>(),
                            chars[hit + len..].iter().collect::<String>(),
                        );
                        view! { {a}<mark>{b}</mark>{c} }.into_any()
                    }
                    None => name.clone().into_any(),
                };
                let side = match agent {
                    Some(d) => { let title = d.clone(); view! { <span title=title>{d}</span> }.into_any() }
                    None => {
                        let dir = p[..p.len() - name.len()].to_string();
                        view! { <span title=p.clone()>{dir}</span> }.into_any()
                    }
                };
                let pick = p.clone();
                view! {
                    <div id=format!("mention-opt-{n}") class=if on { "si on" } else { "si" } role="option"
                        aria-selected=if on { "true" } else { "false" }
                        on:mousedown=|e| e.prevent_default() on:click=move |_| on_pick.run(pick.clone())>
                        <b title=p>{label}</b>
                        {side}
                        <span></span>
                    </div>
                }
            })
            .collect_view();
        Some(view! {
            <div class="pop slash">
                <div class="sh">
                    <span>{t!("Mention.Title")}</span>
                    <span>{status}" "<kbd>"↑"</kbd>" "<kbd>"↓"</kbd>" "<kbd>"Tab"</kbd></span>
                </div>
                <div id="mention-list" role="listbox" aria-label=t!("Mention.Title") aria-busy=if scanning { "true" } else { "false" }>
                    {rows}
                    {(!scanning && empty).then(|| view! { <div class="si" aria-hidden="true"><span>{t!("Mention.NoMatch")}</span></div> })}
                </div>
            </div>
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn agent_names() {
        let info = json!({"agents":[{"name":"review","description":"Reviews"},{"name":"review"},{"name":"stripe:Company Researcher"},{"name":"a:b.c@d-e_f"}]});
        let a = agents(Some(&info), &[]);
        assert_eq!(a, [("agent-review".to_string(), "Reviews".to_string()), ("agent-a:b.c@d-e_f".to_string(), String::new())]);
        assert_eq!(agents(None, &["x".into()]), [("agent-x".to_string(), String::new())]);
    }

    #[test]
    fn ranking() {
        let agents = vec!["agent-a".to_string(), "agent-b".into(), "agent-c".into(), "agent-d".into()];
        let paths = vec!["src/".to_string(), "README.md".into()];
        let r = refilter(&agents, &paths, "");
        assert_eq!(r.len(), 5);
        assert_eq!(&r[..3], ["agent-a", "agent-b", "agent-c"]);
        assert_eq!(refilter(&agents, &paths, "read"), ["README.md"]);
    }
}
