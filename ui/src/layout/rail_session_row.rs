//! Port of `Components/Layout/RailSessionRow.razor` — spec 06 §4 RailSessionRow. Owner: ui-shell.
//! One rail row: a live session (link) or a past one (its page opens the transcript as history).

use crate::common::icon::Icon;
use crate::state::use_app;
use ccui_shared::model::*;
use ccui_shared::session::SessionSummary;
use ccui_shared::{fmt, i18n, session_rules, t, text};
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use rust_decimal::Decimal;

/// The base-relative path of the current route, without `?` / `#` (`Nav.ToBaseRelativePath(Nav.Uri)`).
pub fn current_path() -> Signal<String> {
    let loc = use_location();
    Signal::derive(move || loc.pathname.get().trim_start_matches('/').to_string())
}

/// NavLink prefix matching: the current path is `href` or below it.
pub fn link_active(cur: &str, href: &str) -> bool {
    cur == href || (!href.is_empty() && cur.starts_with(href) && cur[href.len()..].starts_with('/'))
}

/// A cost worth showing (≥ half a cent) as `$0.00`, else "".
pub fn cost_label(c: Option<Decimal>) -> String {
    match c {
        Some(c) if c >= Decimal::new(5, 3) => fmt::cost(c, 2),
        _ => String::new(),
    }
}

/// `Where(branch, repo)` as markup (nothing when empty).
fn where_view(branch: Option<&str>, repo: Option<&str>) -> Option<impl IntoView> {
    place(branch, repo).map(|w| view! { <div class="branch"><Icon name="branch"/>{w}</div> })
}

/// A live session row (status dot, title/titling, branch, pill/state/cost).
#[component]
pub fn RailSessionRow(
    /// Reactive so a keyed row follows its session (status, title) without being rebuilt; a plain value converts.
    #[prop(into)]
    session: Signal<SessionSummary>,
) -> impl IntoView {
    let cur = current_path();
    let href = move || format!("session/{}", session.read().id);
    let on = move || link_active(&cur.get(), &href());
    view! {
        <a class=move || if on() { "s on" } else { "s" } href=href aria-current=move || on().then_some("page")>
            {move || {
                let s = session.get();
                let name = if s.untitled {
                    let k = if s.items_len > 0 { "Rail.Titling" } else { "Rail.Untitled" };
                    view! { <div class="n pending">{t!(k)}</div> }.into_any()
                } else {
                    view! { <div class="n">{s.name.clone()}</div> }.into_any()
                };
                let place = where_view(s.worktree.as_deref().or(s.branch.as_deref()), None);
                let right = match s.status {
                    SessionStatus::Waiting => view! {
                        <span class="pill wait"><span aria-hidden="true">{s.pending_count}</span><span class="vh">{i18n::plural(s.pending_count as i64, "Mascot.Decision")}</span></span>
                    }
                    .into_any(),
                    SessionStatus::Running => view! { <span class="c c-ok">{t!("Status.Running")}</span> }.into_any(),
                    SessionStatus::Crashed => view! { <span class="pill err">{s.exit_label.clone()}</span> }.into_any(),
                    _ => view! { <span class="c">{cost_label(Some(s.cost_usd))}</span> }.into_any(),
                };
                view! {
                    <span class=format!("dot {}", dot(s.status))></span>
                    <div>{name}{place}</div>
                    {right}
                }
            }}
        </a>
    }
}

/// A past session row: click opens the transcript (session_open) then navigates.
#[component]
pub fn RailPastRow(
    past: ccui_shared::api::PastSession,
) -> impl IntoView {
    let app = use_app();
    let cur = current_path();
    let href = format!("session/{}", past.id);
    let on = Signal::derive({
        let href = href.clone();
        move || cur.get() == href
    });
    let id = past.id.clone();
    // SessionPage's `load_session` opens the transcript (`session_open`), so the link itself is the C# Open + navigate.
    let elsewhere = move || app.summary.with(|s| session_rules::running_elsewhere(s, &id));
    view! {
        <a class=move || if on.get() { "s on" } else { "s" } href=href aria-current=move || on.get().then_some("page")>
            <span class="dot exited"></span>
            <div>
                <div class="n">{past.title.clone()}</div>
                {where_view(past.branch.as_deref(), Some(&text::repo_name(&past.cwd)))}
                {move || elsewhere().then(|| view! { <div class="branch c-ok">{t!("Status.Elsewhere")}</div> })}
            </div>
            <span class="c">{cost_label(past.cost_usd)}</span>
        </a>
    }
}

/// `Dot(s)`: Waiting → wait, Running → run, Crashed → err, Exited → exited, else idle (reused by palette, Overview).
pub fn dot(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Waiting => "wait",
        SessionStatus::Running => "run",
        SessionStatus::Crashed => "err",
        SessionStatus::Exited => "exited",
        _ => "idle",
    }
}

/// `Where(branch, repo)` text: non-empty of [branch unless "HEAD", repo] joined " · " (None when empty).
pub fn place(branch: Option<&str>, repo: Option<&str>) -> Option<String> {
    let parts: Vec<&str> = [branch.filter(|b| *b != "HEAD"), repo].into_iter().flatten().filter(|s| !s.is_empty()).collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots() {
        assert_eq!(dot(SessionStatus::Waiting), "wait");
        assert_eq!(dot(SessionStatus::Running), "run");
        assert_eq!(dot(SessionStatus::Crashed), "err");
        assert_eq!(dot(SessionStatus::Exited), "exited");
        assert_eq!(dot(SessionStatus::Idle), "idle");
        assert_eq!(dot(SessionStatus::Starting), "idle");
    }

    #[test]
    fn places() {
        assert_eq!(place(Some("main"), Some("repo")).as_deref(), Some("main · repo"));
        assert_eq!(place(Some("HEAD"), Some("repo")).as_deref(), Some("repo"));
        assert_eq!(place(Some(""), None), None);
        assert_eq!(place(None, None), None);
        assert_eq!(place(Some("feat"), None).as_deref(), Some("feat"));
    }

    #[test]
    fn links_and_costs() {
        assert!(link_active("overview", "overview"));
        assert!(link_active("settings/appearance/x", "settings/appearance"));
        assert!(!link_active("overviews", "overview"));
        assert!(link_active("", ""));
        assert!(!link_active("overview", ""));
        assert_eq!(cost_label(Some(Decimal::new(4, 3))), "");
        assert_eq!(cost_label(None), "");
        assert_eq!(cost_label(Some(Decimal::new(125, 2))), fmt::cost(Decimal::new(125, 2), 2));
    }
}
