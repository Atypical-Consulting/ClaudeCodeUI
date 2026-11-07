#![warn(unused, dead_code)]
//! Port of `Components/Pages/Background.razor` — spec 06 §6.5. Owner: ui-pages-a.
//! `claude --bg` sessions (and the interactive ones running), from `claude agents --json --all`: logs, stop, respawn
//! and remove go through the CLI's own subcommands; Open shows the transcript here. Read again every 15 s.

use crate::common::icon::Icon;
use crate::interop;
use crate::ipc;
use crate::pages::new_session::{go, set_title, use_go};
use crate::state::use_app;
use ccui_shared::bg_agents::Agent;
use ccui_shared::model::BgTask;
use ccui_shared::{fmt, i18n, t, text};
use leptos::prelude::*;
use std::collections::HashSet;
use std::time::Duration;

/// The 15 s re-read keeps every known row where it was (new ones go last), so nothing moves under the pointer.
pub fn keep_order(fresh: Vec<Agent>, old: &[Agent]) -> Vec<Agent> {
    let mut v = fresh;
    v.sort_by_key(|r| old.iter().position(|o| o.id == r.id).unwrap_or(usize::MAX));
    v
}

/// A session's folder, and its repo when it is a worktree (".claude/worktrees/<folder>" of a repo).
pub fn place(cwd: &str, home: &str, sep: &str) -> (String, Option<String>) {
    let p = cwd.replace('\\', "/");
    let p = p.trim_end_matches('/');
    match p.find("/.claude/worktrees/") {
        Some(i) if i > 0 => (p[i + 19..].to_string(), Some(p[..i].rsplit('/').next().unwrap_or_default().to_string())),
        _ => {
            let f = p.rsplit('/').next().unwrap_or_default();
            (if f.is_empty() { text::short(p, home, sep) } else { f.to_string() }, None)
        }
    }
}

/// The CLI's word in the UI's language; an unknown one as the CLI says it.
pub fn state_word(s: &str) -> String {
    let key = format!("Bg.St.{s}");
    if i18n::has(&key) { t!(&key) } else { s.to_string() }
}

/// Frozen with no conversation: nothing left to resume or answer.
pub fn gone(a: &Agent, has_transcript: bool) -> bool {
    a.stale && a.session_id.is_some() && !has_transcript
}

/// Copy attach: a background session with no conversation here to Open, unless it is frozen with that conversation
/// gone.
pub fn attachable(a: &Agent, has_transcript: bool) -> bool {
    a.background() && (a.session_id.is_none() || !has_transcript) && !(a.stale && a.session_id.is_some())
}

fn local_g(t: Option<ccui_shared::Time>) -> Option<String> {
    t.map(fmt::short_date_time)
}

