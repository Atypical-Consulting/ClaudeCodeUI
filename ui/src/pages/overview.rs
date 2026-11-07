#![warn(unused, dead_code)]
//! Port of `Components/Pages/Overview.razor` — spec 06 §6.3. Owner: ui-pages-a.

use crate::common::icon::Icon;
use crate::interop;
use crate::ipc;
use crate::layout::rail::live_row;
use crate::layout::rail_session_row::dot;
use crate::pages::new_session::{go, plain_key, set_title, trim_sep, use_go};
use crate::state::use_app;
use ccui_shared::api::{AnswerArgs, Ending, PastSession, SavedLimits, Stop};
use ccui_shared::edit_diff::DiffKind;
use ccui_shared::model::{Decision, RateLimits, SessionStatus, Time};
use ccui_shared::session::{DecisionEntry, SessionSummary};
use ccui_shared::worktree_rules::{display_name, WorktreeInfo, WtState};
use ccui_shared::{ask_user, elicit, fmt, i18n, session_rules, t, text, tool_kinds};
use chrono::{Duration as Span, Local, Utc};
use leptos::prelude::*;
use rust_decimal::Decimal;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// A window this full is the page's alarm: its meter turns Ember.
const HOT_AT: f64 = 0.8;
const RISKY_AHEAD: i32 = 10;
const WT_SHOWN: usize = 5;
/// The rail's 30, cached: enough left after the probes.
const RECENT_TAKE: usize = 30;

/// A detached HEAD is not a branch name worth showing.
pub fn branch(p: &PastSession) -> Option<&str> {
    p.branch.as_deref().filter(|b| !b.is_empty() && *b != "HEAD")
}

fn substantial(e: Option<&Ending>) -> bool {
    e.is_some_and(|e| e.stop != Stop::Done || e.text.as_ref().is_some_and(|t| t.encode_utf16().count() >= 20) || e.tool.is_some())
}

/// Left out of the cards: sessions another process runs, a known cost under 5 cents, and an unknown cost that is a
/// probe (no branch, or almost nothing written).
pub fn worth(p: &PastSession, end: Option<&Ending>, away: bool) -> bool {
    !away && p.cost_usd.map_or_else(|| branch(p).is_some() && substantial(end), |c| c >= Decimal::new(5, 2))
}

/// Up to 7 sessions worth reopening (a lead, then full rows of three), newest first. The lead is the newest, unless
/// one waits on an AskUserQuestion of the last 24 h, else one interrupted or left without a reply.
pub fn pick_cards(recent: &[PastSession], endings: &HashMap<String, Ending>, away: &dyn Fn(&PastSession) -> bool, now: Time) -> Vec<PastSession> {
    let mut list: Vec<PastSession> = recent.iter().filter(|p| worth(p, endings.get(&p.id), away(p))).take(7).cloned().collect();
    if list.len() > 4 {
        list.truncate(1 + (list.len() - 1) / 3 * 3); // never an orphan card
    }
    let fresh = |p: &PastSession| now - p.last_write < Span::hours(24);
    let lead = list
        .iter()
        .position(|p| fresh(p) && endings.get(&p.id).is_some_and(|e| e.stop == Stop::Asked))
        .or_else(|| list.iter().position(|p| fresh(p) && endings.get(&p.id).is_some_and(|e| e.interrupted || e.unanswered)));
    if let Some(i) = lead {
        let p = list.remove(i);
        list.insert(0, p);
    }
    list
}

/// A plain `claude` names no session id on its command line: a fresh write says it as well, and so does a transcript
/// of a folder where a plain `claude` runs written in the last 30 min. Doubt counts as running.
pub fn away(p: &PastSession, elsewhere: bool, live_cwds: &HashSet<String>, now: Time) -> bool {
    let age = now - p.last_write;
    elsewhere || age < Span::seconds(60) || age < Span::minutes(30) && live_cwds.contains(trim_sep(&p.cwd))
}

/// Who owes the next word, in one flag.
pub fn flag_key(end: Option<&Ending>) -> Option<&'static str> {
    let e = end?;
    if e.unanswered {
        return Some("Ov.NoReply");
    }
    if e.interrupted {
        return Some("Ov.Interrupted");
    }
    match e.stop {
        Stop::Asked => Some("Ov.Asked"),
        Stop::Cut => Some("Ov.Cut"),
        // an offer, not a wait: said, so the excerpt's "?" is not a mystery
        Stop::Done if e.text.as_ref().is_some_and(|t| t.trim_end().ends_with('?')) => Some("Ov.EndsAsk"),
        _ => None,
    }
}

/// A name shrinks to at most 8 of its own characters, never further.
pub fn floor(s: &str) -> String {
    format!("calc({}ch + 1px)", s.encode_utf16().count().min(8))
}

/// A repo under the temp root is an agent's scratch: real work is never there.
/// ponytail: the C# also matched `Path.GetTempPath()`; on Windows that is %TEMP%, unknown to the webview.
pub fn scratch(repo: &str) -> bool {
    let r = repo.to_lowercase();
    ["/tmp", "/private/tmp", "/private/var/folders"].iter().any(|t| r.starts_with(&format!("{t}/")))
}

/// `-\d+$` dropped: a numbered series (worktree-x, -2, -3) is one row.
pub fn stem(s: &str) -> &str {
    let t = s.trim_end_matches(|c: char| c.is_ascii_digit());
    match t.strip_suffix('-') {
        Some(head) if t.len() < s.len() => head,
        _ => s,
    }
}

/// Worktrees holding work git cannot get back elsewhere, riskiest first; a numbered series is its riskiest member
/// plus a count.
pub fn unsaved_rows(found: Vec<WorktreeInfo>) -> Vec<(WorktreeInfo, usize)> {
    let mut v: Vec<WorktreeInfo> = found
        .into_iter()
        .filter(|w| {
            w.state == WtState::Check && !w.branch_only && (w.dirty > 0 || w.unpushed > 0 && !w.merged && !w.squash_merged)
        })
        .filter(|w| !scratch(&w.repo))
        .collect();
    v.sort_by(|a, b| b.at_risk().cmp(&a.at_risk()).then(b.dirty.cmp(&a.dirty)).then(b.last_activity.cmp(&a.last_activity)));
    let mut out: Vec<(WorktreeInfo, usize, String)> = Vec::new();
    for w in v {
        let key = stem(w.branch.as_deref().unwrap_or(&w.name)).to_string();
        match out.iter_mut().find(|(f, _, k)| f.repo == w.repo && *k == key) {
            Some((_, n, _)) => *n += 1,
            None => out.push((w, 0, key)),
        }
    }
    out.into_iter().map(|(w, n, _)| (w, n)).collect()
}

