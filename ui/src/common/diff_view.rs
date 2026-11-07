//! Port of `Components/Shared/DiffView.razor` — spec 06 §7. Owner: ui-shell.
//! Unified diff of one file. Code lines are highlighted in place by interop.js (`[data-hl] code.lc`): the rows are
//! rendered static, once (no reactive text inside a `code.lc`).

use ccui_shared::edit_diff::{self, DiffKind, DiffLine};
use ccui_shared::t;
use leptos::prelude::*;

const MAX_ROWS: usize = 2000;

/// File header (+adds −dels) and up to 2000 rows; `data-hl` = `edit_diff::lang(path)`.
#[component]
pub fn DiffView(
    path: String,
    lines: Vec<ccui_shared::edit_diff::DiffLine>,
) -> impl IntoView {
    let adds = lines.iter().filter(|l| l.kind == DiffKind::Add).count();
    let dels = lines.iter().filter(|l| l.kind == DiffKind::Del).count();
    let more = lines.len().saturating_sub(MAX_ROWS);
    let lang = edit_diff::lang(&path);
    let rows = lines.into_iter().take(MAX_ROWS).map(row).collect_view();
    view! {
        <div class="diff">
            <div class="fh"><span title=path.clone()>{path.clone()}</span><span><span class="a">{format!("+{adds}")}</span>" "<span class="d">{format!("−{dels}")}</span></span></div>
            <div class="db" tabindex="0" role="region" aria-label=path><div data-hl=lang>
                {rows}
                {(more > 0).then(|| view! { <div class="hunk">{t!("Diff.MoreLines", more)}</div> })}
            </div></div>
        </div>
    }
}

fn num(n: Option<i32>) -> String {
    n.map(|n| n.to_string()).unwrap_or_default()
}

fn row(l: DiffLine) -> AnyView {
    match l.kind {
        DiffKind::Hunk => view! { <div class="hunk">{l.text}</div> }.into_any(),
        DiffKind::More => view! { <div class="hunk">{t!("Diff.MoreOccurrences", l.n.unwrap_or(0))}</div> }.into_any(),
        DiffKind::Ctx => {
            view! { <div class="l"><span class="n">{num(l.n)}</span><span></span><code class="lc">{l.text}</code></div> }.into_any()
        }
        DiffKind::Add | DiffKind::Del => {
            let add = l.kind == DiffKind::Add;
            view! {
                <div class=if add { "l add" } else { "l del" }>
                    <span class="n">{num(l.n)}</span><span class="g">{if add { "+" } else { "−" }}</span><code class="lc">{l.text}</code>
                </div>
            }
            .into_any()
        }
    }
}