/// `claude --bg` agents table (bg_list every 15 s, stable order), logs/stop/respawn/rm with armed confirm.
#[component]
pub fn Background() -> impl IntoView {
    let app = use_app();
    let nav = use_go();
    set_title(&t!("Nav.Background"));
    let (home, sep) = app.boot.with_value(|b| (b.home.clone(), b.sep.clone()));

    let rows = RwSignal::new(None::<Vec<Agent>>);
    let have = RwSignal::new(HashSet::<String>::new()); // session ids that have a transcript here
    let error = RwSignal::new(None::<String>);
    let output = RwSignal::new(None::<String>);
    let shown = RwSignal::new(None::<String>);
    let cursor = RwSignal::new(None::<String>);
    let out_command = RwSignal::new(String::new());
    let armed = RwSignal::new(None::<(String, String)>);
    let running = RwSignal::new(None::<(String, String)>);
    let armed_at = StoredValue::new(0f64);
    let loading = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let failed = RwSignal::new(false);
    let daemon_up = RwSignal::new(true);
    let needs_only = RwSignal::new(false);
    let read_at = RwSignal::new(chrono::Utc::now());
    let notice = RwSignal::new(None::<String>);
    let disarm = StoredValue::new(None::<TimeoutHandle>);
    let pane_el = NodeRef::<leptos::html::Div>::new();
    let refresh_el = NodeRef::<leptos::html::Button>::new();
    let out_el = NodeRef::<leptos::html::Div>::new();

    let load = move |sort: bool| async move {
        if loading.get_untracked() {
            return;
        }
        loading.set(true);
        error.set(None);
        let got = ipc::bg_list().await;
        if rows.is_disposed() {
            return;
        }
        match got {
            Ok(list) => {
                let ids: Vec<String> = list.rows.iter().filter_map(|r| r.session_id.clone()).collect();
                let exist = ipc::transcripts_exist(&ids).await.unwrap_or_default();
                if rows.is_disposed() {
                    return;
                }
                let fresh = match rows.get_untracked() {
                    Some(old) if !sort => keep_order(list.rows, &old),
                    _ => list.rows,
                };
                if !fresh.iter().any(Agent::needs_you) {
                    needs_only.set(false);
                }
                have.set(exist.into_iter().collect());
                rows.set(Some(fresh));
                daemon_up.set(list.daemon_up);
                read_at.set(chrono::Utc::now());
            }
            Err(e) => error.set(Some(t!("Session.ActionFailed", e.trim_end().to_string()))),
        }
        loading.set(false);
    };

    leptos::task::spawn_local(load(true));
    if let Ok(h) = set_interval_with_handle(
        move || {
            if !rows.is_disposed() && !loading.get_untracked() && !busy.get_untracked() {
                leptos::task::spawn_local(load(false));
            }
        },
        Duration::from_secs(15),
    ) {
        on_cleanup(move || h.clear());
    }
    on_cleanup(move || {
        if let Some(h) = disarm.try_update_value(Option::take).flatten() {
            h.clear();
        }
    });

    let after = |f: Box<dyn FnOnce()>| request_animation_frame(f);
    let focus_row = move |id: String| {
        after(Box::new(move || {
            if let Some(p) = pane_el.try_get_untracked().flatten() {
                interop::reveal(&p, &format!("bg-{id}"), "nearest", true);
            }
        }))
    };

    let act = move |command: String, a: Agent| async move {
        if busy.get_untracked() {
            return;
        }
        let index = rows.with_untracked(|r| r.as_ref().and_then(|r| r.iter().position(|x| x.id == a.id))).unwrap_or(0);
        notice.set(None);
        busy.set(true);
        running.set(Some((a.id.clone(), command.clone())));
        armed.set(None);
        if let Some(h) = disarm.try_update_value(Option::take).flatten() {
            h.clear();
        }
        shown.set(Some(a.id.clone()));
        out_command.set(command.clone());
        let got = ipc::bg_run(&command, &a.id).await;
        if rows.is_disposed() {
            return;
        }
        let ok = match got {
            Ok(r) => {
                failed.set(!r.ok);
                // logs is terminal output: escapes stripped, \r-overwritten lines kept in their last state.
                output.set(Some(if command == "logs" {
                    BgTask::tail(&r.text, 200)
                } else if !r.text.is_empty() {
                    r.text
                } else {
                    t!(if r.ok { "Bg.Done" } else { "Bg.Failed" })
                }));
                r.ok
            }
            Err(e) => {
                failed.set(true);
                output.set(Some(t!("Session.ActionFailed", e.trim_end().to_string())));
                false
            }
        };
        busy.set(false);
        running.set(None);
        if command != "logs" {
            load(false).await;
            if rows.is_disposed() {
                return;
            }
        }
        let list = rows.get_untracked().unwrap_or_default();
        if list.iter().any(|r| r.id == a.id) {
            // Focus follows the work, never drops to body: to the output a button produced.
            after(Box::new(move || {
                if let Some(o) = out_el.try_get_untracked().flatten() {
                    let _ = o.focus();
                }
            }));
        } else {
            // The row went (rm): its output goes with it; say so, and hand focus to the row that took its place,
            // else Refresh.
            notice.set(Some(t!(if ok { "Bg.Removed" } else { "Bg.Gone2" }, a.label())));
            output.set(None);
            shown.set(None);
            match list.get(index).or(list.last()) {
                Some(next) => {
                    cursor.set(Some(next.id.clone()));
                    focus_row(next.id.clone());
                }
                None => after(Box::new(move || {
                    if let Some(b) = refresh_el.try_get_untracked().flatten() {
                        let _ = b.focus();
                    }
                })),
            }
        }
    };

    let is_armed = move |id: &str, cmd: &str| armed.with(|a| a.as_ref().is_some_and(|(i, c)| i == id && c == cmd));
    let is_running = move |id: &str, cmd: &str| running.with(|a| a.as_ref().is_some_and(|(i, c)| i == id && c == cmd));

    // rm and respawn confirm in place: the first click arms, the second runs; leaving the button or 5 s without the
    // second click disarms it.
    let confirm = move |a: Agent, command: &'static str| {
        if busy.get_untracked() {
            return;
        }
        let now = js_sys::Date::now();
        let on = armed.with_untracked(|x| x.as_ref().is_some_and(|(i, c)| *i == a.id && c == command));
        if on && now - armed_at.get_value() < 400.0 {
            return; // a double click arms, never confirms
        }
        if !on {
            armed.set(Some((a.id.clone(), command.to_string())));
            armed_at.set_value(now);
            if let Some(h) = disarm.try_update_value(Option::take).flatten() {
                h.clear();
            }
            let id = a.id.clone();
            if let Ok(h) = set_timeout_with_handle(
                move || {
                    if armed.try_get_untracked().flatten().is_some_and(|(i, c)| i == id && c == command) {
                        armed.set(None);
                    }
                },
                Duration::from_secs(5),
            ) {
                disarm.set_value(Some(h));
            }
            return;
        }
        leptos::task::spawn_local(act(command.to_string(), a));
    };
    let disarm_on = move |id: String, command: &'static str| {
        if armed.with_untracked(|x| x.as_ref().is_some_and(|(i, c)| *i == id && c == command)) {
            armed.set(None);
        }
    };

    let shown_rows = Memo::new(move |_| {
        rows.with(|r| {
            let r = r.clone().unwrap_or_default();
            if needs_only.get() && r.iter().any(Agent::needs_you) { r.into_iter().filter(Agent::needs_you).collect() } else { r }
        })
    });
    let cur = Memo::new(move |_| {
        let c = cursor.get();
        shown_rows.with(|r| if r.iter().any(|x| Some(&x.id) == c.as_ref()) { c } else { r.first().map(|x| x.id.clone()) })
    });
    let needs = Memo::new(move |_| rows.with(|r| r.as_ref().map_or(0, |r| r.iter().filter(|x| x.needs_you()).count())));

    let header = move || {
        rows.with(|r| r.as_ref().map(|r| (r.len(), r.iter().filter(|x| x.background()).count()))).map(|(n, nb)| {
            let at = read_at.get();
            let counts = format!("{} · {}", i18n::plural(n as i64, "Bg.Sessions"), t!("Bg.Split", nb, n - nb));
            let nn = needs.get();
            view! {
                <span class="sub" title=t!("Bg.ReadAt", fmt::long_time(at))>{counts}" · "{t!("Bg.ReadShort", fmt::hms(at))}</span>
                // outside the ellipsised counts: the one filter never clips
                {(nn > 0).then(|| view! {
                    <button tabindex="0" class="linkish bgneed" aria-pressed=move || if needs_only.get() { "true" } else { "false" }
                        title=move || t!(if needs_only.get() { "Bg.ShowAll" } else { "Bg.ShowNeeds" }) on:click=move |_| needs_only.update(|v| *v = !*v)>
                        {i18n::plural(nn as i64, "Bg.Needs")}
                        {move || needs_only.get().then(|| view! { <span class="bgshowing">{t!("Bg.Showing", nn, n)}</span> })}
                    </button>
                })}
            }
        })
    };

    let row = move |first: Agent| {
        let id = first.id.clone();
        let a = {
            let id = id.clone();
            Memo::new(move |prev: Option<&Agent>| {
                shown_rows.with(|r| r.iter().find(|x| x.id == id).cloned()).or_else(|| prev.cloned()).unwrap_or_else(|| first.clone())
            })
        };
        let id_s = StoredValue::new(id.clone());
        let tab = move || if cur.with(|c| c.as_deref() == Some(id_s.get_value().as_str())) { 0 } else { -1 };
        let has_t = move || a.with(|a| a.session_id.as_ref().is_some_and(|s| have.with(|h| h.contains(s))));
        let is_gone = move || gone(&a.get(), has_t());
        let (h1, s1) = (home.clone(), sep.clone());
        let label = move || a.with(Agent::label);
        let bypass = move || a.with(|a| a.mode.as_deref() == Some("bypassPermissions"));
        let bg = move || a.with(Agent::background);
        let busy_attr = move || if busy.get() { "true" } else { "false" };
        let both = move |cmd: &'static str, idle: &'static str, armed_key: &'static str| {
            let on = move || is_armed(&id_s.get_value(), cmd);
            view! {
                <span aria-hidden=move || on().then_some("true")>{move || format!("{}{}", t!(idle), if is_running(&id_s.get_value(), cmd) && !on() { "…" } else { "" })}</span>
                <span aria-hidden=move || (!on()).then_some("true")>{t!(armed_key)}</span>
            }
        };
        let verb = move |cmd: &'static str, class: &'static str, label_key: &'static str, title: Option<String>| {
            view! {
                <button class=move || if is_running(&id_s.get_value(), cmd) { format!("{class} busy") } else { class.to_string() }
                    aria-busy=move || is_running(&id_s.get_value(), cmd).then_some("true") tabindex=tab aria-disabled=busy_attr
                    aria-label=move || t!(label_key, label()) title=title
                    on:click=move |_| leptos::task::spawn_local(act(cmd.to_string(), a.get_untracked()))>
                    {t!(if cmd == "logs" { "Bg.Logs" } else { "Bg.Stop" })}{move || if is_running(&id_s.get_value(), cmd) { "…" } else { "" }}
                </button>
            }
        };
        let id2 = id.clone();
        let id3 = id.clone();
        let id4 = id.clone();
        view! {
            <tr id=format!("bg-{id}") data-r="" tabindex=tab class=move || (shown.get().as_deref() == Some(id2.as_str())).then_some("sel")
                on:focusin=move |_| cursor.set(Some(id_s.get_value()))>
                <td>{move || {
                    let a = a.get();
                    let (folder, repo) = place(&a.cwd, &h1, &s1);
                    let title = match local_g(a.started_at) {
                        Some(g) => format!("{} · {}", a.cwd, t!("Bg.StartedAt", g)),
                        None => a.cwd.clone(),
                    };
                    view! {
                        <div class="n" title=a.background().then(|| a.id.clone())>{a.label()}</div>
                        <div class="branch" title=title><span class="tag">{t!(if a.background() { "Bg.Kind.Background" } else { "Bg.Kind.Interactive" })}</span>
                            {repo.map(|r| view! { <span class="rp">{r}</span><span class="sep" aria-hidden="true">"·"</span> })}<span class="lf">{folder}</span></div>
                    }
                }}</td>
                <td>{move || {
                    let a = a.get();
                    let gone = is_gone();
                    let cls = if a.needs_you() { "st-need" } else if a.stale { "st-frozen" } else if a.working() { "st-safe" } else { "st-active" };
                    let word = if a.stale { t!("Bg.FrozenState", state_word(&a.state)) } else { state_word(&a.state) };
                    let asks = matches!(a.state.as_str(), "blocked" | "waiting").then(|| a.needs.clone().or(a.waiting_for.clone())).flatten();
                    let now = chrono::Utc::now();
                    view! {
                        <span class=format!("state {cls}")><i></i>{word}</span>
                        {asks.map(|q| {
                            let s = t!(if gone { "Bg.Asked" } else { "Bg.Asks" }, q);
                            view! { <div class="bgwhy" title=s.clone()>{s.clone()}</div> }
                        })}
                        {a.last_at.filter(|_| a.background()).map(|at| view! {
                            <div class="bgage" title=t!("Bg.LastStepAt", fmt::short_date_time(at))>{t!("Bg.LastStep", fmt::age(at, now))}
                                {gone.then(|| view! { <span title=t!("Bg.GoneVerdict")>{format!(" · {}", t!("Bg.GoneShort"))}</span> })}</div>
                        })}
                    }
                }}</td>
                <td class="num" title=move || local_g(a.with(|a| a.started_at))>{move || a.with(|a| a.started_at).map_or_else(|| "—".to_string(), |s| fmt::age(s, chrono::Utc::now()))}</td>
                <td><div class="rowacts">
                    <Show when=has_t>
                        <button class="btn" tabindex=tab data-enter="" aria-label=move || t!("Bg.OpenOf", label())
                            title=move || t!(if bg() { "Bg.OpenHint" } else { "Bg.OpenLive" })
                            on:click=move |_| if let Some(s) = a.get_untracked().session_id { go(nav, &format!("session/{s}")) }>{t!("Action.Open")}</button>
                    </Show>
                    <Show when=move || bg() && attachable(&a.get(), has_t())>
                        <button type="button" class="btn ghost" tabindex=tab data-enter="" data-copy=move || format!("claude attach {}", a.with(|a| a.id.clone()))
                            title=move || t!("Bg.AttachHint", a.with(|a| a.id.clone())) aria-label=move || t!("Bg.AttachOf", label())>{t!("Bg.Attach")}</button>
                    </Show>
                    <Show when=move || bg() && daemon_up.get()>{verb("logs", "btn ghost", "Bg.LogsOf", None)}</Show>
                    // it would resume a conversation that is no longer on disk: what it would do is unknown, so it is not offered
                    <Show when=move || bg() && !is_gone()>
                        <button class=move || if is_running(&id_s.get_value(), "respawn") { "btn ghost both busy" } else { "btn ghost both" }
                            aria-busy=move || is_running(&id_s.get_value(), "respawn").then_some("true") tabindex=tab aria-disabled=busy_attr
                            aria-label=move || t!(if is_armed(&id_s.get_value(), "respawn") { "Bg.RespawnConfirmOf" } else { "Bg.RespawnOf" }, label())
                            title=move || if bypass() { format!("{} {}", t!("Bg.RespawnHint"), t!("Bg.RespawnBypass")) } else { t!("Bg.RespawnHint") }
                            on:click=move |_| confirm(a.get_untracked(), "respawn") on:blur=move |_| disarm_on(id_s.get_value(), "respawn")>
                            {both("respawn", "Bg.Respawn", "Bg.RespawnConfirm")}
                        </button>
                    </Show>
                    // "its conversation is kept": not when there is none
                    <Show when=move || bg() && daemon_up.get() && !is_gone() && !matches!(a.with(|a| a.state.clone()).as_str(), "done" | "stopped" | "failed")>
                        {verb("stop", "btn ghost", "Bg.StopOf", Some(t!("Bg.StopHint")))}
                    </Show>
                    <Show when=bg>
                        <button class=move || if is_running(&id_s.get_value(), "rm") { "btn danger bgrm both busy" } else { "btn danger bgrm both" }
                            aria-busy=move || is_running(&id_s.get_value(), "rm").then_some("true") tabindex=tab aria-disabled=busy_attr
                            aria-label=move || t!(if is_armed(&id_s.get_value(), "rm") { "Bg.RemoveConfirmOf" } else { "Bg.RemoveOf" }, label())
                            title=t!("Bg.RemoveHint")
                            on:click=move |_| confirm(a.get_untracked(), "rm") on:blur=move |_| disarm_on(id_s.get_value(), "rm")>
                            {both("rm", "Bg.Remove", "Bg.RemoveConfirm")}
                        </button>
                    </Show>
                </div></td>
            </tr>
            {move || armed.get().filter(|(i, _)| *i == id3).map(|(_, cmd)| {
                let hint = if cmd == "rm" {
                    t!("Bg.RemoveHint")
                } else if bypass() {
                    format!("{} {}", t!("Bg.RespawnHint"), t!("Bg.RespawnBypass"))
                } else {
                    t!("Bg.RespawnHint")
                };
                view! { <tr class="bgout bgarmrow"><td colspan="4"><div class="bgarm" role="status">{hint}</div></td></tr> }
            })}
            {move || (shown.get().as_deref() == Some(id4.as_str())).then(|| output.get()).flatten().map(|out| {
                let id = id_s.get_value();
                view! {
                    <tr class="bgout"><td colspan="4">
                        <div class=move || if failed.get() { "cmdlog wfres bad" } else { "cmdlog wfres" } role="region" tabindex="-1"
                            aria-label=t!("Bg.Output", label()) node_ref=out_el>
                            <div class="bgoh"><b>{move || t!(if failed.get() { "Bg.FailedOf" } else { "Bg.OutputOf" }, out_command.get(), label())}</b>
                                <button class="btn ghost" tabindex="0" on:click=move |_| {
                                    output.set(None);
                                    shown.set(None);
                                    cursor.set(Some(id.clone()));
                                    focus_row(id.clone());
                                }>{t!("Action.Close")}</button></div>{out}
                        </div>
                    </td></tr>
                }
            })}
        }
    };

    // Only the kind of body re-renders it: the table (and the focused row in it) survives every re-read.
    let state = Memo::new(move |_| rows.with(|r| r.as_ref().map(Vec::is_empty)));
    let body = move || {
        match state.get() {
            // a failed first read says so, never "no session is running"
            None => (error.get().is_none()).then(|| view! { <p role="status">{t!("Bg.Loading")}</p> }).into_any(),
            Some(true) => view! { <p inner_html=t!("Bg.None")></p> }.into_any(),
            Some(false) => view! {
                {move || (!daemon_up.get()).then(|| {
                    // a frozen session could pick up again
                    let can_run = rows.with(|r| r.iter().flatten().any(|x| x.stale && x.session_id.as_ref().is_none_or(|s| have.with(|h| h.contains(s)))));
                    // One HTML string: the text is markup, and the copy button is the global [data-copy] handler's.
                    let run = if can_run {
                        format!(
                            " <button type=\"button\" class=\"linkish\" tabindex=\"0\" data-copy=\"claude daemon run\" title=\"{}\">{}</button>",
                            fmt::enc(&t!("Bg.DaemonRunHint")),
                            fmt::enc(&t!("Bg.DaemonRun"))
                        )
                    } else {
                        String::new()
                    };
                    let html = format!("<b>{}</b> {}{run}", fmt::enc(&t!("Bg.DaemonDown")), t!("Bg.DaemonDownText"));
                    view! { <p class=if can_run { "bgnote" } else { "bgnote flat" } role="note" inner_html=html></p> }
                })}
                <span class="vh" role="status">{move || { let n = needs.get(); if n > 0 { i18n::plural(n as i64, "Bg.Needs") } else { String::new() } }}</span>
                <table class=move || {
                    let live = shown_rows.with(|r| r.iter().any(|x| x.background() && !x.stale));
                    format!("wtt bgt {}{}", if live { "live" } else { "few" }, if error.get().is_some() { " stale" } else { "" })
                } aria-label=t!("Bg.Title")>
                    <thead><tr><th scope="col">{t!("Bg.Col.Session")}</th><th scope="col">{t!("Col.State")}</th><th scope="col" class="num">{t!("Bg.Col.Started")}</th><th scope="col"><span class="vh">{t!("Col.Actions")}</span></th></tr></thead>
                    <tbody>
                        <For each=move || shown_rows.get() key=|a| a.id.clone() children=row.clone()/>
                    </tbody>
                </table>
                <p class="ovnote bgkeys"><kbd>"J"</kbd>" "<kbd>"K"</kbd>" "{t!("Bg.KeyMove")}" · "<kbd>"↵"</kbd>" "
                    {move || t!(if shown_rows.with(|r| r.iter().any(|x| attachable(x, x.session_id.as_ref().is_some_and(|s| have.with(|h| h.contains(s)))))) { "Bg.KeyOpen" } else { "Bg.KeyOpenOnly" })}
                    " · "<kbd>"R"</kbd>" "{t!("Bg.KeyRefresh")}" · "<kbd>"Esc"</kbd>" "{t!("Bg.KeyBack")}</p>
                // the verbs are on screen: what each one does, not only in its title
                {move || shown_rows.with(|r| r.iter().any(|x| x.background() && !x.stale)).then(|| view! { <p class="ovnote bgkeys">{t!("Bg.Legend")}</p> })}
            }
            .into_any(),
        }
    };

    view! {
        <div class="pane" node_ref=pane_el>
            <div class="ph wrap">
                <h1 class="ttl">{t!("Nav.Background")}</h1>
                {header}
                <span class="end"><button class=move || if loading.get() { "btn busy" } else { "btn" } tabindex="0" aria-disabled=move || if loading.get() { "true" } else { "false" }
                    title=t!("Bg.RefreshHint") node_ref=refresh_el on:click=move |_| leptos::task::spawn_local(load(true))><Icon name="refresh" sm=true/>{t!("Wt.Refresh")}</button></span>
            </div>
            <div class="wt">
                <div class="wt-head"><div><p class="bglead" inner_html=t!("Bg.Text")></p></div></div>
                // "Removed …": the row and its output are gone, the announcement is not
                <p class="ovnote bgnotice" role="status">{move || notice.get()}</p>
                {move || error.get().map(|e| view! { <div class="errbox" role="alert">{e}</div> })}
                {body}
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str) -> Agent {
        Agent { id: id.into(), kind: "background".into(), ..Agent::default() }
    }

    #[test]
    fn order_is_kept_new_last() {
        let old = vec![agent("a"), agent("b"), agent("c")];
        let fresh = vec![agent("n"), agent("c"), agent("a")];
        let ids: Vec<String> = keep_order(fresh, &old).into_iter().map(|a| a.id).collect();
        assert_eq!(ids, ["a", "c", "n"]);
    }

    #[test]
    fn places() {
        assert_eq!(place("/r/api/.claude/worktrees/brave-x/", "/h", "/"), ("brave-x".into(), Some("api".into())));
        assert_eq!(place("C:\\r\\api", "C:\\h", "\\"), ("api".into(), None));
        assert_eq!(place("/", "/h", "/"), ("".into(), None));
    }

    #[test]
    fn verbs_offered() {
        let mut a = agent("x");
        a.session_id = Some("s".into());
        assert!(!attachable(&a, true)); // Open covers it
        assert!(attachable(&a, false));
        a.stale = true;
        assert!(gone(&a, false) && !attachable(&a, false));
        assert!(!gone(&a, true));
        a.session_id = None;
        assert!(attachable(&a, false) && !gone(&a, false));
    }
}
