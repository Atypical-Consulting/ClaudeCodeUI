//! Port of `Components/Session/TodoPanel.razor` — spec 07 §6, 02 §10. Owner: ui-session-thread-composer.

use crate::common::icon::Icon;
use crate::state::SessionHandle;
use ccui_shared::t;
use ccui_shared::todo_list::{self, TodoStatus};
use leptos::prelude::*;

/// The agent's task list (`todo_list::from(items)` memo), collapsible. Pinned between the thread and the composer like
/// the terminal's todo panel. Open while work is left, unless the header was clicked; collapsed, it keeps one line:
/// progress and the task in progress. `open` is per mount (SessionPage keys its pane by session).
#[component]
pub fn TodoPanel(session: SessionHandle) -> impl IntoView {
    let todos = Memo::new(move |_| session.with_items(|it| todo_list::from(it)));
    let open = RwSignal::new(None::<bool>);
    let is_open = Memo::new(move |_| {
        open.get().unwrap_or_else(|| todos.with(|l| l.iter().any(|t| t.status != TodoStatus::Completed)))
    });
    let now = move || todos.with(|l| l.iter().find(|t| t.status == TodoStatus::InProgress).map(|t| t.label().to_string()));
    let items = move || {
        todos
            .get()
            .into_iter()
            .map(|t| {
                let done = t.status == TodoStatus::Completed;
                view! {
                    <li class=css(t.status)>
                        <span class="mk" role="img" aria-label=todo_list::status_label(t.status)>
                            {done.then(|| view! { <Icon name="check" sm=true/> })}
                        </span>{t.label().to_string()}
                    </li>
                }
            })
            .collect_view()
    };
    view! {
        <Show when=move || !todos.read().is_empty()>
            <section class="todos" aria-label=t!("Todo.Title")>
                <button type="button" class="th" aria-expanded=move || if is_open.get() { "true" } else { "false" }
                    aria-controls=move || is_open.get().then_some("todo-list")
                    on:click=move |_| open.set(Some(!is_open.get_untracked()))>
                    {move || view! { <Icon name=if is_open.get() { "down" } else { "right" } sm=true/> }}
                    <span class="cap">{t!("Todo.Title")}</span>
                    <span class="n">{move || todos.with(|l| todo_list::progress(l))}</span>
                    {move || (!is_open.get()).then(now).flatten().map(|n| view! { <span class="now">{n}</span> })}
                </button>
                <Show when=move || is_open.get()>
                    <ul id="todo-list">{items}</ul>
                </Show>
            </section>
        </Show>
    }
}

/// InProgress "now", Completed "done".
pub fn css(s: TodoStatus) -> Option<&'static str> {
    match s {
        TodoStatus::InProgress => Some("now"),
        TodoStatus::Completed => Some("done"),
        TodoStatus::Pending => None,
    }
}
