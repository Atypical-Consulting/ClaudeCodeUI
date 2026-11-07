//! Port of `Components/Session/Ledger.razor` — spec 07 §4. Owner: ui-session-thread-composer.

use crate::common::icon::Icon;
use crate::session::tool_row::ToolRow;
use crate::state::SessionHandle;
use ccui_shared::model::*;
use ccui_shared::{fmt, i18n};
use leptos::prelude::*;

/// A run of top-level tool calls (ToolRow each), collapsible. Expanded is the default (latest run open); clicking the
/// header overrides it for this instance.
#[component]
pub fn Ledger(
    session: SessionHandle,
    /// Item indexes of the run's ToolItems (`thread_blocks::Block::Run`); each ToolRow reads `session.tool_at(i)`.
    /// Local state (`open`) survives the run growing.
    #[prop(into)]
    tools: Signal<Vec<usize>>,
    /// Reactive: the Thread passes true for the latest run only, so an older run folds when a new one starts.
    #[prop(into, default = false.into())]
    expanded: Signal<bool>,
) -> impl IntoView {
    let open = RwSignal::new(None::<bool>);
    let is_open = Memo::new(move |_| open.get().unwrap_or_else(|| expanded.get()));
    let title = move || {
        tools.with(|ix| {
            let rows: Vec<(Time, Option<Time>)> = ix
                .iter()
                .filter_map(|&i| session.row(i))
                .filter_map(|r| r.with(|it| it.as_tool().map(|t| (t.started_at, t.ended_at))))
                .collect();
            head(&rows)
        })
    };
    view! {
        <div class="ledger">
            <button type="button" class="lh" aria-expanded=move || if is_open.get() { "true" } else { "false" }
                on:click=move |_| open.set(Some(!is_open.get_untracked()))>
                {move || view! { <Icon name=if is_open.get() { "down" } else { "right" } sm=true/> }}{title}
            </button>
            <Show when=move || is_open.get()>
                <For each=move || tools.get() key=|i| *i let:i>
                    <ToolRow session=session tool=session.tool_at(i)/>
                </For>
            </Show>
        </div>
    }
}

/// `Ledger.Head`: `Tools` plural, then ` · {dur}` when the summed run time (running tools count 0) is ≥ 50 ms.
pub fn head(tools: &[(Time, Option<Time>)]) -> String {
    let n = i18n::plural(tools.len() as i64, "Tools");
    let ms: i64 = tools.iter().map(|(s, e)| (e.unwrap_or(*s) - *s).num_milliseconds()).sum();
    if ms >= 50 {
        format!("{n} · {}", fmt::dur(ms))
    } else {
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heads() {
        let t0 = Time::default();
        let ms = |n| t0 + chrono::Duration::milliseconds(n);
        assert_eq!(head(&[(t0, None), (t0, Some(ms(30)))]), i18n::plural(2, "Tools"));
        assert_eq!(head(&[(t0, Some(ms(30))), (t0, Some(ms(4470)))]), format!("{} · {}", i18n::plural(2, "Tools"), fmt::dur(4500)));
    }
}
