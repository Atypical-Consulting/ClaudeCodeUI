//! Port of `Components/Layout/CommandPalette.razor` — spec 06 §5. Owner: ui-shell.
//! Ctrl K: sessions (live + recent), message hits from past transcripts, and actions, filtered. Escape is handled by
//! MainLayout. A modal combobox: the input owns focus and points at the active option; the app behind is inert
//! while open.

use crate::common::icon::Icon;
use crate::interop;
use crate::ipc;
use crate::layout::rail::{is_active, RECENT_TAKE};
use crate::layout::rail_session_row::{cost_label, current_path, dot};
use crate::state::{use_app, use_ui};
use ccui_shared::api::{PastSession, TranscriptHit, THEMES};
use ccui_shared::model::*;
use ccui_shared::session::{SessionSummary, SessionsSummary};
use ccui_shared::{fmt, i18n, session_rules, t, text};
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;

// Message search: from MIN_CHARS, debounced, cancelled by the next keystroke or by closing.
const MIN_CHARS: usize = 3;
const MAX_HITS: usize = 20;
const DEBOUNCE_MS: u32 = 250;

/// What a row does.
#[derive(Clone, Debug, PartialEq)]
pub enum Run {
    /// Close and navigate to this base-relative path ("" = New session, "?cwd=…", "session/{id}"…).
    Go(String),
    /// Fork that session, then open the fork.
    Fork(String),
    /// Close, stop that session, then the Overview.
    Stop(String),
    /// Next theme (the palette stays open).
    Theme,
    /// Close and check for updates.
    Update,
}

/// One palette row. `dot` = dot class for a session, icon name for an action; `msg` = a message hit (name is its
/// snippet).
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub session: bool,
    pub name: String,
    pub dot: String,
    pub detail: Option<String>,
    pub pill: usize,
    pub right: Option<String>,
    pub kbd: Option<String>,
    pub run: Run,
    pub msg: bool,
}

impl Row {
    /// Display order = rows order, so ↑/↓ follow the screen. Message hits arrive late and go last: appending never
    /// moves the row under `sel`, so "type an action, press Enter" still runs that action.
    pub fn kind(&self) -> u8 {
        if self.msg {
            2
        } else if self.session {
            0
        } else {
            1
        }
    }

    fn action(name: String, icon: &str, detail: Option<String>, pill: usize, right: Option<String>, kbd: Option<String>, run: Run) -> Row {
        Row { session: false, name, dot: icon.into(), detail, pill, right, kbd, run, msg: false }
    }
}

/// Everything `rows` reads.
pub struct Input<'a> {
    pub summary: &'a SessionsSummary,
    pub recent: &'a [PastSession],
    pub hits: &'a [TranscriptHit],
    pub q: &'a str,
    pub theme: &'a str,
    /// Route `session/{id}`.
    pub current: Option<&'a str>,
    pub version: &'a str,
    pub home: &'a str,
    pub sep: &'a str,
    pub now: Time,
    /// `RunningElsewhere(id)`.
    pub elsewhere: &'a dyn Fn(&str) -> bool,
}

/// `Join`: " · " over the non-empty parts.
fn join(parts: &[Option<&str>]) -> String {
    parts.iter().flatten().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" · ")
}

fn some(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}

/// `Hit(s)`: an empty (trimmed) query matches everything, else a case-insensitive contains.
pub fn hit(q: &str, s: Option<&str>) -> bool {
    let t = q.trim();
    t.is_empty() || s.is_some_and(|s| text::find_ci(s, t).is_some())
}

