//! Port of `Components/Session/Composer.razor` — spec 07 §36. Owner: ui-session-thread-composer.

use crate::common::effort_slider::EffortSlider;
use crate::common::icon::Icon;
use crate::interop;
use crate::ipc;
use crate::session::config_menu::ConfigMenu;
use crate::session::mention_popover::MentionPopover;
use crate::session::mode_menu::ModeMenu;
use crate::session::model_menu::{current_model, models};
use crate::session::model_menu::ModelMenu;
use crate::session::slash_popover::SlashPopover;
use crate::session::PopoverCtl;
use crate::state::{use_ui, SessionHandle};
use ccui_shared::api::AnswerArgs;
use ccui_shared::model::*;
use ccui_shared::prompt_history::PromptHistory;
use ccui_shared::session::SessionMeta;
use ccui_shared::{file_index, fmt, i18n, images, invisible, json, session_rules, t};
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;
use leptos_router::NavigateOptions;
use serde_json::Value;
use std::time::Duration;

/// How long the Composer waits after the last keystroke before writing the draft to the backend.
const DRAFT_DEBOUNCE: Duration = Duration::from_millis(300);

/// `SlashQuery`: while a name is typed after "/" (no whitespace yet). A recalled "/command" (history index ≥ 0) must
/// not open the popover that would take the arrows.
pub fn slash_query(text: &str, dismissed: bool, history_index: i32) -> Option<String> {
    (!dismissed && history_index < 0 && text.starts_with('/') && !text.chars().any(char::is_whitespace)).then(|| text.to_string())
}

/// `MentionQuery`: a path typed after an "@" that starts the last word; never over an untouched recalled prompt
/// (Send trims, so "fix @src/Foo.cs " comes back as a live query and the picker would take the recall's arrows).
pub fn mention_query(text: &str, dismissed: bool, menus: bool, slash: bool, recalled: Option<&str>) -> Option<String> {
    if dismissed || menus || slash || recalled == Some(text) {
        return None;
    }
    file_index::query(text)
}

/// What a typed command does instead of being sent (they open a terminal dialog the CLI cannot show headless).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Native {
    ModelMenu,
    Config,
    /// Navigate to this base-relative path.
    Go(String),
    /// Inspector context panel (memory, extra folders).
    Context,
    Rewind,
    Diff,
}

/// `Native(t)` on the trimmed text (exact match).
pub fn native(t: &str, session_id: &str, has_models: bool, crashed: bool, has_cwd: bool) -> Option<Native> {
    let ext = |tab: &str| Some(Native::Go(format!("extensions?tab={tab}&session={session_id}")));
    match t {
        "/model" | "/output-style" if has_models => Some(Native::ModelMenu), // the model menu holds the styles
        "/config" | "/settings" if !crashed => Some(Native::Config),
        "/mcp" | "/hooks" | "/permissions" => ext(&t[1..]),
        "/allowed-tools" => ext("permissions"),
        "/plugin" | "/plugins" => ext("plugins"),
        "/memory" | "/add-dir" => Some(Native::Context), // the context panel holds both
        "/rewind" | "/checkpoint" | "/undo" => Some(Native::Rewind),
        "/diff" if has_cwd => Some(Native::Diff), // terminal-only in the CLI
        _ => None,
    }
}

/// The textarea's placeholder key (and the suggestion, ghost text, when shown).
pub fn placeholder(status: SessionStatus, ultra_turn: bool, suggestion: Option<&str>) -> String {
    match status {
        SessionStatus::Crashed => t!("Composer.PhCrashed"),
        SessionStatus::Waiting => t!("Composer.PhWaiting"),
        SessionStatus::Exited => t!("Composer.PhHistory"),
        SessionStatus::Running if ultra_turn => t!("Composer.PhWorkflow"),
        SessionStatus::Running => t!("Composer.PhRunning"),
        _ => match suggestion {
            Some(s) => t!("Composer.Suggestion", s),
            None => t!("Composer.PhIdle"),
        },
    }
}

/// `FastReason`.
pub fn fast_reason(reason: Option<&str>) -> Option<String> {
    reason.map(|r| match r {
        "sdk_opt_in_required" => t!("Composer.FastSdkOptIn"),
        "extra_usage_disabled" => t!("Composer.FastExtraUsage"),
        other => other.to_string(),
    })
}

/// Which popover the Composer shows (at most one, in this priority).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pop {
    Mode,
    Model,
    Config,
    Slash,
    Mention,
    None,
}

fn latest_rewindable(meta: &SessionMeta, items: &[&Item]) -> Option<UserItem> {
    items.iter().rev().filter_map(|i| i.as_user()).find(|u| session_rules::can_rewind(meta, items, u.uuid.as_deref())).cloned()
}