/// The window rolled over since the reading (no reset known = still live).
pub fn live(reset: Option<Time>, now: Time) -> bool {
    reset.is_none_or(|r| r > now)
}

/// Above the sessions when a window is at 80 % or more (it then decides whether to resume), below otherwise.
pub fn quota_hot(l: &RateLimits, now: Time) -> bool {
    live(l.five_hour_reset, now) && l.five_hour >= HOT_AT || live(l.seven_day_reset, now) && l.seven_day >= HOT_AT
}

/// A reset said from today: "today 23:00", "tomorrow 19:59", then the weekday.
fn soon(t: Time, now: Time) -> String {
    match (t.with_timezone(&Local).date_naive() - now.with_timezone(&Local).date_naive()).num_days() {
        0 => t!("Ov.TodayAt", fmt::time(t)),
        1 => t!("Ov.TomorrowAt", fmt::time(t)),
        _ => fmt::day(t, now),
    }
}

fn quota_window(name: String, label: String, used: f64, reset: Option<Time>, old: bool, now: Time) -> impl IntoView {
    let is_live = live(reset, now);
    let f = if is_live { used } else { 0.0 }; // the window rolled over since the reading: it starts again empty
    let pct = fmt::round_half_even(f.clamp(0.0, 1.0) * 100.0) as i64;
    let resets = reset.filter(|_| is_live).map(|r| t!("Quota.Resets", soon(r, now)));
    let reads = match &resets {
        Some(r) => format!("{}, {r}", fmt::pct(f)),
        None => fmt::pct(f),
    };
    let mut cls = String::from("qw");
    if f >= HOT_AT {
        cls.push_str(" hot");
    }
    if old {
        cls.push_str(" old");
    }
    view! {
        <div class=cls><span>{name}</span><span class="bar" role="meter" aria-label=label aria-valuemin="0" aria-valuemax="100" aria-valuenow=pct aria-valuetext=reads>
            <i style=format!("width:{pct}%")></i></span><b>{fmt::pct(f)}</b>{resets.map(|r| view! { <span class="rs">{r}</span> })}</div>
    }
}

/// The quota strip (+ its alarm line when hot).
fn quota(sl: &SavedLimits, now: Time) -> impl IntoView {
    let l = &sl.limits;
    let hot = quota_hot(l, now);
    let stale = now - sl.at > Span::hours(24);
    // "9 % of this week left.": the hottest live window, said as what remains.
    let hot_line = hot.then(|| {
        if live(l.seven_day_reset, now) && l.seven_day >= HOT_AT {
            t!("Ov.QuotaLeft", fmt::pct(1.0 - l.seven_day), t!("Ov.ThisWeek"), l.seven_day_reset.map(|r| soon(r, now)).unwrap_or_default())
        } else {
            t!("Ov.QuotaLeft", fmt::pct(1.0 - l.five_hour), t!("Ov.ThisWindow"), l.five_hour_reset.map(|r| soon(r, now)).unwrap_or_default())
        }
    });
    view! {
        <section class=if stale { "qstrip stale" } else { "qstrip" } aria-labelledby="ov-quota">
            <h3 class="cap" id="ov-quota" title=t!("Ov.QuotaHelp") aria-describedby="ov-quota-help">{t!("Quota.Usage")}</h3>
            {(!hot).then(|| view! { <span class="sr-only" id="ov-quota-help">{t!("Ov.QuotaHelp")}</span> })}
            {quota_window(t!("Quota.FiveHours"), t!("Quota.Window.five_hour"), l.five_hour, l.five_hour_reset, now - sl.at > Span::hours(1), now)}
            {quota_window(t!("Quota.SevenDays"), t!("Quota.Window.seven_day"), l.seven_day, l.seven_day_reset, stale, now)}
            <span class="asof" title=fmt::short_date_time(sl.at)>{t!(if stale { "Ov.AsOfStale" } else { "Ov.AsOf" }, fmt::age(sl.at, now))}</span>
        </section>
        // the alarm says what is left and until when: what decides whether to resume
        {hot_line.map(|h| view! { <p class="ovhelp qhelp" id="ov-quota-help">{h}</p> })}
    }
}

fn cost_span(c: Option<Decimal>) -> impl IntoView {
    match c.filter(|c| *c >= Decimal::new(5, 3)) {
        Some(c) => view! { <span class="cost">{fmt::cost(c, 2)}</span> }.into_any(),
        None => view! { <span class="cost" title=t!("Ov.CostUnknown")>"—"</span> }.into_any(),
    }
}

/// Status label and colour class of a live session.
fn state_of(s: &SessionSummary) -> (String, &'static str) {
    match s.status {
        SessionStatus::Waiting => (t!("Status.WaitingOn", tool_kinds::label(s.first_pending_tool.as_deref().unwrap_or(""))), "c-wait"),
        SessionStatus::Running => (t!("Status.Running"), "c-ok"),
        SessionStatus::Starting => (t!("Status.Starting"), "c-mute"),
        SessionStatus::Crashed => (t!("Session.ProcessStopped"), "c-err"),
        _ => (t!("Status.Done"), "c-mute"),
    }
}

fn answerable(tool: &str) -> bool {
    tool != ask_user::TOOL && tool != elicit::TOOL
}

/// Today's local midnight, 6 days back, in UTC (the week the state line sums).
fn week_start() -> Time {
    let d = Local::now().date_naive() - Span::days(6);
    d.and_hms_opt(0, 0, 0).and_then(|t| t.and_local_timezone(Local).earliest()).map_or_else(Utc::now, |t| t.with_timezone(&Utc))
}

fn url_encode(s: &str) -> String {
    String::from(js_sys::encode_uri_component(s))
}

/// Everything the snapshot fixes at once: the cards, the lead and the counts.
#[derive(Clone, Default)]
struct Snap {
    cards: Vec<PastSession>,
    endings: HashMap<String, Ending>,
    elsewhere: usize,
    elsewhere_list: Vec<PastSession>,
    left_probes: usize,
}