/// `Uri.EscapeDataString`: RFC 3986 unreserved characters kept, every other UTF-8 byte `%XX`.
pub fn escape_data(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn session_row(s: &SessionSummary) -> Row {
    let state = match s.status {
        SessionStatus::Waiting => s.first_pending_tool.as_ref().map(|p| t!("Status.WaitingOn", p)),
        SessionStatus::Idle | SessionStatus::Exited => some(cost_label(Some(s.cost_usd))),
        _ => None,
    };
    Row {
        session: true,
        name: s.name.clone(),
        dot: dot(s.status).into(),
        detail: some(join(&[s.branch.as_deref(), s.repo.as_deref(), state.as_deref()])),
        pill: if s.status == SessionStatus::Waiting { s.pending_count } else { 0 },
        right: match s.status {
            SessionStatus::Running => Some(t!("Status.Running")),
            SessionStatus::Crashed => Some(s.exit_label.clone()),
            SessionStatus::Waiting => None,
            _ => Some(t!("Status.Done")),
        },
        kbd: None,
        run: Run::Go(format!("session/{}", s.id)),
        msg: false,
    }
}

fn past_row(p: &PastSession, now: Time, elsewhere: &dyn Fn(&str) -> bool) -> Row {
    let repo = text::repo_name(&p.cwd);
    let day = fmt::day(p.last_write, now);
    let cost = cost_label(p.cost_usd);
    Row {
        session: true,
        name: p.title.clone(),
        dot: "exited".into(),
        detail: some(join(&[p.branch.as_deref(), Some(&repo), Some(&day), Some(&cost)])),
        pill: 0,
        right: Some(if elsewhere(&p.id) { t!("Status.Elsewhere") } else { t!("Status.History") }),
        kbd: None,
        // SessionPage opens the transcript (session_open): navigating is the C# Open + Go.
        run: Run::Go(format!("session/{}", p.id)),
        msg: false,
    }
}

/// The session on screen, else the most recent one; a worktree cwd gives its repo.
fn current_cwd(i: &Input) -> Option<String> {
    let here = i.current.and_then(|id| i.summary.sessions.iter().find(|s| s.id == id)).map(|s| s.cwd.clone());
    let cwd = here.or_else(|| i.summary.sessions.last().map(|s| s.cwd.clone())).or_else(|| i.recent.first().map(|p| p.cwd.clone()))?;
    Some(text::root_of(&cwd))
}

/// `Rows()`: sessions (needing you first, newest first), actions, then message hits.
pub fn rows(i: &Input) -> Vec<Row> {
    let s = i.summary;
    let mut active: Vec<&SessionSummary> = s.sessions.iter().filter(|x| x.status != SessionStatus::Exited).rev().collect();
    active.sort_by_key(|x| match x.status {
        SessionStatus::Waiting => 0,
        SessionStatus::Running => 1,
        _ => 2,
    });
    let live = active.into_iter().map(|x| (x.name.clone(), x.branch.clone(), x.repo.clone(), session_row(x)));
    let past = i
        .recent
        .iter()
        .filter(|p| !is_active(s, &p.id))
        .map(|p| (p.title.clone(), p.branch.clone(), Some(text::repo_name(&p.cwd)), past_row(p, i.now, i.elsewhere)));
    let take = if i.q.is_empty() { 5 } else { 8 };
    let mut out: Vec<Row> = live
        .chain(past)
        .filter(|(n, b, r, _)| hit(i.q, Some(n)) || hit(i.q, b.as_deref()) || hit(i.q, r.as_deref()))
        .take(take)
        .map(|(.., row)| row)
        .collect();

    let next = s.decision_queue.first();
    let here = i.current.and_then(|id| s.sessions.iter().find(|x| x.id == id));
    let cwd = current_cwd(i);
    let mut actions = Vec::new();
    if let Some(n) = next {
        actions.push(Row::action(
            t!("Palette.NextDecision"),
            "shield",
            Some(n.session_name.clone()),
            s.decision_queue.len(),
            None,
            Some(i18n::kbd("Ctrl ⇧ A")),
            Run::Go(format!("session/{}", n.session_id)),
        ));
    }
    if let Some(h) = here.filter(|h| h.can_fork) {
        actions.push(Row::action(t!("Session.Fork"), "branch", Some(h.name.clone()), 0, None, None, Run::Fork(h.id.clone())));
    }
    actions.push(match &cwd {
        None => Row::action(t!("Rail.NewSession"), "plus", None, 0, None, Some(i18n::kbd("Alt N")), Run::Go(String::new())),
        Some(c) => Row::action(
            t!("Palette.NewSessionIn", text::chip(c, i.home, i.sep)),
            "plus",
            None,
            0,
            None,
            Some(i18n::kbd("Alt N")),
            Run::Go(format!("?cwd={}", escape_data(c))),
        ),
    });
    actions.push(Row::action(t!("Nav.Overview"), "grid", None, 0, None, Some(i18n::kbd("Ctrl ⇧ O")), Run::Go("overview".into())));
    if let Some(h) = here.filter(|h| h.status != SessionStatus::Exited) {
        actions.push(Row::action(t!("Session.Stop"), "x", Some(h.name.clone()), 0, None, None, Run::Stop(h.id.clone())));
    }
    actions.push(Row::action(
        t!("Palette.Theme", t!(&format!("Theme.{}", i.theme))),
        "palette",
        None,
        0,
        Some(t!("Palette.ChangeTheme")),
        None,
        Run::Theme,
    ));
    actions.push(Row::action(t!("Nav.Worktrees"), "branch", None, 0, None, None, Run::Go("worktrees".into())));
    actions.push(Row::action(t!("Nav.Extensions"), "plug", None, 0, None, None, Run::Go("extensions".into())));
    actions.push(Row::action(t!("Settings.Title"), "palette", None, 0, None, None, Run::Go("settings/appearance".into())));
    actions.push(Row::action(t!("Appearance.UpdateCheck"), "refresh", None, 0, some(i.version.to_string()), None, Run::Update));
    out.extend(actions.into_iter().filter(|a| hit(i.q, Some(&a.name))));

    out.extend(i.hits.iter().map(|h| {
        let live = s.sessions.iter().find(|x| x.id == h.session.id);
        Row {
            session: true,
            name: h.snippet.clone(),
            dot: live.map_or("exited", |x| dot(x.status)).into(),
            detail: some(join(&[Some(&h.session.title), Some(&text::repo_name(&h.session.cwd))])),
            pill: 0,
            right: Some(fmt::day(h.at, i.now)),
            kbd: None,
            run: Run::Go(format!("session/{}", h.session.id)),
            msg: true,
        }
    }));
    out
}

/// `Mark(s)`: (before, match, after) of the first case-insensitive match of the trimmed query (whitespace-collapsed
/// fallback), rendered as text + `<mark>` + text.
pub fn mark(s: &str, q: &str) -> (String, Option<String>, String) {
    let mut t = q.trim().to_string();
    // A message snippet has its whitespace collapsed: so does the query when it is not found as typed.
    if !t.is_empty() && text::find_ci(s, &t).is_none() {
        t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    }
    let found = if t.is_empty() { None } else { text::find_ci(s, &t) };
    let Some(i) = found else { return (s.to_string(), None, String::new()) };
    let chars: Vec<char> = s.chars().collect();
    let n = t.chars().count();
    (chars[..i].iter().collect(), Some(chars[i..i + n].iter().collect()), chars[i + n..].iter().collect())
}

/// The theme after `theme` in the cycle.
pub fn next_theme(theme: &str) -> &'static str {
    let i = THEMES.iter().position(|t| *t == theme).map_or(0, |i| i + 1);
    THEMES[i % THEMES.len()]
}