/// Prompt box: draft (`session.draft`; text → `session_set_draft_text(text, rev)` debounced, images →
/// `session_set_draft_images(images, rev)` on change; adopt a pushed draft only on a new rev), images (images::from in wasm), queued chips, menus/popovers,
/// slash commands, history recall (PromptHistory + past_prompts), Invisible strip notice, send / stop, meta row.
#[component]
pub fn Composer(session: SessionHandle) -> impl IntoView {
    let ui = use_ui();
    let navigate = use_navigate();
    let nav: StoredValue<Box<dyn Fn(&str)>, LocalStorage> =
        StoredValue::new_local(Box::new(move |p: &str| navigate(p, NavigateOptions::default())));


    // ------------------------------------------------------------------ draft (lives on the session handle)
    let text = Memo::new(move |_| session.draft.with(|d| d.text.clone()));
    let imgs = Memo::new(move |_| session.draft.with(|d| d.images.clone()));
    let pending: StoredValue<Option<TimeoutHandle>> = StoredValue::new(None);
    let cancel_pending = move || {
        if let Some(h) = pending.try_update_value(Option::take).flatten() {
            h.clear();
        }
    };
    let send_text_now = move || {
        let Some((text, rev)) = session.draft.try_with_untracked(|d| (d.text.clone(), d.rev)) else { return };
        let id = session.id();
        leptos::task::spawn_local(async move {
            if let Err(e) = ipc::session_set_draft_text(&id, &text, rev).await {
                leptos::logging::error!("[{id}] draft: {e}");
            }
        });
    };
    let set_text = move |v: String| {
        session.draft.update(|d| d.text = v);
        cancel_pending();
        let h = set_timeout_with_handle(
            move || {
                let _ = pending.try_set_value(None);
                send_text_now();
            },
            DRAFT_DEBOUNCE,
        );
        if let Ok(h) = h {
            let _ = pending.try_set_value(Some(h));
        }
    };
    let set_images = move |v: Vec<UserImage>| {
        session.draft.update(|d| d.images = v.clone());
        let rev = session.draft.with_untracked(|d| d.rev);
        let id = session.id();
        leptos::task::spawn_local(async move {
            if let Err(e) = ipc::session_set_draft_images(&id, &v, rev).await {
                leptos::logging::error!("[{id}] draft images: {e}");
            }
        });
    };
    // A pushed draft (rewind restored a prompt) is adopted by the handle on a new rev: drop the pending write, which
    // carries the old rev and would be ignored anyway.
    let rev = Memo::new(move |_| session.draft.with(|d| d.rev));
    Effect::new(move |prev: Option<u64>| {
        let r = rev.get();
        if prev.is_some_and(|p| p != r) {
            cancel_pending();
        }
        r
    });

    // ------------------------------------------------------------------ local state
    let menu_open = RwSignal::new(false);
    let mode_open = RwSignal::new(false);
    let config_open = RwSignal::new(false);
    let slash_dismissed = RwSignal::new(false);
    let mention_dismissed = RwSignal::new(false);
    let settings_asked = StoredValue::new(false);
    let reading = RwSignal::new(false);
    let recalled = RwSignal::new(None::<String>);
    let ultra_available = RwSignal::new(None::<bool>);
    let applied_effort = RwSignal::new(None::<String>);
    let effort_cap = RwSignal::new(None::<String>);
    let error = RwSignal::new(None::<String>);
    let history = RwSignal::new(PromptHistory::new());
    let past: StoredValue<Option<Vec<String>>> = StoredValue::new(None);
    let slash = PopoverCtl::new();
    let mention = PopoverCtl::new();

    let area = NodeRef::<leptos::html::Textarea>::new();
    let boxed = NodeRef::<leptos::html::Div>::new();
    let attach_btn = NodeRef::<leptos::html::Button>::new();
    let picker = NodeRef::<leptos::html::Input>::new();
    let mode_btn = NodeRef::<leptos::html::Button>::new();
    let pick_btn = NodeRef::<leptos::html::Button>::new();
    let focus_area = move || {
        request_animation_frame(move || {
            if let Some(a) = area.get_untracked() {
                interop::focus(&a);
            }
        })
    };

    // ------------------------------------------------------------------ derived
    let status = Memo::new(move |_| session.meta.with(|m| m.status));
    let crashed = Memo::new(move |_| status.get() == SessionStatus::Crashed);
    let running = Memo::new(move |_| status.get() == SessionStatus::Running);
    let ultra_turn = Memo::new(move |_| {
        running.get() && session.with_items(|it| it.iter().rev().find_map(|i| i.as_user()).is_some_and(|u| u.ultracode))
    });
    let model_list = Memo::new(move |_| session.heavy.with(|h| models(h.initialize_info.as_ref())));
    let model = Memo::new(move |_| {
        let m = session.meta.with(|m| m.model.clone());
        session.heavy.with(|h| current_model(m.as_deref(), h.initialize_info.as_ref()))
    });
    let effort = Memo::new(move |_| applied_effort.get().or_else(|| session.meta.with(|m| m.effort.clone())));
    let model_label = move || {
        let sm = session.meta.with(|m| m.model.clone());
        match model.get() {
            Some(m) => json::str(&m, "displayName").map(str::to_string).or(sm).unwrap_or_default(),
            None => sm.unwrap_or_else(|| t!("Model")),
        }
    };
    let slash_query = Memo::new(move |_| text.with(|t| slash_query(t, slash_dismissed.get(), history.with(|h| h.index()))));
    let mention_query = Memo::new(move |_| {
        let menus = menu_open.get() || mode_open.get();
        let slash_some = slash_query.with(Option::is_some);
        text.with(|t| recalled.with(|r| mention_query(t, mention_dismissed.get(), menus, slash_some, r.as_deref())))
    });
    let slash_open = Memo::new(move |_| !menu_open.get() && !mode_open.get() && slash_query.with(Option::is_some) && slash.open.get());
    let mention_open = Memo::new(move |_| mention_query.with(Option::is_some) && mention.open.get());
    let suggestion = Memo::new(move |_| session.meta.with(|m| m.suggestion.clone()));
    let suggestion_shown = Memo::new(move |_| {
        suggestion.with(Option::is_some) && status.get() == SessionStatus::Idle && text.with(String::is_empty) && imgs.with(Vec::is_empty)
    });
    let fast_on = Memo::new(move |_| session.meta.with(|m| m.fast_mode_state == "on"));
    let fast_ok = Memo::new(move |_| {
        fast_on.get() && model.with(|m| m.as_ref().is_some_and(|m| matches!(json::prop(m, "supportsFastMode"), Some(Value::Bool(true)))))
    });
    let effort_levels = Memo::new(move |_| {
        let cap = effort_cap.get();
        model.with(|m| {
            let m = m.as_ref()?;
            if !matches!(json::prop(m, "supportsEffort"), Some(Value::Bool(true))) {
                return None;
            }
            let Some(Value::Array(a)) = json::prop(m, "supportedEffortLevels") else { return None };
            let levels: Vec<&str> = a.iter().map(|x| x.as_str().unwrap_or("")).collect();
            Some(session_rules::capped(&levels, cap.as_deref()).into_iter().map(str::to_string).collect::<Vec<_>>())
        })
    });
    let has_content = move || !text.with(|t| t.trim().is_empty()) || !imgs.with(Vec::is_empty);

    // ------------------------------------------------------------------ actions
    let run = move |fut: std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>>>>| {
        error.set(None);
        leptos::task::spawn_local(async move {
            if let Err(e) = fut.await {
                let _ = error.try_set(Some(t!("Composer.ActionFailed", e)));
            }
        });
    };
    // ultracodeAvailable and the applied effort live in get_settings: read once the process answered initialize, and
    // after a model or effort change.
    let read_settings = move || {
        let id = session.id();
        leptos::task::spawn_local(async move {
            match ipc::session_request(&id, "get_settings", None).await {
                Ok(settings) => {
                    if let Some(a) = json::prop(&settings, "applied") {
                        let u = match json::prop(a, "ultracodeAvailable") {
                            Some(Value::Bool(b)) => Some(*b),
                            _ => None,
                        };
                        let _ = ultra_available.try_set(u);
                        let _ = applied_effort.try_set(json::str(a, "effort").map(str::to_string));
                        let cap = json::prop(&settings, "effective").and_then(|eff| session_rules::effort_cap(eff, json::str(a, "model")));
                        let _ = effort_cap.try_set(cap);
                    }
                }
                Err(e) => leptos::logging::error!("[{id}] get_settings : {e}"),
            }
        });
    };
    Effect::new(move |_| {
        let ready = session.meta.with(|m| m.has_process) && session.heavy.with(|h| h.initialize_info.is_some());
        if ready && !settings_asked.get_value() {
            settings_asked.set_value(true);
            read_settings();
        }
    });
    let set_effort = move |level: String| {
        error.set(None);
        let id = session.id();
        leptos::task::spawn_local(async move {
            if let Err(e) = ipc::session_set_effort(&id, &level).await {
                let _ = error.try_set(Some(t!("Composer.ActionFailed", e)));
            }
            read_settings();
        });
    };
    let close_mode_to_trigger = move || {
        mode_open.set(false);
        request_animation_frame(move || {
            if let Some(b) = mode_btn.get_untracked() {
                interop::focus(&b);
            }
        });
    };
    let close_menu_to_trigger = move || {
        menu_open.set(false);
        if session.meta.with_untracked(|m| m.has_process) {
            read_settings(); // another model: its applied effort and effort cap
        }
        request_animation_frame(move || {
            if let Some(b) = pick_btn.get_untracked() {
                interop::focus(&b);
            }
        });
    };
    let close_config = move || {
        config_open.set(false);
        focus_area();
    };

    let apply_native = move |n: Native| match n {
        Native::ModelMenu => menu_open.set(true),
        Native::Config => config_open.set(true),
        Native::Go(path) => {
            let _ = nav.try_with_value(|f| f(&format!("/{path}")));
        }
        Native::Context => {
            ui.selected_tool_id.set(None);
            ui.diff_open.set(false);
            ui.inspector_open.set(true);
        }
        // The terminal's message picker: the latest message that can go back; the Thread's own buttons reach the others.
        Native::Rewind => {
            let last = untrack(|| session.meta.with(|m| session.with_items(|it| latest_rewindable(m, it))));
            match last {
                Some(u) => ui.request_rewind(u),
                None => error.set(Some(t!("Rewind.Unavailable"))),
            }
        }
        Native::Diff => {
            ui.selected_tool_id.set(None);
            ui.diff_open.set(true);
            ui.inspector_open.set(true);
        }
    };

    // now: the CLI's "send now", the message interrupts the turn instead of waiting for it.
    let send = move |now: bool| {
        let t = text.get_untracked().trim().to_string();
        let sent_imgs = imgs.get_untracked();
        if (t.is_empty() && sent_imgs.is_empty()) || reading.get_untracked() || crashed.get_untracked() {
            return;
        }
        // Invisible characters are taken out and shown first: the next Send sends what was reviewed.
        let (clean, hidden) = invisible::strip(&t);
        if hidden > 0 {
            set_text(clean);
            error.set(Some(t!("Composer.Invisible", hidden)));
            return;
        }
        set_text(String::new());
        if !sent_imgs.is_empty() {
            set_images(Vec::new());
        }
        recalled.set(None);
        history.update(PromptHistory::reset);
        menu_open.set(false);
        mode_open.set(false);
        config_open.set(false);
        if sent_imgs.is_empty() {
            let (has_models, has_cwd) = (model_list.with_untracked(|m| !m.is_empty()), session.meta.with_untracked(|m| !m.cwd.is_empty()));
            if let Some(n) = native(&t, &session.id(), has_models, crashed.get_untracked(), has_cwd) {
                apply_native(n);
                return;
            }
        }
        error.set(None);
        let pending_ids: Vec<String> = session.meta.with_untracked(|m| m.pending.iter().map(|p| p.request_id.clone()).collect());
        let sid = session.id();
        leptos::task::spawn_local(async move {
            let res: Result<(), String> = async {
                // "Or tell Claude what you want instead…": the pending tools are refused, then the message follows.
                for request_id in pending_ids {
                    let args = AnswerArgs { request_id, decision: Decision::Deny, mode: None, message: None, input: None, destination: None };
                    ipc::session_answer(&sid, &args).await?;
                }
                let uuid = ipc::session_send(&sid, &t, &sent_imgs).await?;
                if now {
                    // The queued chip arrives with the next patch (≤ 50 ms): send it now once it is there.
                    // ponytail: bounded wait on the read model; a send_now on a message that is not queued would
                    // interrupt the turn instead, so never call it blind.
                    for _ in 0..20 {
                        let state = session.meta.try_with_untracked(|m| (m.queued.iter().any(|q| q.uuid == uuid), m.status));
                        match state {
                            Some((true, _)) => {
                                ipc::session_send_now(&sid, &uuid).await?;
                                break;
                            }
                            Some((false, SessionStatus::Running | SessionStatus::Waiting)) => {
                                gloo_timers::future::TimeoutFuture::new(25).await
                            }
                            _ => break,
                        }
                    }
                }
                Ok(())
            }
            .await;
            if let Err(e) = res {
                let _ = error.try_set(Some(t!("Composer.ActionFailed", e)));
                // Never lose the prompt to a failed send.
                if text.try_with_untracked(String::is_empty) == Some(true) {
                    set_text(t);
                }
                if imgs.try_with_untracked(Vec::is_empty) == Some(true) && !sent_imgs.is_empty() {
                    set_images(sent_imgs);
                }
            }
        });
    };

    // Picked, pasted or dropped files. The type comes from the bytes; each one is read whole, up to MAX_BYTES.
    let add_images = move |e: leptos::ev::Event| {
        let input = event_target::<web_sys::HtmlInputElement>(&e);
        let Some(list) = input.files() else { return };
        let files: Vec<web_sys::File> = (0..list.length()).filter_map(|i| list.get(i)).collect();
        input.set_value("");
        if files.is_empty() {
            return;
        }
        error.set(None);
        reading.set(true);
        leptos::task::spawn_local(async move {
            for f in files {
                let name = f.name();
                let size = f.size() as i64;
                if imgs.try_with_untracked(Vec::len).unwrap_or(0) >= images::MAX_COUNT {
                    let _ = error.try_set(Some(t!("Composer.ImageTooMany", images::MAX_COUNT)));
                    break;
                }
                if size > images::MAX_BYTES as i64 {
                    let _ = error.try_set(Some(t!("Composer.ImageTooBig", name, fmt::size(size), fmt::size(images::MAX_BYTES as i64))));
                    continue;
                }
                let bytes = match wasm_bindgen_futures::JsFuture::from(f.array_buffer()).await {
                    Ok(buf) => js_sys::Uint8Array::new(&buf).to_vec(),
                    Err(err) => {
                        let msg = err.as_string().unwrap_or_else(|| format!("{err:?}"));
                        let _ = error.try_set(Some(t!("Composer.ActionFailed", msg)));
                        continue;
                    }
                };
                match images::from(&bytes, Some(&name)) {
                    Some(img) => {
                        let Some(mut now) = imgs.try_get_untracked() else { break };
                        now.push(img);
                        set_images(now);
                    }
                    None => {
                        let _ = error.try_set(Some(t!("Composer.ImageType", name)));
                    }
                }
            }
            let _ = reading.try_set(false);
        });
    };
    let remove = move |i: usize| {
        let mut now = imgs.get_untracked();
        if i < now.len() {
            now.remove(i);
            set_images(now);
        }
        focus_area(); // the removed button took the focus with it
    };
    let pick = Callback::new(move |name: String| {
        set_text(format!("/{name} "));
        focus_area();
    });
    let pick_mention = Callback::new(move |path: String| {
        set_text(file_index::insert(&text.get_untracked(), &path));
        focus_area();
    });

    // ↑ (step 1) / ↓ (step -1) from interop.js, caret on the first / last line and no popover open.
    let recall = move |step: i32, current: String| {
        let walk = move |current: String| {
            let Some(Some(t)) = history.try_update(|h| h.r#move(step, &current)) else { return };
            recalled.set(Some(t.clone()));
            set_text(t);
        };
        if history.try_with_untracked(|h| h.index()) != Some(-1) || step <= 0 {
            walk(current);
            return;
        }
        let load = move |past: Vec<String>| {
            // image-only messages have no text to recall
            let mut entries: Vec<String> = untrack(|| {
                session.with_items(|it| it.iter().filter_map(|i| i.as_user()).map(|u| u.text.clone()).filter(|t| !t.is_empty()).collect())
            });
            entries.reverse();
            entries.extend(past);
            history.update(|h| h.load(entries));
        };
        match past.try_get_value() {
            Some(Some(p)) => {
                load(p);
                walk(current);
            }
            Some(None) => {
                // Past prompts of this folder, read once per Composer (≤ 10 transcripts).
                let (cwd, sid) = (session.meta.with_untracked(|m| m.cwd.clone()), session.id());
                leptos::task::spawn_local(async move {
                    let p = ipc::past_prompts(&cwd, Some(&sid)).await.unwrap_or_else(|e| {
                        leptos::logging::error!("[{sid}] past prompts: {e}");
                        Vec::new()
                    });
                    if past.try_set_value(Some(p.clone())).is_some() {
                        return; // gone meanwhile
                    }
                    load(p);
                    walk(current);
                });
            }
            None => {}
        }
    };

    // First render: paste / drop / paperclip, and the ↑ ↓ recall.
    Effect::new(move |_| {
        if let (Some(b), Some(i), Some(btn)) = (boxed.get(), picker.get(), attach_btn.get()) {
            interop::images(&b, &i, &btn);
        }
    });
    Effect::new(move |_| {
        if let Some(a) = area.get() {
            interop::history(&a, recall);
        }
    });
    on_cleanup(move || {
        // Leaving mid-walk: the draft holds the recalled prompt; give the typed draft back.
        if let Some(Some(d)) = history.try_with_untracked(|h| h.draft().map(str::to_string)) {
            let _ = session.draft.try_update(|x| x.text = d);
            cancel_pending();
            send_text_now();
        } else if pending.try_with_value(Option::is_some) == Some(true) {
            cancel_pending();
            send_text_now();
        }
    });

    let key = move |e: leptos::ev::KeyboardEvent| {
        let k = e.key();
        if k == "Enter" && (e.ctrl_key() || e.meta_key()) {
            e.prevent_default();
            send(e.shift_key() && running.get_untracked());
            return;
        }
        if k == "Tab" && !e.shift_key() && suggestion_shown.get_untracked() {
            e.prevent_default(); // the focus stays in the box
            if let Some(s) = suggestion.get_untracked() {
                set_text(s);
            }
            return;
        }
        if k == "Escape" {
            // the menu closes on .composer
            slash_dismissed.set(slash_query.with_untracked(Option::is_some));
            mention_dismissed.set(mention_query.with_untracked(Option::is_some));
            return;
        }
        let t = text.get_untracked();
        if !t.starts_with('/') {
            slash_dismissed.set(false);
        }
        if file_index::query(&t).is_none() {
            mention_dismissed.set(false);
        }
        if mention_open.get_untracked() {
            match k.as_str() {
                "ArrowDown" | "ArrowUp" => {
                    e.prevent_default();
                    mention.press(&k);
                }
                "Tab" if !e.shift_key() => {
                    e.prevent_default();
                    mention.press("Tab");
                }
                _ => {}
            }
            return;
        }
        if !slash_open.get_untracked() {
            return;
        }
        if matches!(k.as_str(), "ArrowDown" | "ArrowUp" | "Tab") {
            e.prevent_default();
            slash.press(&k);
        }
    };
    let section_key = move |e: leptos::ev::KeyboardEvent| {
        if e.key() != "Escape" {
            return;
        }
        if config_open.get_untracked() {
            close_config();
        }
        menu_open.set(false);
        mode_open.set(false);
    };

    // ------------------------------------------------------------------ view parts
    let pop = Memo::new(move |_| {
        if mode_open.get() {
            Pop::Mode
        } else if menu_open.get() {
            Pop::Model
        } else if config_open.get() {
            Pop::Config
        } else if slash_query.with(Option::is_some) {
            Pop::Slash
        } else if mention_query.with(Option::is_some) {
            Pop::Mention
        } else {
            Pop::None
        }
    });
    let close_mode = Callback::new(move |_: ()| close_mode_to_trigger());
    let close_menu = Callback::new(move |_: ()| close_menu_to_trigger());
    let close_cfg = Callback::new(move |_: ()| close_config());
    let popover = move || match pop.get() {
        Pop::Mode => view! {
            <div class="shield" on:click=move |_| close_mode_to_trigger()></div>
            <ModeMenu session=session on_close=close_mode on_escape=close_mode/>
        }
        .into_any(),
        Pop::Model => view! {
            <div class="shield" on:click=move |_| close_menu_to_trigger()></div>
            <ModelMenu session=session on_close=close_menu on_escape=close_menu/>
        }
        .into_any(),
        Pop::Config => view! {
            <div class="shield" on:click=move |_| close_config()></div>
            <ConfigMenu session=session on_close=close_cfg/>
        }
        .into_any(),
        Pop::Slash => view! { <SlashPopover session=session query=slash_query on_pick=pick ctl=slash/> }.into_any(),
        // The pane is keyed by session: another session gets a new Composer and a new scan.
        Pop::Mention => view! { <MentionPopover session=session query=mention_query on_pick=pick_mention ctl=mention/> }.into_any(),
        Pop::None => ().into_any(),
    };

    let queued = Memo::new(move |_| session.meta.with(|m| m.queued.clone()));
    let chips = move || {
        (!queued.with(Vec::is_empty)).then(|| {
            view! {
                <ul class="queued" aria-label=t!("Composer.QueuedList")>
                    <For each=move || queued.get() key=|q| q.uuid.clone() let:q>
                        {
                            let (u1, u2) = (q.uuid.clone(), q.uuid.clone());
                            let (title, shown) = (q.text.clone(), q.text.clone());
                            let cancel_label = t!("Composer.CancelQueued", q.text);
                            view! {
                                <li class="qchip">
                                    <span class="tag" title=t!("Composer.QueueHint")>{t!("Composer.Queued")}</span>
                                    <span class="qt" title=title>{shown}</span>
                                    <button type="button" class="btn ghost" aria-label=t!("Composer.SendNowQueued", q.text.clone())
                                        title=t!("Composer.SendNowHint")
                                        on:click=move |_| {
                                            let (sid, u) = (session.id(), u1.clone());
                                            run(Box::pin(async move { ipc::session_send_now(&sid, &u).await }));
                                        }><Icon name="right" sm=true/></button>
                                    // The chip leaves on the CLI's command_lifecycle frame; the focus goes back to the prompt.
                                    <button type="button" class="btn ghost" aria-label=cancel_label.clone() title=cancel_label
                                        on:click=move |_| {
                                            let (sid, u) = (session.id(), u2.clone());
                                            run(Box::pin(async move { ipc::session_cancel_queued(&sid, &u).await }));
                                            focus_area();
                                        }><Icon name="x" sm=true/></button>
                                </li>
                            }
                        }
                    </For>
                </ul>
            }
        })
    };
    let thumbs = move || {
        let list = imgs.get();
        (!list.is_empty()).then(|| {
            let rows = list
                .into_iter()
                .enumerate()
                .map(|(i, img)| {
                    let n = i + 1;
                    let inner = if img.document() {
                        let title = img.name.clone();
                        let name = img.name.unwrap_or_else(|| t!("Composer.Document", n));
                        view! { <span class="miss" title=title><Icon name="paperclip" sm=true/>{name}</span> }.into_any()
                    } else {
                        view! { <img src=img.url alt=t!("Composer.ImageAlt", n)/> }.into_any()
                    };
                    let label = t!("Composer.RemoveImage", n);
                    view! {
                        <li>{inner}
                            <button type="button" class="x" title=label.clone() aria-label=label on:click=move |_| remove(i)><Icon name="x" sm=true/></button>
                        </li>
                    }
                })
                .collect_view();
            view! { <ul class="thumbs" aria-label=t!("Composer.Images")>{rows}</ul> }
        })
    };
    let mode_name = move || session.meta.with(|m| i18n::name("Mode.Name.", Some(&m.mode)));
    let mode_part = move || {
        if crashed.get() {
            view! { <span class="chip" title=t!("Composer.PermissionMode")>{mode_name}</span> }.into_any()
        } else {
            view! {
                <button class=move || if session.meta.with(|m| session_rules::risky(&m.mode)) { "chip risk" } else { "chip" } node_ref=mode_btn
                    title=t!("Composer.PermissionMode") aria-label=move || format!("{}: {}", t!("Composer.PermissionMode"), mode_name())
                    aria-haspopup="listbox" aria-expanded=move || if mode_open.get() { "true" } else { "false" }
                    on:click=move |_| {
                        mode_open.update(|o| *o = !*o);
                        menu_open.set(false);
                    }>{mode_name}<Icon name="down"/></button>
            }
            .into_any()
        }
    };
    let model_part = move || {
        if !model_list.with(Vec::is_empty) && !crashed.get() {
            view! {
                <button class="pick model" node_ref=pick_btn title=t!("Model") aria-label=move || format!("{}: {}", t!("Model"), model_label())
                    aria-haspopup="listbox" aria-expanded=move || if menu_open.get() { "true" } else { "false" }
                    on:click=move |_| {
                        menu_open.update(|o| *o = !*o);
                        mode_open.set(false);
                    }>
                    <Icon name="cpu"/>{model_label}" "
                    {move || session.meta.with(|m| m.context_window).map(|w| view! { <small>{fmt::tokens(w)}</small> })}
                    <Icon name="down"/>
                </button>
            }
            .into_any()
        } else {
            view! {
                <span class="pick dis" aria-disabled="true"><Icon name="cpu"/>{move || session.meta.with(|m| m.model.clone()).unwrap_or_else(|| t!("Model"))}</span>
            }
            .into_any()
        }
    };
    // Meta-derived parts go through memos: the meta is replaced on every patch.
    let advisor = Memo::new(move |_| session.meta.with(|m| m.advisor.clone()));
    let output_style = Memo::new(move |_| session.meta.with(|m| m.output_style.clone()));
    let artifacts = Memo::new(move |_| session.meta.with(|m| m.artifacts.clone()));
    let outcome_of = Memo::new(move |_| session.meta.with(|m| (m.ultracode, m.last_result_subtype.clone())));
    let advisor_tag = move || {
        advisor.get().map(|adv| {
            let label = model_list.with(|ms| {
                ms.iter()
                    .find(|m| json::str(m, "resolvedModel") == Some(adv.as_str()) && json::str(m, "value") != Some("default"))
                    .map(|m| json::str(m, "displayName").unwrap_or(&adv).to_string())
                    .unwrap_or_else(|| adv.clone())
            });
            view! { <span class="tag" title=t!("Model.AdvisorHint")>{t!("Composer.Advisor", label)}</span> }
        })
    };
    let style_tag = move || {
        output_style.get().filter(|s| s != "default").map(|os| {
            view! { <span class="tag" title=t!("Model.StyleHint")>{t!("Composer.Style", os)}</span> }
        })
    };
    let effort_part = move || {
        if crashed.get() {
            return None;
        }
        effort_levels.get().map(|levels| {
            view! {
                <EffortSlider levels=levels value=Signal::derive(move || effort.get().unwrap_or_default())
                    on_change=Callback::new(set_effort) name=Callback::new(|l: String| i18n::name("Effort.", Some(&l)))
                    label=t!("Composer.Effort")/>
            }
        })
    };
    let fast_part = move || {
        if fast_ok.get() {
            view! {
                <button class=move || if fast_on.get() { "pick on" } else { "pick" } aria-pressed=move || if fast_on.get() { "true" } else { "false" }
                    title=t!("Composer.FastMode") on:click=move |_| {
                        let (sid, on) = (session.id(), !fast_on.get_untracked());
                        run(Box::pin(async move { ipc::session_set_fast(&sid, on).await }));
                    }><Icon name="zap"/>{t!("Composer.Fast")}</button>
            }
            .into_any()
        } else {
            let title = move || match fast_reason(session.meta.with(|m| m.fast_mode_reason.clone()).as_deref()) {
                Some(r) => t!("Composer.FastUnavailableWhy", r),
                None => t!("Composer.FastUnavailable"),
            };
            view! { <button type="button" class="pick dis" aria-disabled="true" title=title><Icon name="zap"/>{t!("Composer.Fast")}</button> }.into_any()
        }
    };
    // As the terminal's thinking toggle (set_max_thinking_tokens); mid-conversation the CLI warns it costs latency.
    let thinking_part = move || {
        if crashed.get() {
            return view! { <button type="button" class="pick dis" aria-disabled="true">{t!("Composer.Thinking")}</button> }.into_any();
        }
        let on = move || session.meta.with(|m| m.thinking);
        view! {
            <button class=move || if on() { "pick on" } else { "pick" } aria-pressed=move || if on() { "true" } else { "false" }
                title=move || t!(if session.meta.with(|m| m.context_tokens) > 0 { "Composer.ThinkingMid" } else { "Composer.ThinkingHint" })
                on:click=move |_| {
                    let (sid, v) = (session.id(), !on());
                    run(Box::pin(async move { ipc::session_set_thinking(&sid, v).await }));
                }>{t!("Composer.Thinking")}</button>
        }
        .into_any()
    };
    // Its own toggle, at any effort (claude 2.1.284 split it from the effort slider).
    let ultra_part = move || {
        if crashed.get() || ultra_available.get() == Some(false) {
            return view! {
                <button type="button" class="pick dis" aria-disabled="true" title=t!("Composer.UltraUnavailable")><Icon name="agents"/>"Ultracode"</button>
            }
            .into_any();
        }
        let on = move || session.meta.with(|m| m.ultracode);
        view! {
            <button class=move || if on() { "pick on" } else { "pick" } aria-pressed=move || if on() { "true" } else { "false" }
                title=t!("Composer.UltraHint") on:click=move |_| {
                    let (sid, v) = (session.id(), !on());
                    run(Box::pin(async move { ipc::session_set_ultracode(&sid, v).await }));
                }><Icon name="agents"/>"Ultracode"</button>
        }
        .into_any()
    };
    let pending_n = Memo::new(move |_| session.meta.with(|m| m.pending.len()));
    let blank = Memo::new(move |_| text.with(|t| t.trim().is_empty()));
    let sends = move || {
        let blank = blank.get();
        if running.get() {
            let ultra = ultra_turn.get();
            view! {
                {ultra.then(|| view! { <span class="tag acc">"ultracode"</span> })}
                {(!blank).then(|| view! {
                    <button class="btn go primary" title=t!("Composer.QueueHint") on:click=move |_| send(false)>
                        {t!("Composer.Queue")}" "<kbd>{i18n::kbd("Ctrl ⏎")}</kbd></button>
                    <button class="btn go" title=t!("Composer.SendNowHint") on:click=move |_| send(true)>
                        {t!("Composer.SendNow")}" "<kbd>{i18n::kbd("Ctrl ⇧ ⏎")}</kbd></button>
                })}
                <button class="btn go" on:click=move |_| {
                    let sid = session.id();
                    run(Box::pin(async move { ipc::session_interrupt(&sid).await }));
                }><Icon name="stop" sm=true/>{t!(if ultra { "Composer.StopWorkflow" } else { "Composer.Interrupt" })}" "<kbd>{t!("Key.Esc")}</kbd></button>
            }
            .into_any()
        } else if crashed.get() {
            view! { <button class="btn go" disabled=true>{t!("Composer.Send")}</button> }.into_any()
        } else if pending_n.get() > 0 {
            // Sending answers the pending requests with a denial first: say so on the button itself.
            let n = pending_n.get();
            view! {
                <button class="btn go danger" title=t!("Composer.DenyAndSendHint") disabled=move || !has_content() on:click=move |_| send(false)>
                    {t!("Composer.DenyAndSend", n)}" "<kbd>{i18n::kbd("Ctrl ⏎")}</kbd></button>
            }
            .into_any()
        } else {
            view! {
                <button class=move || if has_content() { "btn go primary" } else { "btn go" } disabled=move || !has_content() on:click=move |_| send(false)>
                    {t!("Composer.Send")}" "<kbd>{i18n::kbd("Ctrl ⏎")}</kbd></button>
            }
            .into_any()
        }
    };
    let turn_live = Memo::new(move |_| session.meta.with(|m| m.turn_started_at.is_some()));
    let now = super::clock(turn_live.into());
    let turn = move || {
        let (started, last) = session.meta.with(|m| (m.turn_started_at, m.last_turn_ms));
        started.map(|t| (now.get() - t).num_milliseconds()).or(last)
    };
    let arts = move || {
        let list = artifacts.get();
        (!list.is_empty()).then(|| {
            let n = list.len();
            let rows = list
                .into_iter()
                .map(|a| {
                    let (href, title) = (a.url.clone(), a.url);
                    view! { <li><a href=href target="_blank" rel="noopener noreferrer" title=title>{a.title}</a></li> }
                })
                .collect_view();
            // The CLI's /artifacts: what this session published (system/session_metadata).
            view! {
                <details class="arts"><summary>{i18n::plural(n as i64, "Artifacts.Count")}</summary>
                    <ul aria-label=t!("Artifacts.List")>{rows}</ul></details>
            }
        })
    };
    let outcome = move || {
        if let Some(e) = error.get() {
            return e.into_any();
        }
        let (ultra, subtype) = outcome_of.get();
        if ultra && !running.get() {
            return view! { <span class="tag acc">"ultracode"</span>" "{t!("Composer.UltraThisTurn")} }.into_any();
        }
        // StatusLine announces the turn outcome: shown here, not announced twice.
        let word = match subtype.as_deref() {
            _ if running.get() => String::new(),
            None | Some("success") => String::new(),
            Some(session_rules::INTERRUPTED) => t!("Composer.Interrupted"),
            Some(_) => t!("Composer.TurnError"),
        };
        view! { <span aria-hidden="true">{word}</span> }.into_any()
    };

    view! {
        <section class="composer rich" aria-label=t!("Composer.Label") on:keydown=section_key>
            {popover}
            <div aria-live="polite">{chips}</div>
            <div class="box" node_ref=boxed>
                {thumbs}
                <textarea node_ref=area prop:value=move || text.get()
                    on:input=move |e| {
                        set_text(event_target_value(&e));
                        history.update(PromptHistory::reset);
                    }
                    on:keydown=key placeholder=move || placeholder(status.get(), ultra_turn.get(), suggestion_shown.get().then(|| suggestion.get()).flatten().as_deref())
                    disabled=move || crashed.get()
                    aria-label=t!("Composer.Label")
                    aria-keyshortcuts=move || if suggestion_shown.get() { "ArrowUp ArrowDown Tab" } else { "ArrowUp ArrowDown" }
                    aria-describedby="composer-history"
                    aria-autocomplete=move || (slash_open.get() || mention_open.get()).then_some("list")
                    aria-controls=move || if slash_open.get() { Some("slash-list") } else if mention_open.get() { Some("mention-list") } else { None }
                    aria-activedescendant=move || {
                        if slash_open.get() { slash.active_id.get() } else if mention_open.get() { mention.active_id.get() } else { None }
                    }></textarea>
                <div class="tools-row">
                    <span class="tools">
                        {mode_part}
                        // Paste and drop (interop images) hand files to this input.
                        <button type="button" class="pick" node_ref=attach_btn disabled=move || crashed.get()
                            title=t!("Composer.Attach") aria-label=t!("Composer.Attach")><Icon name="paperclip"/></button>
                        <input type="file" node_ref=picker on:change=add_images multiple=true
                            accept="image/png,image/jpeg,image/gif,image/webp,application/pdf,text/plain,text/markdown,.md,.txt,.csv,.json,.log"
                            hidden=true tabindex="-1"/>
                        {model_part}
                        {advisor_tag}
                        {style_tag}
                        {effort_part}
                        {fast_part}
                        {thinking_part}
                        {ultra_part}
                    </span>
                    <span class="sends">{sends}</span>
                </div>
            </div>
            <div class="meta">
                {move || turn().map(|d| view! { <span>{t!("Composer.Turn")}" "<b>{fmt::dur(d)}</b></span> })}
                <span>{t!("Composer.Session")}" "<b>{move || session.meta.with(|m| fmt::cost(m.cost_usd, 4))}</b></span>
                <span>{t!("Kv.Tools")}" "<b>{move || session.meta.with(|m| m.tool_count)}</b></span>
                {arts}
                <span id="composer-history" role="status">
                    {move || history.with(|h| if h.index() >= 0 { t!("Composer.History", h.index() + 1, h.count()) } else { String::new() })}
                </span>
                {move || {
                    let (ctx, w) = session.meta.with(|m| (m.context_tokens, m.context_window));
                    (ctx > 0).then(|| view! {
                        <span>{t!("Composer.Context")}" "<b>{fmt::tokens(ctx)}</b>{w.map(|w| format!(" / {}", fmt::tokens(w)))}</span>
                    })
                }}
                // One live region, so an error is announced.
                <span class=move || if error.with(Option::is_some) { "r c-err-fg" } else { "r" } role="status">{outcome}</span>
            </div>
        </section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries() {
        assert_eq!(slash_query("/co", false, -1).as_deref(), Some("/co"));
        assert_eq!(slash_query("/co ", false, -1), None);
        assert_eq!(slash_query("/co", false, 0), None, "a recalled command does not open the popover");
        assert_eq!(slash_query("/co", true, -1), None);
        assert_eq!(mention_query("look @src/Fo", false, false, false, None).as_deref(), Some("src/Fo"));
        assert_eq!(mention_query("fix @src/Foo.cs", false, false, false, Some("fix @src/Foo.cs")), None);
        assert_eq!(mention_query("fix @src", false, true, false, None), None);
    }

    #[test]
    fn natives() {
        assert_eq!(native("/model", "s", true, false, true), Some(Native::ModelMenu));
        assert_eq!(native("/model", "s", false, false, true), None);
        assert_eq!(native("/config", "s", true, true, true), None);
        assert_eq!(native("/hooks", "s", true, false, true), Some(Native::Go("extensions?tab=hooks&session=s".into())));
        assert_eq!(native("/allowed-tools", "s", true, false, true), Some(Native::Go("extensions?tab=permissions&session=s".into())));
        assert_eq!(native("/plugins", "s", true, false, true), Some(Native::Go("extensions?tab=plugins&session=s".into())));
        assert_eq!(native("/undo", "s", true, false, true), Some(Native::Rewind));
        assert_eq!(native("/diff", "s", true, false, false), None);
        assert_eq!(native("/diff now", "s", true, false, true), None);
    }

    #[test]
    fn placeholders_and_reasons() {
        assert_eq!(placeholder(SessionStatus::Running, true, None), t!("Composer.PhWorkflow"));
        assert_eq!(placeholder(SessionStatus::Idle, false, Some("go on")), t!("Composer.Suggestion", "go on"));
        assert_eq!(placeholder(SessionStatus::Idle, false, None), t!("Composer.PhIdle"));
        assert_eq!(fast_reason(Some("sdk_opt_in_required")), Some(t!("Composer.FastSdkOptIn")));
        assert_eq!(fast_reason(Some("other")).as_deref(), Some("other"));
        assert_eq!(fast_reason(None), None);
    }
}