/// Busy mode (table + decision queue) or at rest (resume cards snapshot, quota strip, unsaved worktrees).
#[component]
pub fn Overview() -> impl IntoView {
    let app = use_app();
    let nav = use_go();
    set_title(&t!("Nav.Overview"));

    let qi = RwSignal::new(0usize);
    let error = RwSignal::new(None::<String>);
    let limits = RwSignal::new(None::<SavedLimits>);
    let week = RwSignal::new(None::<Vec<PastSession>>);
    let snap = RwSignal::new(Snap::default());
    let unsaved = RwSignal::new(None::<Vec<(WorktreeInfo, usize)>>);
    let wt_error = RwSignal::new(None::<String>);
    let loaded = RwSignal::new(false);
    let ri = RwSignal::new(0usize);
    let moved = RwSignal::new(false);
    let snap_at = RwSignal::new(Utc::now());
    let tick = RwSignal::new(0u32);
    let deltas = RwSignal::new(HashMap::<String, (usize, usize)>::new());
    let asked = StoredValue::new(HashSet::<String>::new());
    let resume_focused = StoredValue::new(false);
    let autofocused = StoredValue::new(false);
    let resume_el = NodeRef::<leptos::html::Div>::new();
    let queue_el = NodeRef::<leptos::html::Div>::new();
    let wt_el = NodeRef::<leptos::html::Div>::new();

    // Ages and the snapshot's own age move once a minute; the cards themselves never do until R.
    let now = move || {
        tick.track();
        Utc::now()
    };
    if let Ok(h) = set_interval_with_handle(move || tick.update(|t| *t = t.wrapping_add(1)), Duration::from_secs(60)) {
        on_cleanup(move || h.clear());
    }

    let busy = Memo::new(move |_| app.summary.with(|s| s.sessions.iter().any(|x| x.status != SessionStatus::Exited)));
    let active_ids = Memo::new(move |_| {
        app.summary.with(|s| s.sessions.iter().filter(|x| x.status != SessionStatus::Exited).map(|x| x.id.clone()).collect::<Vec<_>>())
    });
    let queue = Memo::new(move |_| app.summary.with(|s| s.decision_queue.clone()));
    let cards = Memo::new(move |_| snap.with(|s| s.cards.clone()));
    let n_cards = Memo::new(move |_| cards.with(Vec::len));
    let has_unsaved = Memo::new(move |_| unsaved.with(|u| u.as_ref().is_some_and(|u| !u.is_empty())));
    let hot = Memo::new(move |_| limits.with(|l| l.as_ref().is_some_and(|l| quota_hot(&l.limits, now()))));

    // Sessions and their endings land together: the lead card depends on both, so it is picked once and never swaps.
    let snapshot = move || async move {
        let recent = ipc::recent_sessions(RECENT_TAKE).await.map(|r| r.list).unwrap_or_default();
        let live_cwds: HashSet<String> = ipc::plain_claude_cwds().await.unwrap_or_default().into_iter().collect();
        let ids: Vec<String> = recent.iter().map(|p| p.id.clone()).collect();
        let endings: HashMap<String, Ending> = ipc::transcript_endings(&ids).await.unwrap_or_default().into_iter().collect();
        if snap.is_disposed() {
            return;
        }
        let at = Utc::now();
        let summary = app.summary.get_untracked();
        let is_away = |p: &PastSession| away(p, session_rules::running_elsewhere(&summary, &p.id), &live_cwds, at);
        let c = pick_cards(&recent, &endings, &is_away, at);
        let left: Vec<&PastSession> = match c.iter().map(|x| x.last_write).min() {
            Some(min) => recent.iter().filter(|p| p.last_write >= min && !c.iter().any(|x| x.id == p.id)).collect(),
            None => Vec::new(),
        };
        let s = Snap {
            elsewhere: recent.iter().filter(|p| is_away(p)).count(),
            elsewhere_list: recent.iter().filter(|p| is_away(p)).take(3).cloned().collect(),
            left_probes: left.iter().filter(|p| !is_away(p)).count(),
            cards: c,
            endings,
        };
        snap.set(s);
        loaded.set(true);
        snap_at.set(at);
        ri.set(0);
    };

    // Worktrees holding work git cannot get back elsewhere (the last scan when under 2 min old).
    let unsaved_scan = move |fresh: bool| async move {
        let cache = ipc::worktrees_cached().await.unwrap_or_default();
        let young = cache.cached_at.is_some_and(|at| Utc::now() - at < Span::minutes(2));
        let found = if !fresh && young {
            cache.rows
        } else {
            let repos = match ipc::worktrees_discover(false).await {
                Ok(r) => r,
                Err(e) => return (None, Some(e)), // git missing: say so rather than show nothing
            };
            let mut found = Vec::new();
            for r in repos {
                // one unreadable repo: the Worktrees page names it
                if let Ok(list) = ipc::worktrees_scan(&r, None).await {
                    found.extend(list);
                }
            }
            found
        };
        (Some(unsaved_rows(found)), None)
    };

    // The idle view reads transcripts and scans worktrees off the render path. Nothing above the cards depends on a
    // late part (the week only fills a sentence), so a card never moves under the pointer.
    leptos::task::spawn_local(async move {
        let l = ipc::last_limits().await.ok().flatten();
        limits.set(l);
        snapshot().await;
        let w = ipc::sessions_since(week_start()).await.ok();
        if snap.is_disposed() {
            return;
        }
        week.set(w);
        let cache_at = ipc::worktrees_cached().await.ok().and_then(|c| c.cached_at);
        let cache_age = cache_at.map_or(Span::zero(), |at| Utc::now() - at);
        let (u, e) = unsaved_scan(false).await;
        if snap.is_disposed() {
            return;
        }
        unsaved.set(u.clone());
        wt_error.set(e);
        // A cache older than 30 s rows at once, then a fresh scan patches the counts in place. Only when the order
        // holds: rows never move under the keyboard.
        if cache_age > Span::seconds(30) && cache_age < Span::minutes(2) {
            if let (Some(shown), (Some(now_rows), None)) = (u, unsaved_scan(true).await) {
                let same = shown.iter().map(|x| &x.0.path).eq(now_rows.iter().map(|x| &x.0.path));
                if same {
                    unsaved.set(Some(now_rows));
                }
            }
        }
    });

    // After render: the at-rest list takes focus once (unless focus is somewhere) and clips its tops; the queue too.
    Effect::new(move |_| {
        if busy.get() || n_cards.get() == 0 || resume_focused.get_value() {
            return;
        }
        request_animation_frame(move || {
            if let Some(el) = resume_el.try_get_untracked().flatten() {
                resume_focused.set_value(true);
                interop::focus_if_idle(&el);
                interop::clip_tops(&el);
            }
        });
    });
    Effect::new(move |_| {
        if autofocused.get_value() || queue.with(Vec::is_empty) {
            return;
        }
        request_animation_frame(move || {
            if let Some(el) = queue_el.try_get_untracked().flatten() {
                autofocused.set_value(true);
                interop::focus_if_idle(&el);
            }
        });
    });

    // "+3 −1" for an edit waiting for approval (the diff is read from disk by the backend).
    Effect::new(move |_| {
        for d in queue.get() {
            let p = d.permission;
            if !tool_kinds::is_edit(&p.tool) || asked.with_value(|a| a.contains(&p.request_id)) {
                continue;
            }
            asked.update_value(|a| {
                a.insert(p.request_id.clone());
            });
            leptos::task::spawn_local(async move {
                if let Ok(lines) = ipc::edit_diff_from_input(&p.tool, &p.input).await {
                    let add = lines.iter().filter(|l| l.kind == DiffKind::Add).count();
                    let del = lines.iter().filter(|l| l.kind == DiffKind::Del).count();
                    deltas.update(|m| {
                        m.insert(p.request_id, (add, del));
                    });
                }
            });
        }
    });

    let answer = move |d: DecisionEntry, decision: Decision| {
        // already answered: a second A/D before the re-render
        if !app.summary.with_untracked(|s| s.decision_queue.iter().any(|x| x.permission.request_id == d.permission.request_id)) {
            return;
        }
        error.set(None);
        leptos::task::spawn_local(async move {
            let args = AnswerArgs {
                request_id: d.permission.request_id.clone(),
                decision,
                mode: None,
                message: None,
                input: None,
                destination: None,
            };
            if let Err(e) = ipc::session_answer(&d.session_id, &args).await {
                error.set(Some(t!("Perm.AnswerFailed", e)));
            }
        });
    };

    let queue_key = move |e: leptos::ev::KeyboardEvent| {
        if !plain_key(&e) {
            return; // Ctrl Shift A, Ctrl A, Cmd D… are not answers
        }
        let q = queue.get_untracked();
        if q.is_empty() {
            return;
        }
        let i = qi.get_untracked().min(q.len() - 1);
        match e.key().as_str() {
            "j" | "J" | "ArrowDown" => qi.set((i + 1) % q.len()),
            "k" | "K" | "ArrowUp" => qi.set((i + q.len() - 1) % q.len()),
            "Enter" => go(nav, &format!("session/{}", q[i].session_id)),
            // the others are answered in their card
            "a" | "A" if answerable(&q[i].permission.tool) => answer(q[i].clone(), Decision::Allow),
            "d" | "D" => answer(q[i].clone(), Decision::Deny),
            _ => {}
        }
    };

    let resume_key = move |e: leptos::ev::KeyboardEvent| {
        let n = n_cards.get_untracked();
        if !plain_key(&e) || n == 0 {
            return;
        }
        moved.set(true);
        let cur = ri.get_untracked().min(n - 1);
        let wt = has_unsaved.get_untracked();
        let to_wt = move || {
            if let Some(el) = wt_el.try_get_untracked().flatten() {
                interop::focus_first(&el);
            }
        };
        let key = e.key();
        let digit = key.chars().next().filter(|c| key.len() == 1 && ('1'..='9').contains(c)).map(|c| c as usize - '1' as usize);
        match key.as_str() {
            _ if digit.is_some_and(|d| d < n) => ri.set(digit.unwrap_or(0)), // select only: ⏎ opens
            "Home" | "Escape" => ri.set(0),
            "End" => ri.set(n - 1),
            "w" | "W" if wt => to_wt(), // into the worktrees: Tab then walks them
            "j" | "J" if cur == n - 1 && wt => to_wt(), // down past the last card, as K climbs back
            "j" | "J" | "ArrowRight" => ri.set((cur + 1).min(n - 1)), // no wrap (up / down are geometric, in interop.js)
            "k" | "K" | "ArrowLeft" => ri.set(cur.saturating_sub(1)),
            "Enter" => {
                if let Some(p) = cards.with_untracked(|c| c.get(cur).map(|p| p.id.clone())) {
                    go(nav, &format!("session/{p}"));
                }
            }
            "r" | "R" => leptos::task::spawn_local(snapshot()), // a fresh snapshot: digits renumber, the lead first
            _ => {}
        }
    };

    // ------------------------------------------------------------------------------------------- at rest

    let state_line = move || {
        let s = snap.read();
        let mut parts = vec![t!("Ov.NothingHere")];
        if s.elsewhere > 0 && s.elsewhere_list.is_empty() {
            parts.push(i18n::plural(s.elsewhere as i64, "Ov.Elsewhere")); // named under the cards instead
        }
        if let Some(w) = week.get() {
            let today = Local::now().date_naive();
            let sum = |it: &mut dyn Iterator<Item = &PastSession>| it.map(|p| p.cost_usd.unwrap_or_default()).sum::<Decimal>();
            parts.push(t!("Ov.Today", fmt::cost(sum(&mut w.iter().filter(|p| p.last_write.with_timezone(&Local).date_naive() == today)), 2)));
            parts.push(t!("Ov.Week", fmt::cost(sum(&mut w.iter()), 0)));
        }
        parts.join(" · ")
    };

    let card = move |i: usize, p: PastSession, end: Option<Ending>| {
        let flag = move |end: &Option<Ending>| {
            flag_key(end.as_ref()).map(|k| view! { <span class=if k == "Ov.Asked" { "flag" } else { "flag cut" } id=format!("rc-f-{i}")>{t!(k)}</span> })
        };
        let lw = p.last_write;
        let when = move || view! { <span class="when" title=fmt::short_date_time(lw)>{move || fmt::age(lw, now())}</span> };
        let meta = {
            let c = p.cost_usd;
            move || view! { <span class="ac">{when()}<span class="sep">"·"</span>{cost_span(c)}</span> }
        };
        let excerpt = match end.as_ref() {
            Some(Ending { text: Some(said), .. }) => {
                let before = end.as_ref().is_some_and(|e| e.interrupted && !e.unanswered);
                let said = if i == 0 { said.clone() } else { text::tail(said, 110) };
                view! { <span class="ex said" id=format!("rc-x-{i}")><span>{before.then(|| view! { <span class="lbl">{format!("{} ", t!("Ov.BeforeStop"))}</span> })}{said}</span></span> }.into_any()
            }
            Some(Ending { tool: Some(tool), target, .. }) => view! {
                <span class="ex did" id=format!("rc-x-{i}")><span class="lbl">{t!("Ov.LastStep")}</span>" "
                    <span style=format!("color:{}", tool_kinds::color(tool))>{tool_kinds::label(tool)}</span>" "{tool_kinds::relative(target.as_deref().unwrap_or(""), &p.cwd)}</span>
            }
            .into_any(),
            _ => view! { <span class="ex none" id=format!("rc-x-{i}")>{if end.is_none() { t!("Ov.Unread") } else { String::new() }}</span> }.into_any(),
        };
        let repo = text::repo_name(&p.cwd);
        let b = branch(&p).map(str::to_string);
        let sel = move || ri.get() == i;
        let described = format!("{}rc-x-{i}", if flag_key(end.as_ref()).is_some() { format!("rc-f-{i} ") } else { String::new() });
        view! {
            <div role="listitem" class=if i == 0 { "rcw lead" } else { "rcw" }>
                <a class=move || if sel() { "rc sel" } else { "rc" } aria-current=move || sel().then_some("true") href=format!("session/{}", p.id) tabindex="-1"
                    aria-labelledby=format!("rc-n-{i}") aria-describedby=described>
                    <span class="rt"><kbd class="num" aria-hidden="true">{i + 1}</kbd><span class="n" id=format!("rc-n-{i}") title=p.title.clone()>{p.title.clone()}</span>
                        {(i == 0).then(|| flag(&end))}{(i > 0).then(meta)}</span>
                    {excerpt}
                    // its own line on grid cards: it never competes with the title or the meta
                    {(i > 0).then(|| flag(&end))}
                    <span class="rm">{(i == 0).then(meta)}<span class="branch">
                        {b.map(|b| view! { <Icon name="branch"/><span class="bn" title=b.clone() style=format!("min-width:{}", floor(&b))>{b.clone()}</span><span class="sep">"·"</span> })}
                        <span class="rp" title=repo.clone() style=format!("min-width:{}", floor(&repo))>{repo.clone()}</span></span>
                        // the lead keeps an invisible copy while the cursor is elsewhere, so its height never changes
                        <span class="slot"><span class=move || if sel() { "go btn primary" } else { "go btn primary off" } aria-hidden="true">{t!("Ov.ResumeAction")}</span></span></span>
                </a>
            </div>
        }
    };

    let cards_section = move || {
        view! {
            <section class="ovs" aria-labelledby="ov-recent">
                <div class="ovcap"><h3 class="cap" id="ov-recent">{t!("Ov.Recent")}</h3>
                    <span class="keys" id="ov-keys" title=t!("Ov.MoreKeys")>
                        <span class=move || if ri.get() == 0 { "esc off" } else { "esc" }><span class="k"><kbd>"Esc"</kbd>" "{t!("Ov.First")}</span>" · "</span>
                        <span class="k"><kbd>"1"</kbd>"–"<kbd>{move || n_cards.get()}</kbd>" "<kbd>"J"</kbd>" "<kbd>"K"</kbd>" "{t!("Ov.Select")}</span>" · "
                        <span class="k"><kbd>"Enter"</kbd>" "{t!("Ov.ResumeKey")}</span>" · "<span class="k"><kbd>"R"</kbd>" "{t!("Ov.Reread")}</span>
                        {move || has_unsaved.get().then(|| view! { " · "<span class="k"><kbd>"W"</kbd>" "{t!("Ov.WtKey")}</span> })}
                    </span></div>
                <div class=move || match n_cards.get() { 2 => "resume g1", 3 => "resume g2", _ => "resume" } role="list" tabindex="0" node_ref=resume_el
                    aria-labelledby="ov-recent" aria-describedby="ov-keys" data-esc-home=move || (ri.get() != 0).then_some("1") on:keydown=resume_key>
                    {move || {
                        let s = snap.get();
                        s.cards.into_iter().enumerate().map(|(i, p)| {
                            let end = s.endings.get(&p.id).cloned();
                            card(i, p, end)
                        }).collect_view()
                    }}
                </div>
                // The J/K cursor is visual; this announces it (silent on arrival: the focused list names itself).
                <div class="sr-only" role="status">{move || moved.get().then(|| {
                    let n = n_cards.get();
                    let i = ri.get().min(n.saturating_sub(1));
                    format!("{} · {}", t!("OfN", i + 1, n), cards.with(|c| c.get(i).map(|p| p.title.clone()).unwrap_or_default()))
                })}</div>
                {move || {
                    let s = snap.read();
                    (!s.elsewhere_list.is_empty()).then(|| {
                        let more = s.elsewhere.saturating_sub(s.elsewhere_list.len());
                        // Named, and still reachable: opening one shows its transcript and warns before a second
                        // process could start.
                        view! {
                            <p class="ovnote">{format!("{} ", t!("Ov.ElsewhereList"))}
                                {s.elsewhere_list.iter().enumerate().map(|(k, e)| {
                                    let title = if e.title.chars().count() <= 40 { e.title.clone() } else { format!("{}…", e.title.chars().take(39).collect::<String>()) };
                                    view! { {(k > 0).then_some("\u{a0}· ")}<a href=format!("session/{}", e.id) tabindex="0" title=t!("Status.ElsewhereLine")>{title}</a> }
                                }).collect_view()}
                                {(more > 0).then(|| format!(" · +{more}"))}</p>
                        }
                    })
                }}
                {move || {
                    let n = snap.with(|s| s.left_probes);
                    (n > 0).then(|| view! { <p class="ovnote">{t!("Ov.LeftOut", i18n::plural(n as i64, "Ov.LeftProbe"), i18n::kbd("Ctrl K").replace(' ', ""))}</p> })
                }}
                // Frozen on purpose (a digit names what it showed), never silently: an open tab says how old its cards are.
                {move || (now() - snap_at.get() > Span::minutes(2)).then(|| view! {
                    <p class="ovnote">{t!("Ov.SnapAge", fmt::age(snap_at.get(), now()))}" "<kbd>"R"</kbd>" "{t!("Ov.SnapRefresh")}</p>
                })}
            </section>
        }
    };

    let worktrees_section = move || {
        let list = unsaved.get().unwrap_or_default();
        let count = list.len();
        view! {
            <section class="ovs" aria-labelledby="ov-wt">
                <div class="ovcap"><h3 class="cap" id="ov-wt" title=t!("Ov.WtHelp") aria-describedby="ov-wt-help">{t!("Ov.Wt")}
                    {(count > WT_SHOWN).then(|| view! { <span class="of">" · "{t!("OfN", WT_SHOWN, count)}</span> })}</h3>
                    <a href="worktrees" tabindex="0">{t!("Ov.WtAll")}" →"</a></div>
                <p class="ovhelp" id="ov-wt-help">{t!("Ov.WtHelp")}<span class="wtkeys">" · "<kbd>"J"</kbd>" "<kbd>"K"</kbd>" "{t!("Ov.WtKeys")}" · "<kbd>"Esc"</kbd>" "{t!("Ov.WtBack")}</span></p>
                <div class="ovwt" node_ref=wt_el>
                    {list.into_iter().take(WT_SHOWN).map(|(w, more)| {
                        let name = display_name(&w).to_string();
                        let lw = w.last_activity;
                        let cls = if w.at_risk() >= RISKY_AHEAD { "wtr risky" } else { "wtr" };
                        view! {
                            <a class=cls href=format!("worktrees?path={}", url_encode(&w.path)) title=w.path.clone() tabindex="0">
                                <span class="n"><span class="nm" title=name.clone()>{name.clone()}</span>{(more > 0).then(|| view! { <span class="more">"+"{more}</span> })}</span>
                                <span class="r">{w.repo.rsplit(['/', '\\']).next().unwrap_or_default().to_string()}</span>
                                // the Worktrees page's own sentence: the row and the page it opens say the same
                                <span class="why" inner_html=w.why.clone()></span>
                                <span class="r">{move || lw.map(|t| fmt::age(t, now())).unwrap_or_default()}</span>
                            </a>
                        }
                    }).collect_view()}
                </div>
            </section>
        }
    };

    let at_rest = move || {
        view! {
            <div class="empty">
                <h2>{move || t!(if n_cards.get() > 0 { "Ov.Resume" } else { "Ov.Empty" })}</h2>
                <p class="lead one">{state_line}</p>
                {move || (n_cards.get() == 0).then(|| view! {
                    <p class="lead">{t!("Ov.EmptyText")}</p>
                    <a class="btn primary start" href=""><Icon name="plus" sm=true/>{t!("Rail.NewSession")}<kbd>{i18n::kbd("Alt N")}</kbd></a>
                })}
            </div>
            {move || hot.get().then(|| limits.get().map(|l| quota(&l, now())))}
            {move || (n_cards.get() > 0).then(cards_section)}
            {move || (!hot.get()).then(|| limits.get().map(|l| quota(&l, now())))}
            {move || {
                if has_unsaved.get() {
                    worktrees_section().into_any()
                } else if let Some(e) = wt_error.get() {
                    view! {
                        <p class="ovnote">{t!("Ov.WtFailed", e)}" "<button type="button" class="btn ghost" on:click=move |_| {
                            wt_error.set(None);
                            leptos::task::spawn_local(async move {
                                let (u, e) = unsaved_scan(false).await;
                                unsaved.set(u);
                                wt_error.set(e);
                            });
                        }>{t!("Wt.Refresh")}</button></p>
                    }
                    .into_any()
                } else if unsaved.with(Option::is_none) {
                    view! { <p class="ovnote" role="status">{t!("Ov.WtScanning")}</p> }.into_any()
                } else {
                    ().into_any()
                }
            }}
        }
    };

    // ------------------------------------------------------------------------------------------- busy

    let session_row = move |id: String| {
        let first = app.summary.with_untracked(|s| s.sessions.iter().find(|x| x.id == id).cloned());
        let Some(first) = first else { return ().into_any() };
        let s = live_row(app, first);
        let url = format!("session/{id}");
        let url2 = url.clone();
        view! {
            <tr class=move || (s.with(|s| s.status) == SessionStatus::Waiting).then_some("hot") on:click=move |_| go(nav, &url)>
                <td>{move || {
                    let s = s.get();
                    let place = [s.branch.clone(), s.repo.clone()].into_iter().flatten().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
                    let url = url2.clone();
                    view! {
                        <div class="n" title=s.name.clone()><a href=url on:click=|e| e.stop_propagation()>{s.name.clone()}</a></div>
                        {(!place.is_empty()).then(|| view! { <div class="branch"><Icon name="branch"/>{place}</div> })}
                        {s.todos.filter(|(_, n)| *n > 0).map(|(d, n)| view! { <div class="branch" title=t!("Todo.Title")><Icon name="check"/>{t!("Todo.Progress", d, n)}</div> })}
                    }
                }}</td>
                <td>{move || {
                    let s = s.get();
                    let (label, cls) = state_of(&s);
                    view! { <span class=format!("stt {cls}")><span class=format!("dot {}", dot(s.status))></span>{label}</span> }
                }}</td>
                <td><span class="mtag">{move || i18n::name("Mode.Name.", Some(&s.with(|s| s.mode.clone())))}</span></td>
                <td>{move || {
                    let s = s.get();
                    // Pending tool first, else the last top-level tool; a crash shows the exit line.
                    if s.status == SessionStatus::Crashed {
                        let txt = if s.start_failed { t!("Crash.StartFailed.Title") } else { t!("Session.Exited", s.exit_code.unwrap_or(0)) };
                        return view! { <span class="last c-err-fg">{txt}</span> }.into_any();
                    }
                    match s.last_tool {
                        None => view! { <span class="last"></span> }.into_any(),
                        Some(t) => view! {
                            <span class="last"><span style=format!("color:{}", tool_kinds::color(&t.name)) title=t.name.clone()>{tool_kinds::label(&t.name)}</span>" "{tool_kinds::target(&t.name, &t.input, &s.cwd)}</span>
                        }
                        .into_any(),
                    }
                }}</td>
                <td class="num">{move || s.with(|s| s.tool_count)}</td>
                <td class="num">{move || s.with(|s| fmt::dur((s.last_event_at - s.started_at).num_milliseconds()))}</td>
                <td class="num">{move || s.with(|s| fmt::cost(s.cost_usd, 2))}</td>
            </tr>
        }
        .into_any()
    };

    let queue_row = move |d: DecisionEntry| {
        let rid = d.permission.request_id.clone();
        let entry = {
            let rid = rid.clone();
            Memo::new(move |prev: Option<&DecisionEntry>| {
                queue.with(|q| q.iter().find(|x| x.permission.request_id == rid).cloned()).or_else(|| prev.cloned()).unwrap_or_else(|| d.clone())
            })
        };
        let sel = {
            let rid = rid.clone();
            move || queue.with(|q| q.iter().position(|x| x.permission.request_id == rid)) == Some(qi.get().min(queue.with(Vec::len).saturating_sub(1)))
        };
        let sel2 = sel.clone();
        let e = entry.get_untracked();
        let p = e.permission.clone();
        let delta = {
            let rid = rid.clone();
            move || {
                deltas.with(|m| m.get(&rid).copied()).filter(|(a, d)| a + d > 0).map(|(a, d)| view! { " "<span style="color:var(--fg-4)">{format!("+{a} −{d}")}</span> })
            }
        };
        view! {
            <div class=move || if sel() { "q sel" } else { "q" } aria-current=move || sel2().then_some("true")>
                <span class="who" title=e.session_name.clone()>{e.session_name.clone()}</span>
                <span class="k" style=format!("color:{}", tool_kinds::color(&p.tool)) title=p.tool.clone()>{tool_kinds::label(&p.tool)}</span>
                <span class="p">{tool_kinds::target(&p.tool, &p.input, &e.session_cwd)}{delta}</span>
                // Enter on a button must not also open the session
                <span class="qa" on:keydown=|e| e.stop_propagation()>
                    // answered through its form in the session, never a bare allow
                    {answerable(&p.tool).then(|| view! {
                        <button type="button" class="btn primary" on:click=move |_| answer(entry.get_untracked(), Decision::Allow)>{t!("Perm.Allow")}</button>
                    })}
                    <button type="button" class="btn danger" on:click=move |_| answer(entry.get_untracked(), Decision::Deny)>{t!("Perm.Deny")}</button>
                    <a class="btn" href=format!("session/{}", e.session_id)>{t!("Action.Open")}</a>
                </span>
            </div>
        }
    };

    let queue_view = move || {
        let current = move || queue.with(|q| q.get(qi.get().min(q.len().saturating_sub(1))).cloned());
        view! {
            <div class="queue" role="group" tabindex="0" node_ref=queue_el aria-label=t!("Ov.Queue") aria-describedby="q-keys" on:keydown=queue_key>
                <div class="qh"><span class="cap">{t!("Ov.Queue")}</span><span id="q-keys" class="keys"><kbd>"J"</kbd>" "<kbd>"K"</kbd>" "{t!("Palette.Navigate")}" · "
                    {move || current().filter(|c| answerable(&c.permission.tool)).map(|_| view! { <kbd>"A"</kbd>" "{t!("Perm.HintAllow")}" · " })}
                    <kbd>"D"</kbd>" "{t!("Perm.HintDeny")}" · "<kbd>"Enter"</kbd>" "{t!("Palette.Open")}</span></div>
                // The J/K cursor is visual; this announces it to screen readers.
                <div class="sr-only" role="status">{move || current().map(|c| {
                    let n = queue.with(Vec::len);
                    let p = &c.permission;
                    format!("{} · {} · {} {}", t!("OfN", qi.get().min(n.saturating_sub(1)) + 1, n), c.session_name, p.tool, tool_kinds::target(&p.tool, &p.input, &c.session_cwd))
                })}</div>
                {move || error.get().map(|e| view! { <div class="errbox" role="alert" style="margin:10px 16px">{e}</div> })}
                <For each=move || queue.get() key=|d| d.permission.request_id.clone() children=queue_row/>
            </div>
        }
    };

    let busy_view = move || {
        view! {
            <div class="ovhead">
                <h2>{move || {
                    let n = active_ids.with(Vec::len) as i64;
                    let q = queue.with(Vec::len) as i64;
                    format!("{}{}", i18n::plural(n, "Ov.Sessions"), if q > 0 { format!(", {}", i18n::plural(q, "Ov.Decision")) } else { String::new() })
                }}</h2>
                {move || {
                    let q = queue.with(Vec::len);
                    let (running, spent, tools) = app.summary.with(|s| {
                        let act = s.sessions.iter().filter(|x| x.status != SessionStatus::Exited);
                        // reopened sessions bring their lifetime cost: only this run counts
                        act.fold((0, Decimal::ZERO, 0i64), |(r, c, t), x| {
                            (r + usize::from(x.status == SessionStatus::Running), c + x.cost_usd - x.cost_at_open, t + i64::from(x.tool_count))
                        })
                    });
                    view! {
                        <div class="tot">
                            <div class=(q > 0).then_some("w")><b>{q}</b>{t!("Ov.Pending")}</div>
                            <div><b>{running}</b>{t!("Status.Running")}</div>
                            <div><b>{fmt::cost(spent, 2)}</b>{t!("Ov.ThisRun")}</div>
                            <div><b>{tools}</b>{t!("Kv.Tools")}</div>
                        </div>
                    }
                }}
            </div>
            <table class="ovt">
                <thead><tr><th>{t!("Session.Session")}</th><th>{t!("Col.State")}</th><th>{t!("Col.Mode")}</th><th>{t!("Ov.Col.Last")}</th>
                    <th class="num">{t!("Col.Tools")}</th><th class="num">{t!("Ov.Col.Duration")}</th><th class="num">{t!("Ov.Col.Cost")}</th></tr></thead>
                <tbody><For each=move || active_ids.get() key=|id| id.clone() children=session_row/></tbody>
            </table>
            {move || (!queue.with(Vec::is_empty)).then(queue_view)}
        }
    };

    let has_queue_or = Memo::new(move |_| (busy.get(), loaded.get()));

    view! {
        <div class="pane">
            <div class="ph"><h1 class="ttl">{t!("Nav.Overview")}</h1>
                {move || busy.get().then(|| view! {
                    <span class="end"><a class="btn primary" href=""><Icon name="plus" sm=true/>{t!("Rail.NewSession")}<kbd>{i18n::kbd("Alt N")}</kbd></a></span>
                })}</div>
            <div class="ov">
                {move || match has_queue_or.get() {
                    (true, _) => busy_view().into_any(),
                    // No headline until the transcripts are read: "no session yet" for a few hundred ms would be a
                    // wrong empty state.
                    (false, false) => view! { <p class="ovnote" role="status">{t!("Ov.Reading")}</p> }.into_any(),
                    (false, true) => at_rest().into_any(),
                }}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn past(id: &str, mins_ago: i64, cost: Option<&str>, branch: Option<&str>, now: Time) -> PastSession {
        PastSession {
            id: id.into(),
            cwd: "/r/app".into(),
            branch: branch.map(Into::into),
            title: id.into(),
            cost_usd: cost.map(|c| c.parse().unwrap()),
            last_write: now - Span::minutes(mins_ago),
            worktree_path: None,
            mode: None,
            pr_url: None,
        }
    }

    #[test]
    fn cards_lead_and_rows_of_three() {
        let now = Utc.with_ymd_and_hms(2026, 11, 7, 12, 0, 0).unwrap();
        let recent: Vec<PastSession> = (0..9).map(|i| past(&format!("s{i}"), 10 + i * 10, Some("0.10"), None, now)).collect();
        let mut endings = HashMap::new();
        endings.insert("s3".to_string(), Ending { stop: Stop::Asked, ..Ending::default() });
        let none = |_: &PastSession| false;
        let c = pick_cards(&recent, &endings, &none, now);
        // 7 worth, truncated to 1 + 2*3 = 7; the AskUserQuestion leads
        assert_eq!(c.len(), 7);
        assert_eq!(c[0].id, "s3");
        // 5 → 4 (a lead, then a row of three)
        let c = pick_cards(&recent[..5], &HashMap::new(), &none, now);
        assert_eq!(c.len(), 4);
        // cheap and probe sessions are left out; an unknown cost counts with a branch and a substantial ending
        let cheap = past("c", 5, Some("0.01"), None, now);
        let probe = past("p", 5, None, None, now);
        let real = past("r", 5, None, Some("feat"), now);
        let mut e = HashMap::new();
        e.insert("r".to_string(), Ending { tool: Some("Edit".into()), ..Ending::default() });
        let c = pick_cards(&[cheap, probe, real], &e, &none, now);
        assert_eq!(c.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["r"]);
    }

    #[test]
    fn away_rules() {
        let now = Utc.with_ymd_and_hms(2026, 11, 7, 12, 0, 0).unwrap();
        let mut cwds = HashSet::new();
        cwds.insert("/r/app".to_string());
        let mut p = past("a", 0, None, None, now);
        p.last_write = now - Span::seconds(30);
        assert!(away(&p, false, &HashSet::new(), now));
        p.last_write = now - Span::minutes(20);
        assert!(!away(&p, false, &HashSet::new(), now));
        assert!(away(&p, false, &cwds, now));
        p.last_write = now - Span::minutes(40);
        assert!(!away(&p, false, &cwds, now));
        assert!(away(&p, true, &cwds, now));
    }

    #[test]
    fn flags() {
        let e = |stop, text: &str, un, int| Ending { text: Some(text.into()), stop, unanswered: un, interrupted: int, ..Ending::default() };
        assert_eq!(flag_key(None), None);
        assert_eq!(flag_key(Some(&e(Stop::Asked, "x", true, true))), Some("Ov.NoReply"));
        assert_eq!(flag_key(Some(&e(Stop::Asked, "x", false, true))), Some("Ov.Interrupted"));
        assert_eq!(flag_key(Some(&e(Stop::Asked, "x", false, false))), Some("Ov.Asked"));
        assert_eq!(flag_key(Some(&e(Stop::Cut, "x", false, false))), Some("Ov.Cut"));
        assert_eq!(flag_key(Some(&e(Stop::Done, "Shall I? ", false, false))), Some("Ov.EndsAsk"));
        assert_eq!(flag_key(Some(&e(Stop::Done, "Done.", false, false))), None);
    }

    fn wt(path: &str, branch: &str, dirty: i32, unpushed: i32) -> WorktreeInfo {
        WorktreeInfo {
            repo: "/r".into(),
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            branch: Some(branch.into()),
            head: None,
            state: WtState::Check,
            why: String::new(),
            size_bytes: None,
            last_activity: None,
            locked: false,
            lock_reason: None,
            dirty,
            unpushed,
            merged: false,
            squash_merged: false,
            upstream_gone: false,
            branch_only: false,
            unreadable: None,
            pushed: None,
            recent_min: None,
            kept: None,
        }
    }

    #[test]
    fn unsaved_series_fold() {
        let mut merged = wt("/r/m", "m", 0, 3);
        merged.merged = true;
        let mut tmp = wt("/r/t", "t", 2, 0);
        tmp.repo = "/private/tmp/x".into();
        let rows = unsaved_rows(vec![wt("/r/a", "worktree-x", 1, 0), wt("/r/b", "worktree-x-2", 0, 12), merged, tmp, wt("/r/c", "c", 3, 0)]);
        let got: Vec<(&str, usize)> = rows.iter().map(|(w, n)| (w.path.as_str(), *n)).collect();
        assert_eq!(got, [("/r/b", 1), ("/r/c", 0)]);
        assert_eq!(stem("worktree-x-12"), "worktree-x");
        assert_eq!(stem("v2"), "v2");
        assert_eq!(stem("x-"), "x-");
    }

    #[test]
    fn small_rules() {
        assert_eq!(floor("main"), "calc(4ch + 1px)");
        assert_eq!(floor("a-very-long-branch"), "calc(8ch + 1px)");
        assert!(scratch("/private/var/folders/xy/repo") && !scratch("/tmpx/repo"));
        let now = Utc.with_ymd_and_hms(2026, 11, 7, 12, 0, 0).unwrap();
        let l = RateLimits { five_hour: 0.9, five_hour_reset: Some(now - Span::minutes(1)), seven_day: 0.5, ..RateLimits::default() };
        assert!(!quota_hot(&l, now)); // rolled over
        assert!(quota_hot(&RateLimits { five_hour_reset: None, ..l }, now));
    }
}
