//! Port of `Components/Layout/Rail.razor` — spec 06 §4. Owner: ui-shell.

use crate::common::icon::Icon;
use crate::ipc;
use crate::layout::mascot::Mascot;
use crate::layout::quota_meter::QuotaMeter;
use crate::layout::rail_session_row::{current_path, link_active, RailPastRow, RailSessionRow};
use crate::state::{use_app, use_ui, AppStore};
use ccui_shared::api::PastSession;
use ccui_shared::model::*;
use ccui_shared::session::{SessionSummary, SessionsSummary};
use ccui_shared::{i18n, t, text};
use leptos::prelude::*;

/// How many recent transcripts the Rail asks for (`TranscriptStore.Recent()` default; the palette reads the same).
pub const RECENT_TAKE: usize = 30;
const RECENT_SHOWN: usize = 8;
const RECENT_TTL_MS: f64 = 30_000.0;

/// Active sessions grouped by repository, groups and rows in the order each was first opened.
pub fn groups(s: &SessionsSummary) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for x in s.sessions.iter().filter(|x| x.status != SessionStatus::Exited) {
        let repo = x.repo.clone().unwrap_or_else(|| text::repo_name(&x.cwd));
        match out.iter_mut().find(|(r, _)| *r == repo) {
            Some((_, ids)) => ids.push(x.id.clone()),
            None => out.push((repo, vec![x.id.clone()])),
        }
    }
    out
}

/// `Sessions.IsActive(id)`: a live session that has not exited.
pub fn is_active(s: &SessionsSummary, id: &str) -> bool {
    s.sessions.iter().any(|x| x.id == id && x.status != SessionStatus::Exited)
}

/// `FailedMcp`: failed MCP servers of the last session that has an init.
pub fn failed_mcp(s: &SessionsSummary) -> i32 {
    s.sessions.iter().rev().find(|x| x.has_init).map_or(0, |x| x.failed_mcp)
}

/// One live session of the summary as a signal that keeps its last value once the session leaves the list (a keyed
/// row may outlive it by one render).
pub fn live_row(app: AppStore, first: SessionSummary) -> Signal<SessionSummary> {
    let id = first.id.clone();
    Memo::new(move |prev: Option<&SessionSummary>| {
        app.summary
            .with(|s| s.sessions.iter().find(|x| x.id == id).cloned())
            .or_else(|| prev.cloned())
            .unwrap_or_else(|| first.clone())
    })
    .into()
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}

