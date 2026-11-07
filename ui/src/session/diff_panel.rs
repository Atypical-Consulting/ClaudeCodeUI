//! Port of `Components/Session/DiffPanel.razor` — spec 07 §17. Owner: ui-session-inspector.
//! The CLI's /diff: the repository's uncommitted changes (git diff HEAD, untracked files shown as new), re-read when a
//! file-changing tool finishes and on Refresh. Read-only: nothing is staged or written.

use crate::common::diff_view::DiffView;
use crate::common::icon::Icon;
use crate::ipc;
use crate::state::{use_ui, SessionHandle};
use ccui_shared::api::GitDiff;
use ccui_shared::edit_diff::DiffKind;
use ccui_shared::model::*;
use ccui_shared::{i18n, t, tool_kinds};
use leptos::prelude::*;
use leptos::task::spawn_local;

/// ponytail: a fixed cap, page them if a working tree that large is ever common.
const MAX_FILES: usize = 100;

/// Finished tools that may have changed files (Edit/Write/MultiEdit/NotebookEdit, shell commands).
pub fn changing_done<I: std::borrow::Borrow<Item>>(items: &[I]) -> usize {
    items
        .iter()
        .filter(|i| {
            i.borrow().as_tool().is_some_and(|t| {
                t.state == ToolState::Done
                    && (tool_kinds::is_edit(&t.name) || tool_kinds::is_shell(&t.name) || t.name == "NotebookEdit")
            })
        })
        .count()
}

/// Uncommitted changes of the session folder (git_diff) as DiffViews.
#[component]
pub fn DiffPanel(
    session: SessionHandle,
) -> impl IntoView {
    let ui = use_ui();
    let diff = RwSignal::new(None::<GitDiff>);
    let busy = RwSignal::new(false);
    let seen = StoredValue::new(None::<(usize, String)>);
    let done = RwSignal::new(0u32);
    let key = Memo::new(move |_| (session.with_items(|i| changing_done(i)), session.meta.with(|m| m.cwd.clone())));

    let load = move || {
        let k = key.get_untracked();
        seen.set_value(Some(k.clone()));
        busy.set(true);
        spawn_local(async move {
            match ipc::git_diff(&k.1).await {
                Ok(d) => diff.set(Some(d)),
                Err(e) => {
                    leptos::logging::error!("git_diff: {e}");
                    diff.set(Some(GitDiff::default()));
                }
            }
            busy.set(false);
            done.update(|n| *n += 1); // a tool finished while git ran: the effect reads again
        });
    };
    Effect::new(move |_| {
        let k = key.get();
        done.track();
        if busy.get_untracked() || seen.get_value().as_ref() == Some(&k) {
            return;
        }
        load();
    });

    let sub = move || {
        diff.with(|d| {
            let files = &d.as_ref()?.files;
            if files.is_empty() {
                return None;
            }
            let count = |kind| files.iter().map(|f| f.lines.iter().filter(|l| l.kind == kind).count()).sum::<usize>();
            Some(view! {
                <span class="sub">
                    {format!("{} · +{} −{}", i18n::plural(files.len() as i64, "Diff.Files"), count(DiffKind::Add), count(DiffKind::Del))}
                </span>
            })
        })
    };
    let status = move || {
        diff.with(|d| match d {
            None => t!("Diff.Loading"),
            Some(d) if d.root.is_none() => t!("Diff.NotRepo"),
            Some(d) if d.files.is_empty() => t!("Diff.None"),
            _ => String::new(),
        })
    };
    let files = move || {
        let files = diff.with(|d| d.as_ref().map(|d| d.files.clone()).unwrap_or_default());
        let more = files.len().saturating_sub(MAX_FILES);
        view! {
            {files.into_iter().take(MAX_FILES).map(|f| view! { <DiffView path=f.path lines=f.lines /> }).collect_view()}
            {(more > 0).then(|| view! { <p style="margin:0;color:var(--fg-4);font-size:13px">{t!("Diff.MoreFiles", more)}</p> })}
        }
    };

    view! {
        <div class="ph"><h2 class="ttl">{t!("Diff.Title")}</h2>
            {sub}
            <span class="end">
                <button class="btn ghost" title=t!("Diff.Refresh") aria-label=t!("Diff.Refresh") disabled=busy on:click=move |_| load()><Icon name="refresh" sm=true /></button>
                <button class="btn ghost" title=t!("Action.Close") aria-label=t!("Action.Close") on:click=move |_| ui.diff_open.set(false)><Icon name="x" sm=true /></button>
            </span>
        </div>
        <div class="scroll diffs" aria-busy=move || if busy.get() { "true" } else { "false" }>
            <p role="status" style="margin:0;color:var(--fg-4);font-size:13px">{status}</p>
            {files}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, state: ToolState) -> Item {
        Item::Tool(ToolItem {
            id: "t".into(),
            name: name.into(),
            input: serde_json::Value::Null,
            parent_tool_use_id: None,
            state,
            result_text: None,
            structured: None,
            started_at: Default::default(),
            ended_at: None,
            task_id: None,
            task_status: None,
            progress: None,
            tokens: 0,
            sub_tool_uses: 0,
            background: false,
            model: None,
        })
    }

    #[test]
    fn counts_finished_changers() {
        let items = vec![
            tool("Edit", ToolState::Done),
            tool("Bash", ToolState::Done),
            tool("NotebookEdit", ToolState::Done),
            tool("Write", ToolState::Running),
            tool("Read", ToolState::Done),
        ];
        assert_eq!(changing_done(&items), 3);
    }
}