/// Ctrl K modal combobox: sessions, actions, message hits (search_transcripts, 250 ms debounce,
/// 3+ chars, cancellable). Rendered only while `palette_open`.
#[component]
pub fn CommandPalette() -> impl IntoView {
    let ui = use_ui();
    let app = use_app();
    let navigate = use_navigate();
    let cur = current_path();

    let q = RwSignal::new(String::new());
    let sel = RwSignal::new(0usize);
    let theme = RwSignal::new("graphite".to_string());
    let recent = RwSignal::new(Vec::<PastSession>::new());
    let hits = RwSignal::new(Vec::<TranscriptHit>::new());
    let searching = RwSignal::new(false);
    let partial = RwSignal::new(false);
    let failed = RwSignal::new(false);
    // The latest search id: a newer keystroke or closing wins the race. `inflight` = the id the backend is scanning.
    let seq = StoredValue::new(0u64);
    let inflight = StoredValue::new(None::<u64>);
    let input = NodeRef::<leptos::html::Input>::new();
    let list = NodeRef::<leptos::html::Div>::new();

    let cancel = move || {
        seq.update_value(|s| *s += 1);
        if let Some(id) = inflight.try_update_value(Option::take).flatten() {
            leptos::task::spawn_local(async move {
                let _ = ipc::search_transcripts_cancel(id).await;
            });
        }
    };
    let find_messages = move |t: String| {
        cancel();
        hits.set(Vec::new());
        partial.set(false);
        failed.set(false);
        let go = t.chars().count() >= MIN_CHARS;
        searching.set(go);
        if !go {
            return;
        }
        let my = seq.get_value();
        leptos::task::spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(DEBOUNCE_MS).await;
            if seq.try_get_value() != Some(my) {
                return;
            }
            inflight.set_value(Some(my));
            let got = ipc::search_transcripts(my, &t, MAX_HITS).await;
            if seq.try_get_value() != Some(my) {
                return;
            }
            inflight.set_value(None);
            match got {
                Ok(r) => {
                    hits.set(r.hits);
                    partial.set(!r.complete);
                }
                // Fire-and-forget: never leave "Searching messages…" up.
                Err(_) => failed.set(true),
            }
            searching.set(false);
        });
    };

    let current = move || cur.get().strip_prefix("session/").map(|id| id.to_string());
    let all = Memo::new(move |_| {
        let summary = app.summary.read();
        let boot = app.boot.get_value();
        let id = current();
        let elsewhere = |id: &str| session_rules::running_elsewhere(&summary, id);
        rows(&Input {
            summary: &summary,
            recent: &recent.read(),
            hits: &hits.read(),
            q: &q.read(),
            theme: &theme.read(),
            current: id.as_deref(),
            version: &boot.version,
            home: &boot.home,
            sep: &boot.sep,
            now: chrono::Utc::now(),
            elsewhere: &elsewhere,
        })
    });
    let shown = move || {
        let n = all.read().len();
        sel.get().min(n.saturating_sub(1))
    };

    let run = {
        let navigate = navigate.clone();
        move |r: Run| {
            let go = {
                let navigate = navigate.clone();
                move |path: String| {
                    ui.palette_open.set(false);
                    navigate(&format!("/{path}"), NavigateOptions::default());
                }
            };
            match r {
                Run::Go(p) => go(p),
                Run::Fork(id) => leptos::task::spawn_local(async move {
                    match ipc::session_fork(&id, None).await {
                        Ok(fork) => go(format!("session/{fork}")),
                        Err(e) => leptos::logging::error!("session_fork: {e}"),
                    }
                }),
                Run::Stop(id) => {
                    ui.palette_open.set(false);
                    leptos::task::spawn_local(async move {
                        if let Err(e) = ipc::session_stop(&id).await {
                            leptos::logging::error!("session_stop: {e}");
                        }
                        go("overview".into());
                    });
                }
                Run::Theme => {
                    let next = next_theme(&theme.get_untracked());
                    theme.set(next.to_string());
                    interop::set_theme(next);
                    leptos::task::spawn_local(async move {
                        let _ = ipc::app_pref_set(ccui_shared::api::PREF_THEME, Some(next)).await;
                    });
                }
                Run::Update => {
                    ui.palette_open.set(false);
                    leptos::task::spawn_local(async {
                        let _ = ipc::check_for_updates().await;
                    });
                }
            }
        }
    };

    // Open / close transitions: reset and start an empty search, read the theme and recents, then make it modal.
    Effect::new(move |was: Option<bool>| {
        let open = ui.palette_open.get();
        let was = was.unwrap_or(false);
        if open && !was {
            q.set(String::new());
            sel.set(0);
            find_messages(String::new());
            theme.set(interop::get_theme());
            leptos::task::spawn_local(async move {
                if let Ok(r) = ipc::recent_sessions(RECENT_TAKE).await {
                    let _ = recent.try_set(r.list);
                }
            });
            request_animation_frame(move || interop::modal(true, input.try_get_untracked().flatten().as_ref()));
        } else if !open && was {
            interop::modal(false, None);
            cancel();
            searching.set(false);
        }
        open
    });
    // After every render while open: keep the active option in view.
    Effect::new(move |_| {
        let i = shown();
        all.track();
        if !ui.palette_open.get() {
            return;
        }
        request_animation_frame(move || {
            if let Some(l) = list.try_get_untracked().flatten() {
                interop::reveal(&l, &format!("pal-{i}"), "nearest", false);
            }
        });
    });

    let on_key = {
        let run = run.clone();
        move |e: leptos::ev::KeyboardEvent| {
            let n = all.with_untracked(Vec::len);
            if n == 0 {
                return;
            }
            match e.key().as_str() {
                "ArrowDown" => {
                    e.prevent_default();
                    sel.set((sel.get_untracked().min(n - 1) + 1) % n);
                }
                "ArrowUp" => {
                    e.prevent_default();
                    sel.set((sel.get_untracked().min(n - 1) + n - 1) % n);
                }
                "Enter" => {
                    let i = sel.get_untracked().min(n - 1);
                    if let Some(r) = all.with_untracked(|v| v.get(i).map(|r| r.run.clone())) {
                        run(r);
                    }
                }
                _ => {}
            }
        }
    };

    let groups = move || {
        let rows = all.get();
        let on = shown();
        [(0u8, "pal-g-s", "Palette.Sessions"), (1, "pal-g-a", "Palette.Actions"), (2, "pal-g-m", "Palette.Messages")]
            .into_iter()
            .filter(|(k, ..)| rows.iter().any(|r| r.kind() == *k))
            .map(|(k, gid, title)| {
                let items = rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.kind() == k)
                    .map(|(i, r)| {
                        let r = r.clone();
                        let is_on = i == on;
                        let (before, m, after) = mark(&r.name, &q.get_untracked());
                        let lead = if r.session {
                            view! { <span class=format!("dot {}", r.dot)></span> }.into_any()
                        } else {
                            view! { <span class="ic"><Icon name=r.dot.clone() sm=true/></span> }.into_any()
                        };
                        let key = if is_on && r.session {
                            Some(view! { <kbd>"⏎"</kbd> }.into_any())
                        } else {
                            r.kbd.clone().map(|k| view! { <kbd>{k}</kbd> }.into_any())
                        };
                        let right = if r.kbd.is_none() { r.right.clone().filter(|x| !x.is_empty()) } else { None };
                        let run = run.clone();
                        let action = r.run.clone();
                        view! {
                            <div id=format!("pal-{i}") class=if is_on { "pi on" } else { "pi" } role="option"
                                aria-selected=if is_on { "true" } else { "false" } on:click=move |_| run(action.clone())>
                                {lead}
                                <div>
                                    <div class="n">{before}{m.map(|m| view! { <mark>{m}</mark> })}{after}</div>
                                    {r.detail.clone().filter(|d| !d.is_empty()).map(|d| view! { <div class="d">{d}</div> })}
                                </div>
                                <span class="r">
                                    {(r.pill > 0).then(|| view! { <span class="pill wait">{r.pill}</span> })}
                                    {right}
                                    {key}
                                </span>
                            </div>
                        }
                    })
                    .collect_view();
                view! {
                    <div role="group" aria-labelledby=gid>
                        <div class="pg cap" id=gid role="presentation">{t!(title)}</div>
                        {items}
                    </div>
                }
            })
            .collect_view()
    };

    view! {
        <Show when=move || ui.palette_open.get()>
            <div class="scrim" on:click=move |_| ui.palette_open.set(false)></div>
            <div class="palette" role="dialog" aria-modal="true" aria-label=t!("Rail.Search")>
                <div class="q-in">
                    <Icon name="search"/>
                    <input node_ref=input prop:value=move || q.get()
                        on:input=move |e| {
                            let v = event_target_value(&e);
                            q.set(v.clone());
                            sel.set(0);
                            find_messages(v.trim().to_string());
                        }
                        on:keydown=on_key.clone()
                        placeholder=t!("Palette.Placeholder") aria-label=t!("Rail.Search")
                        role="combobox" aria-expanded=move || if all.read().is_empty() { "false" } else { "true" }
                        aria-controls="pal-list" aria-autocomplete="list"
                        aria-activedescendant=move || (!all.read().is_empty()).then(|| format!("pal-{}", shown()))/>
                    <kbd>{t!("Key.Esc")}</kbd>
                </div>
                <div id="pal-list" role="listbox" aria-label=t!("Rail.Search") node_ref=list>{groups.clone()}</div>
                <div role="status">
                    {move || if searching.get() {
                        view! { <div class="pg">{t!("Palette.SearchingMessages")}</div> }.into_any()
                    } else {
                        // A stopped scan is never a definitive "no results".
                        view! {
                            {all.read().is_empty().then(|| view! { <div class="pg">{t!("Palette.NoResults")}</div> })}
                            {if failed.get() {
                                Some(view! { <div class="pg">{t!("Palette.SearchFailed")}</div> })
                            } else if partial.get() {
                                Some(view! { <div class="pg">{t!("Palette.OlderNotSearched")}</div> })
                            } else {
                                None
                            }}
                        }
                        .into_any()
                    }}
                </div>
                <div class="pfoot">
                    <span><kbd>"↑"</kbd>" "<kbd>"↓"</kbd>" "{t!("Palette.Navigate")}</span>
                    <span><kbd>"⏎"</kbd>" "{t!("Palette.Open")}</span>
                </div>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::rail::tests::summary;
    use rust_decimal::Decimal;

    #[test]
    fn marks() {
        assert_eq!(mark("Fix the Parser", "parser"), ("Fix the ".into(), Some("Parser".into()), "".into()));
        assert_eq!(mark("abc", ""), ("abc".into(), None, "".into()));
        assert_eq!(mark("abc", "zz"), ("abc".into(), None, "".into()));
        // whitespace collapsed when not found as typed
        assert_eq!(mark("run the tests now", " the   tests "), ("run ".into(), Some("the tests".into()), " now".into()));
        assert_eq!(mark("Été chaud", "été"), ("".into(), Some("Été".into()), " chaud".into()));
    }

    #[test]
    fn escapes_like_uri_escape_data_string() {
        assert_eq!(escape_data("/Users/me/my repo"), "%2FUsers%2Fme%2Fmy%20repo");
        assert_eq!(escape_data("C:\\a-b_c.d~é"), "C%3A%5Ca-b_c.d~%C3%A9");
    }

    #[test]
    fn theme_cycle() {
        assert_eq!(next_theme("graphite"), "encre");
        assert_eq!(next_theme("contraste"), "graphite");
        assert_eq!(next_theme("bogus"), "graphite");
    }

    fn input<'a>(s: &'a SessionsSummary, recent: &'a [PastSession], q: &'a str, current: Option<&'a str>) -> Input<'a> {
        Input {
            summary: s,
            recent,
            hits: &[],
            q,
            theme: "graphite",
            current,
            version: "1.2.3",
            home: "/Users/me",
            sep: "/",
            now: "2026-11-07T12:00:00Z".parse().unwrap(),
            elsewhere: &|_| false,
        }
    }

    #[test]
    fn rows_order_and_actions() {
        let mut waiting = summary("w", "/r/alpha", Some("alpha"), SessionStatus::Waiting);
        waiting.pending_count = 2;
        waiting.first_pending_tool = Some("Bash".into());
        let mut idle = summary("i", "/r/beta", None, SessionStatus::Idle);
        idle.cost_usd = Decimal::new(150, 2);
        idle.can_fork = true;
        let s = SessionsSummary { sessions: vec![waiting, idle, summary("x", "/r/gone", None, SessionStatus::Exited)], ..Default::default() };
        let past = vec![PastSession {
            id: "p".into(),
            cwd: "/r/gamma".into(),
            branch: Some("main".into()),
            title: "Old talk".into(),
            cost_usd: None,
            last_write: "2026-11-07T10:00:00Z".parse().unwrap(),
            worktree_path: None,
            mode: None,
            pr_url: None,
        }];
        let r = rows(&input(&s, &past, "", Some("i")));
        // waiting first, then the idle one, then the past one; Exited live sessions are not listed
        let names: Vec<&str> = r.iter().filter(|x| x.kind() == 0).map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["w", "i", "Old talk"]);
        assert_eq!(r[0].pill, 2);
        assert_eq!(r[0].detail.as_deref(), Some(format!("alpha · {}", t!("Status.WaitingOn", "Bash")).as_str()));
        assert_eq!(r[0].right, None);
        assert_eq!(r[1].right.as_deref(), Some(t!("Status.Done").as_str()));
        assert_eq!(r[2].right.as_deref(), Some(t!("Status.History").as_str()));
        // actions: fork + stop for the current live session; new session in its cwd
        let runs: Vec<&Run> = r.iter().filter(|x| x.kind() == 1).map(|x| &x.run).collect();
        assert!(runs.contains(&&Run::Fork("i".into())));
        assert!(runs.contains(&&Run::Stop("i".into())));
        assert!(runs.contains(&&Run::Go("?cwd=%2Fr%2Fbeta".into())));
        assert!(runs.contains(&&Run::Theme));
        // a query filters sessions and actions alike
        let r = rows(&input(&s, &past, "gamma", None));
        assert_eq!(r.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Old talk"]);
        let r = rows(&input(&s, &past, "overv", None));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].run, Run::Go("overview".into()));
    }
}
