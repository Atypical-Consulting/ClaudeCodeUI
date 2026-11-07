//! Port of `Components/Session/HookActivity.razor` — spec 07 §7. Owner: ui-session-thread-composer.

use crate::common::icon::Icon;
use ccui_shared::model::*;
use ccui_shared::t;
use leptos::prelude::*;

/// Consecutive shown hooks (`<details>` open when one failed/blocked).
/// Hooks that failed, blocked or printed something (system/hook_response, --include-hook-events). Folded by default:
/// hook output can run long. Opened when one failed, since a blocking hook changed what the agent could do. The
/// `open` attribute follows `bad` only when it changes, so the user's own toggle stays (as Blazor's diff did).
#[component]
pub fn HookActivity(
    /// The block's hooks (derived from `Block::Hooks` indexes): the block grows in place while hooks arrive.
    #[prop(into)]
    hooks: Signal<Vec<HookItem>>,
) -> impl IntoView {
    let bad = Memo::new(move |_| hooks.with(|h| h.iter().any(|h| h.hook.outcome == "error")));
    let first_error = move || hooks.with(|h| h.iter().find(|h| h.hook.outcome == "error").map(|h| label(&h.hook)));
    let rows = move || {
        hooks
            .get()
            .into_iter()
            .map(|h| {
                let HookEvt { name, output, .. } = h.hook.clone();
                let aria = format!("{name}: {}", t!("Hook.Output"));
                view! {
                    <div class="hk">
                        <div class="w"><b>{name}</b><span>{label(&h.hook)}</span></div>
                        {(!output.is_empty()).then(|| view! { <pre tabindex="0" aria-label=aria>{output}</pre> })}
                    </div>
                }
            })
            .collect_view()
    };
    view! {
        <details class=move || if bad.get() { "hooks bad" } else { "hooks" } open=move || bad.get()>
            <summary>
                <Icon name="zap" sm=true/>
                <span class="hn">{t!("Hook.Title")}</span>
                <span class="hs">{move || hooks.with(|h| names(h))}</span>
                {move || first_error().map(|l| view! { <span class="badge no">{l}</span> })}
            </summary>
            {rows}
        </details>
    }
}

/// `HookActivity.Label`: success → Hook.Output, error exit 2 → Hook.Blocking, error → Hook.Failed(code ?? "—"), any
/// other outcome as the CLI names it (only success / error were observed; a cancelled hook is not a failure).
pub fn label(h: &HookEvt) -> String {
    match (h.outcome.as_str(), h.exit_code) {
        ("success", _) => t!("Hook.Output"),
        ("error", Some(2)) => t!("Hook.Blocking"),
        ("error", code) => t!("Hook.Failed", code.map_or_else(|| "—".to_string(), |c| c.to_string())),
        (other, _) => other.to_string(),
    }
}

/// Names grouped in first-seen order: "PreToolUse ×2, Stop".
pub fn names(hooks: &[HookItem]) -> String {
    let mut groups: Vec<(&str, usize)> = Vec::new();
    for h in hooks {
        match groups.iter_mut().find(|(n, _)| *n == h.hook.name) {
            Some((_, c)) => *c += 1,
            None => groups.push((&h.hook.name, 1)),
        }
    }
    groups.iter().map(|(n, c)| if *c > 1 { format!("{n} ×{c}") } else { n.to_string() }).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(name: &str, outcome: &str, code: Option<i32>) -> HookItem {
        HookItem { hook: HookEvt { name: name.into(), outcome: outcome.into(), exit_code: code, output: String::new(), id: None } }
    }

    #[test]
    fn labels_and_names() {
        let hs = [hook("Pre", "success", None), hook("Stop", "error", Some(2)), hook("Pre", "cancelled", None)];
        assert_eq!(names(&hs), "Pre ×2, Stop");
        assert_eq!(label(&hs[0].hook), t!("Hook.Output"));
        assert_eq!(label(&hs[1].hook), t!("Hook.Blocking"));
        assert_eq!(label(&hook("x", "error", None).hook), t!("Hook.Failed", "—"));
        assert_eq!(label(&hs[2].hook), "cancelled");
    }
}
