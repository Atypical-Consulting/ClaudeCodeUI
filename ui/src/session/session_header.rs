//! Port of `Components/Session/SessionHeader.razor` — spec 07 §26. Owner: ui-session-thread-composer.

use crate::common::icon::Icon;
use crate::interop;
use crate::ipc;
use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::model::SessionStatus;
use ccui_shared::session::SessionMeta;
use ccui_shared::{i18n, t, text};
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;

/// `#42` for a PR URL; the URL's last segment otherwise.
pub fn pr_label(url: &str) -> String {
    match text::pr_number(url) {
        Some(n) => format!("#{n}"),
        None => url.trim_end_matches('/').rsplit('/').next().unwrap_or(url).to_string(),
    }
}

/// The short facts first so the ellipsis eats the path (whose full form is in the title and the Copy button).
pub fn facts(m: &SessionMeta, home: &str, sep: &str) -> String {
    [m.branch.clone(), m.repo.clone(), Some(i18n::name("Mode.Name.", Some(&m.mode))), Some(text::short(&m.cwd, home, sep))]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Title (rename), branch/worktree, PR, remote, fork, stop, export, inspector toggle.
#[component]
pub fn SessionHeader(session: SessionHandle) -> impl IntoView {
    let ui = use_ui();
    let app = use_app();
    // Cloned into each task: the header may be gone (SessionRemoved remounts the page) when the call returns.
    let navigate = use_navigate();
    let (home, sep) = app.boot.with_value(|b| (b.home.clone(), b.sep.clone()));
    let renaming = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let draft = RwSignal::new(String::new());
    let note = RwSignal::new(None::<String>);
    let name_input = NodeRef::<leptos::html::Input>::new();
    let rename_btn = NodeRef::<leptos::html::Button>::new();
    // Some(true): into the input once rendered, Some(false): back to the Rename button.
    let focus_name = RwSignal::new(None::<bool>);
    Effect::new(move |_| {
        let Some(to_input) = focus_name.get() else { return };
        let done = if to_input {
            name_input.get().map(|i| interop::focus(&i)).is_some()
        } else {
            rename_btn.get().map(|b| interop::focus(&b)).is_some()
        };
        if done {
            focus_name.set(None);
        }
    });

    let m = move |f: fn(&SessionMeta) -> bool| session.meta.with(f);
    let id = StoredValue::new(session.id());

    // One action at a time; a failure shows in the header's status line, the session is unchanged.
    let run = move |act: std::pin::Pin<Box<dyn std::future::Future<Output = Result<Option<String>, String>>>>| {
        busy.set(true);
        note.set(None);
        leptos::task::spawn_local(async move {
            match act.await {
                Ok(n) => {
                    if n.is_some() {
                        let _ = note.try_set(n);
                    }
                }
                Err(e) => {
                    let _ = note.try_set(Some(t!("Session.ActionFailed", e.trim_end())));
                }
            }
            let _ = busy.try_set(false);
        });
    };

    let start_rename = move |_| {
        draft.set(session.meta.with_untracked(|m| m.name.clone()));
        renaming.set(true);
        focus_name.set(Some(true));
    };
    let commit = move || {
        if !renaming.get_untracked() {
            return; // the blur that follows Enter
        }
        renaming.set(false);
        focus_name.set(Some(false));
        let name = draft.get_untracked().trim().to_string();
        if name.is_empty() || session.meta.with_untracked(|m| m.name == name) {
            return;
        }
        let id = id.get_value();
        run(Box::pin(async move { ipc::session_rename(&id, &name).await.map(|_| None) }));
    };
    let name_keys = move |e: leptos::ev::KeyboardEvent| match e.key().as_str() {
        "Enter" => commit(),
        "Escape" => {
            renaming.set(false);
            focus_name.set(Some(false));
        }
        _ => {}
    };
    let toggle_remote = move |_| {
        let on = session.meta.with_untracked(|m| m.remote_url.is_none());
        let id = id.get_value();
        run(Box::pin(async move { ipc::session_set_remote(&id, on).await.map(|_| None) }));
    };
    let export = move |_| {
        let id = id.get_value();
        let (home, sep) = (home.clone(), sep.clone());
        run(Box::pin(async move {
            ipc::session_export(&id).await.map(|path| Some(t!("Session.Exported", text::short(&path, &home, &sep))))
        }));
    };
    let nav = navigate.clone();
    let stop = move |_| {
        let id = id.get_value();
        let nav = nav.clone();
        leptos::task::spawn_local(async move {
            if let Err(e) = ipc::session_stop(&id).await {
                leptos::logging::error!("session_stop({id}): {e}");
            }
            nav("/overview", NavigateOptions::default());
        });
    };
    let fork = move |_| {
        let id = id.get_value();
        let nav = navigate.clone();
        leptos::task::spawn_local(async move {
            match ipc::session_fork(&id, None).await {
                Ok(new) => nav(&format!("/session/{new}"), NavigateOptions::default()),
                Err(e) => {
                    let _ = note.try_set(Some(t!("Session.ActionFailed", e.trim_end())));
                }
            }
        });
    };
    // The panel takes the Inspector's place of the context, so a selected tool is let go and a hidden Inspector shown.
    let toggle_diff = move |_| {
        let open = !ui.diff_open.get_untracked();
        ui.diff_open.set(open);
        if open {
            ui.selected_tool_id.set(None);
            ui.inspector_open.set(true);
        }
    };

    let title = move || {
        if renaming.get() {
            view! {
                // /rename's twin: Enter saves, Escape or an empty name keeps the old one.
                <input class="ttl-edit" node_ref=name_input prop:value=move || draft.get()
                    on:input=move |e| draft.set(event_target_value(&e)) on:keydown=name_keys on:blur=move |_| commit()
                    aria-label=t!("Session.RenameLabel") maxlength="200" spellcheck="false" autocomplete="off"/>
            }
            .into_any()
        } else {
            let name = move || session.meta.with(|m| m.name.clone());
            view! {
                <h1 class="ttl" tabindex="-1" title=name>{name}</h1>
                <button type="button" class="btn ghost" node_ref=rename_btn title=t!("Session.RenameHint")
                    aria-label=t!("Session.RenameLabel") on:click=start_rename>{t!("Session.Rename")}</button>
            }
            .into_any()
        }
    };
    let (home2, sep2) = app.boot.with_value(|b| (b.home.clone(), b.sep.clone()));
    // Memos: the meta is replaced on every patch, and a link rebuilt under the pointer would swallow its click.
    let pr_url = Memo::new(move |_| session.meta.with(|m| m.pr_url.clone()));
    let add_dirs = Memo::new(move |_| session.meta.with(|m| m.add_dirs.clone()));
    let flags = Memo::new(move |_| session.meta.with(|m| (m.restricted, m.safe_mode)));
    let remote_url = Memo::new(move |_| session.meta.with(|m| m.remote_url.clone()));
    let pr = move || {
        pr_url.get().map(|pr| {
            let label = pr_label(&pr);
            let (href, title, aria) = (pr.clone(), pr.clone(), t!("Session.PrOf", label));
            view! { <a class="chip" href=href target="_blank" rel="noopener noreferrer" title=title aria-label=aria><Icon name="branch" sm=true/>{label}</a> }
        })
    };
    let dirs = move || {
        let d = add_dirs.get();
        (!d.is_empty()).then(|| {
            let title = d.join("\n");
            view! { <span class="sub dirs-n" title=title>{i18n::plural(d.len() as i64, "Dirs.Count")}</span> }
        })
    };
    let tag = move || {
        let (restricted, safe) = flags.get();
        (restricted || safe).then(|| {
            let (hint, label) = if restricted { ("New.RestrictedHint", "New.Restricted") } else { ("New.SafeModeHint", "New.SafeMode") };
            view! { <span class="tag" title=t!(hint)>{t!(label)}</span> }
        })
    };
    let cwd = move || session.meta.with(|m| m.cwd.clone());
    let has_cwd = move || session.meta.with(|m| !m.cwd.is_empty());
    view! {
        <div class="ph">
            {title}
            {pr}
            <span class="sub" title=cwd>{move || session.meta.with(|m| facts(m, &home2, &sep2))}</span>
            {dirs}
            {tag}
            <Show when=has_cwd>
                <button type="button" class="btn ghost" data-copy=cwd title=t!("Session.CopyCwd") aria-label=t!("Session.CopyCwd")>
                    <Icon name="copy" sm=true/>{t!("Md.Copy")}</button>
            </Show>
            <span class="end">
                {move || note.get().map(|n| view! { <span class="sub" role="status">{n}</span> })}
                {move || remote_url.get().map(|ru| {
                    let (href, title) = (ru.clone(), ru);
                    view! { <a class="btn ghost" href=href target="_blank" rel="noopener noreferrer" title=title>{t!("Remote.Open")}</a> }
                })}
                <Show when=move || m(|m| m.has_process && m.remote_available)>
                    <button type="button" class="btn ghost" title=t!("Remote.Hint")
                        aria-pressed=move || if session.meta.with(|m| m.remote_url.is_none()) { "false" } else { "true" }
                        disabled=move || busy.get() on:click=toggle_remote>{t!("Remote.Label")}</button>
                </Show>
                <button type="button" class="btn ghost" title=t!("Session.ExportHint")
                    disabled=move || busy.get() || session.items.with(Vec::is_empty) on:click=export>{t!("Session.Export")}</button>
                <Show when=has_cwd>
                    <button class="btn ghost" title=t!("Diff.OpenHint") aria-pressed=move || if ui.diff_open.get() { "true" } else { "false" }
                        on:click=toggle_diff><Icon name="branch" sm=true/>{t!("Diff.Open")}</button>
                </Show>
                <button class="btn ghost" title=t!("Session.ForkHint") aria-label=t!("Session.Fork")
                    disabled=move || !m(|m| m.can_fork) on:click=fork><Icon name="branch" sm=true/>{t!("Session.ForkShort")}</button>
                {move || m(|m| m.status != SessionStatus::Exited).then(|| {
                    let stop = stop.clone();
                    view! { <button class="btn ghost" title=t!("Session.StopHint") on:click=stop><Icon name="x" sm=true/>{t!("Session.Stop")}</button> }
                })}
                <button class="btn ghost" title=format!("{} ({})", t!("Inspector"), i18n::kbd("Ctrl I")) aria-label=t!("Inspector")
                    aria-pressed=move || if ui.inspector_open.get() { "true" } else { "false" }
                    on:click=move |_| ui.inspector_open.update(|o| *o = !*o)><Icon name="panel" sm=true/></button>
            </span>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(pr_label("https://github.com/o/r/pull/42"), "#42");
        assert_eq!(pr_label("https://example.com/review/abc/"), "abc");
        let m = SessionMeta { branch: Some("main".into()), mode: "plan".into(), cwd: "/Users/me/src/a".into(), ..Default::default() };
        assert_eq!(facts(&m, "/Users/me", "/"), format!("main · {} · ~/src/a", i18n::name("Mode.Name.", Some("plan"))));
    }
}