/// Brand, actions (new, search, overview, worktrees tag, background pill, extensions pill, settings), live
/// sessions grouped by repo, recent (8, throttled 30 s / generation), QuotaMeter, Mascot. Starts `bg_watch` on mount.
#[component]
pub fn Rail() -> impl IntoView {
    let app = use_app();
    let ui = use_ui();
    let cur = current_path();

    // TranscriptStore.Recent reads up to ~4 MB of transcripts when its 30 s cache expires: never two loads in flight,
    // at most every 30 s, unless the generation moved (a session was stopped).
    let recent = RwSignal::new(Vec::<PastSession>::new());
    let loading = StoredValue::new(false);
    let loaded_at = StoredValue::new(f64::NEG_INFINITY);
    let loaded_gen = StoredValue::new(None::<u64>);
    let load_recent = move || {
        let gen = app.transcripts_generation.get_untracked();
        let fresh = loaded_gen.get_value().is_some_and(|g| g >= gen) && now_ms() - loaded_at.get_value() < RECENT_TTL_MS;
        if loading.get_value() || fresh {
            return;
        }
        loaded_at.set_value(now_ms());
        loading.set_value(true);
        leptos::task::spawn_local(async move {
            // Invalidated while reading (a Stop during the scan): read again, its push found this load busy.
            loop {
                match ipc::recent_sessions(RECENT_TAKE).await {
                    Ok(r) => {
                        let _ = recent.try_set(r.list);
                        loaded_gen.try_set_value(Some(r.generation));
                        if r.generation >= app.transcripts_generation.get_untracked() {
                            break;
                        }
                    }
                    Err(e) => {
                        leptos::logging::error!("recent_sessions: {e}");
                        break;
                    }
                }
            }
            loading.try_set_value(false);
        });
    };
    Effect::new(move |_| {
        // Sessions, worktrees, background agents and the transcript generation all re-read (throttled).
        app.summary.track();
        app.worktree_count.track();
        app.bg_needs.track();
        app.transcripts_generation.track();
        load_recent();
    });
    leptos::task::spawn_local(async {
        if let Err(e) = ipc::bg_watch().await {
            leptos::logging::error!("bg_watch: {e}");
        }
    });

    // NavLink: `cur` by prefix (New session: exact); aria-current only on the exact path.
    let cls = move |base: &'static str, href: &'static str, exact: bool| {
        move || {
            let c = cur.get();
            let on = if exact { c == href } else { link_active(&c, href) };
            if on { format!("{base} cur") } else { base.to_string() }
        }
    };
    let aria = move |href: &'static str| move || (cur.get() == href).then_some("page");

    let repos = Memo::new(move |_| app.summary.with(groups));
    let past = Memo::new(move |_| {
        let s = app.summary.read();
        recent.with(|r| r.iter().filter(|p| !is_active(&s, &p.id)).take(RECENT_SHOWN).cloned().collect::<Vec<_>>())
    });
    // The Overview at rest is the recent list, curated: a second, uncurated one beside it would undo its triage.
    let show_past = move || !past.read().is_empty() && !(cur.get() == "overview" && repos.read().is_empty());

    view! {
        <nav class="pane" aria-label=t!("Rail.Label")>
            <div class="ph"><span class="brand"><i></i>"Claude Code"</span></div>
            <div class="rail-actions">
                <a tabindex="0" class=cls("btn block", "", true) href="" aria-current=aria("")>
                    <Icon name="plus" sm=true/>{t!("Rail.NewSession")}<kbd>{i18n::kbd("Alt N")}</kbd>
                </a>
                <button class="search" tabindex="0" on:click=move |_| ui.palette_open.set(true)>
                    <Icon name="search" sm=true/>{t!("Rail.Search")}<kbd>{i18n::kbd("Ctrl K")}</kbd>
                </button>
                <a tabindex="0" class=cls("btn block ghost", "overview", false) href="overview" aria-current=aria("overview")>
                    <Icon name="grid" sm=true/>{t!("Nav.Overview")}
                </a>
                <a tabindex="0" class=cls("btn block ghost", "worktrees", false) href="worktrees" aria-current=aria("worktrees")>
                    <Icon name="branch" sm=true/>{t!("Nav.Worktrees")}
                    {move || app.worktree_count.get().filter(|n| *n > 0).map(|n| view! {
                        <span class="tag" title=t!("Rail.ToCleanHelp")>{t!("Rail.ToClean", n)}</span>
                    })}
                </a>
                <a tabindex="0" class=cls("btn block ghost", "background", false) href="background" aria-current=aria("background")>
                    <Icon name="agents" sm=true/>{t!("Nav.Background")}
                    {move || Some(app.bg_needs.get()).filter(|n| *n > 0).map(|n| view! {
                        <span class="pill wait"><span aria-hidden="true">{n}</span><span class="vh">{format!(" {}", i18n::plural(n as i64, "Bg.Needs"))}</span></span>
                    })}
                </a>
                <a tabindex="0" class=cls("btn block ghost", "extensions", false) href="extensions" aria-current=aria("extensions")>
                    <Icon name="plug" sm=true/>{t!("Nav.Extensions")}
                    {move || Some(app.summary.with(failed_mcp)).filter(|n| *n > 0).map(|n| view! {
                        <span class="pill err"><span aria-hidden="true">{n}</span><span class="vh">{i18n::plural(n as i64, "Rail.FailedMcp")}</span></span>
                    })}
                </a>
                <a tabindex="0" class=cls("btn block ghost", "settings/appearance", false) href="settings/appearance" aria-current=aria("settings/appearance")>
                    <Icon name="palette" sm=true/>{t!("Settings.Title")}
                </a>
            </div>
            <div class="scroll">
                <For each=move || repos.get() key=|(repo, _)| repo.clone() let:group>
                    {
                        let repo = group.0.clone();
                        let ids = Memo::new(move |_| {
                            repos.with(|g| g.iter().find(|(r, _)| *r == repo).map(|(_, ids)| ids.clone()).unwrap_or_default())
                        });
                        view! {
                            <div class="grp"><span class="cap">{group.0.clone()}</span><span class="c">{move || ids.read().len()}</span></div>
                            <div class="list">
                                <For each=move || ids.get() key=|id| id.clone() let:id>
                                    {app.summary
                                        .with_untracked(|s| s.sessions.iter().find(|x| x.id == id).cloned())
                                        .map(|first| view! { <RailSessionRow session=live_row(app, first)/> })}
                                </For>
                            </div>
                        }
                    }
                </For>
                <Show when=show_past>
                    <div class="grp"><span class="cap">{t!("Rail.Recent")}</span></div>
                    <div class="list">
                        <For each=move || past.get() key=|p| p.id.clone() let:p>
                            <RailPastRow past=p/>
                        </For>
                    </div>
                </Show>
            </div>
            <QuotaMeter/>
            <Mascot/>
        </nav>
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rust_decimal::Decimal;

    pub(crate) fn summary(id: &str, cwd: &str, repo: Option<&str>, status: SessionStatus) -> SessionSummary {
        SessionSummary {
            id: id.into(),
            version: 1,
            name: id.into(),
            cwd: cwd.into(),
            repo: repo.map(String::from),
            branch: None,
            worktree: None,
            mode: String::new(),
            status,
            untitled: false,
            items_len: 0,
            pending_count: 0,
            first_pending_tool: None,
            exit_label: String::new(),
            exit_code: None,
            start_failed: false,
            cost_usd: Decimal::ZERO,
            cost_at_open: Decimal::ZERO,
            tool_count: 0,
            started_at: Default::default(),
            last_event_at: Default::default(),
            last_result_subtype: None,
            last_result_at: None,
            last_turn_ms: None,
            has_process: true,
            can_fork: false,
            has_init: false,
            failed_mcp: 0,
            last_tool: None,
            todos: None,
        }
    }

    #[test]
    fn groups_by_repo_in_first_seen_order() {
        let s = SessionsSummary {
            sessions: vec![
                summary("a", "/x/alpha", None, SessionStatus::Idle),
                summary("b", "/x/beta", Some("beta"), SessionStatus::Running),
                summary("c", "/x/alpha/.claude/worktrees/w", None, SessionStatus::Waiting),
                summary("d", "/x/gamma", None, SessionStatus::Exited),
            ],
            ..Default::default()
        };
        assert_eq!(
            groups(&s),
            vec![("alpha".to_string(), vec!["a".to_string(), "c".to_string()]), ("beta".to_string(), vec!["b".to_string()])]
        );
        assert!(is_active(&s, "a"));
        assert!(!is_active(&s, "d"));
        assert!(!is_active(&s, "zz"));
    }

    #[test]
    fn failed_mcp_reads_the_last_init() {
        let mut a = summary("a", "/a", None, SessionStatus::Idle);
        a.has_init = true;
        a.failed_mcp = 2;
        let b = summary("b", "/b", None, SessionStatus::Idle);
        let s = SessionsSummary { sessions: vec![a.clone(), b], ..Default::default() };
        assert_eq!(failed_mcp(&s), 2);
        let mut c = summary("c", "/c", None, SessionStatus::Idle);
        c.has_init = true;
        let s = SessionsSummary { sessions: vec![a, c], ..Default::default() };
        assert_eq!(failed_mcp(&s), 0);
    }
}
