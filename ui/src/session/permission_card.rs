//! Port of `Components/Session/PermissionCard.razor` — spec 07 §35. Owner: ui-session-inspector.
//! can_use_tool for this session. ⏎ / Shift ⏎ / Del arrive through UiState.key (interop.js forwards them while focus is
//! inside [data-permission]). AskUserQuestion gets the QuestionForm instead of Allow/Deny: allowing it bare would tell
//! the model "the user did not answer". QuestionForm and ElicitForm take their own keys (focus_first / skip, submit /
//! decline) from UiState.key: they own their answered state and their role=alert error.

use super::elicit_form::ElicitForm;
use super::question_form::QuestionForm;
use super::tool_detail::pretty;
use crate::common::diff_view::DiffView;
use crate::common::markdown::Markdown;
use crate::interop;
use crate::ipc;
use crate::state::{use_app, use_ui, SessionHandle};
use ccui_shared::api::AnswerArgs;
use ccui_shared::edit_diff::DiffLine;
use ccui_shared::model::*;
use ccui_shared::{ask_user, elicit, i18n, json, permission_rules, session_rules, t, tool_kinds};
use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;

pub const DESTINATIONS: [&str; 4] = ["session", "localSettings", "projectSettings", "userSettings"];

/// A relayed key not seen yet by this listener (`UiState.key` carries a sequence number; the value present at mount
/// is not a new key). Shared by the forms that take keys inside the card.
pub fn on_relayed_key(mut handle: impl FnMut(&str) + 'static) {
    let ui = use_ui();
    let seen = StoredValue::new(ui.key.with_untracked(|k| k.as_ref().map(|k| k.0)));
    Effect::new(move |_| {
        let Some((n, key)) = ui.key.get() else { return };
        if seen.get_value() == Some(n) {
            return;
        }
        seen.set_value(Some(n));
        handle(&key);
    });
}

fn suggestions(p: &PendingPermission) -> &[Value] {
    p.suggestions.as_ref().and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// "Always allow": a non-empty suggestions array, unless the CLI itself offers no "always" for this call.
pub fn can_allow_session(p: &PendingPermission) -> bool {
    !suggestions(p).is_empty() && !p.info.as_ref().is_some_and(|i| i.no_always_allow)
}

/// A setMode suggestion goes through set_permission_mode (no destination to pick).
pub fn sets_mode(p: &PendingPermission) -> bool {
    suggestions(p).iter().any(|x| json::str(x, "type") == Some("setMode"))
}

/// What "Always allow" grants, as the backend sends it: a setMode wins, else the rules and directories.
pub fn session_grant(p: &PendingPermission) -> String {
    let sg = suggestions(p);
    if let Some(mode) = sg.iter().find(|x| json::str(x, "type") == Some("setMode")).and_then(|x| json::str(x, "mode")) {
        return t!("Perm.GrantMode", i18n::name("Mode.Name.", Some(mode)));
    }
    let parts: Vec<String> = sg
        .iter()
        .flat_map(|x| {
            let key = if json::str(x, "type") == Some("addDirectories") { "directories" } else { "rules" };
            match json::prop(x, key) {
                Some(Value::Array(a)) => a
                    .iter()
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        _ => {
                            let tool = json::string(v, "toolName");
                            match json::str(v, "ruleContent").filter(|c| !c.is_empty()) {
                                Some(c) => format!("{tool}({c})"),
                                None => tool,
                            }
                        }
                    })
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            }
        })
        .collect();
    t!("Perm.GrantRules", parts.join(", "))
}

/// Why the CLI asks: its own reason, else the ask rule that matched, else what the reason type says.
pub fn why(p: &PendingPermission) -> Option<String> {
    let i = p.info.as_ref()?;
    if let Some(r) = i.reason.as_deref().filter(|r| !r.is_empty()) {
        return Some(permission_rules::plain(r));
    }
    if let Some(rule) = i.ask_rule.as_deref() {
        return Some(t!("Perm.WhyAskRule", permission_rules::plain(rule), i18n::name("Perm.Dest.", i.ask_rule_source.as_deref())));
    }
    match i.reason_type.as_deref() {
        Some("rule") => Some(t!("Perm.WhyRule")),
        Some("workingDir") => Some(t!("Perm.WhyWorkingDir")),
        _ => None,
    }
}

/// `Path.GetFileName` of input.file_path.
fn file_name(p: &PendingPermission) -> String {
    let f = json::str(&p.input, "file_path").unwrap_or("");
    f.rsplit(['/', '\\']).next().unwrap_or("").to_string()
}

/// The generic card's title (`target_exists` was checked once, when the request arrived).
pub fn title(p: &PendingPermission) -> String {
    if let Some(t) = p.info.as_ref().and_then(|i| i.title.clone()).filter(|t| !t.is_empty()) {
        return t;
    }
    match p.tool.as_str() {
        "Edit" | "MultiEdit" => t!("Perm.Edit", file_name(p)),
        "Write" if p.target_exists == Some(true) => t!("Perm.Overwrite", file_name(p)),
        "Write" => t!("Perm.Create", file_name(p)),
        t if tool_kinds::is_shell(t) => t!("Perm.Run"),
        t => t!("Perm.AllowTool", t),
    }
}

/// What a relayed key does on a plan / generic card (question and elicit forms take their own keys).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyAction {
    Answer(Decision, Option<&'static str>),
    FocusFeedback,
    None,
}

pub fn key_action(key: &str, plan: bool, delegated: bool) -> KeyAction {
    if delegated {
        return KeyAction::None;
    }
    match (key, plan) {
        ("Enter", true) => KeyAction::Answer(Decision::Allow, Some("acceptEdits")),
        ("Shift+Enter", true) => KeyAction::Answer(Decision::Allow, Some("default")),
        // The feedback is typed, then sent with Ctrl ⏎.
        ("Delete", true) => KeyAction::FocusFeedback,
        ("Enter", false) => KeyAction::Answer(Decision::Allow, None),
        ("Shift+Enter", false) => KeyAction::Answer(Decision::AllowSession, None),
        ("Delete", false) => KeyAction::Answer(Decision::Deny, None),
        _ => KeyAction::None,
    }
}

/// First pending request: QuestionForm | ElicitForm | plan | generic (allow / allow session + destination / deny
/// with message); armed keys (Enter / Shift+Enter / Delete).
#[component]
pub fn PermissionCard(
    session: SessionHandle,
    permission: PendingPermission,
) -> impl IntoView {
    let app = use_app();
    let rid = permission.request_id.clone();
    let plan = permission.tool == "ExitPlanMode";
    let question = permission.tool == ask_user::TOOL && !ask_user::parse(&permission.input).is_empty();
    let elicit = permission.tool == elicit::TOOL;
    let can_session = can_allow_session(&permission);
    let answered = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let feedback = RwSignal::new(String::new());
    let destination = RwSignal::new("session".to_string());
    let diff = RwSignal::new(None::<Vec<DiffLine>>);
    let card_section = NodeRef::<html::Section>::new();
    let card_div = NodeRef::<html::Div>::new();
    let feedback_box = NodeRef::<html::Textarea>::new();

    // Keys inert 300 ms on a new card, then focused unless the user is typing (once per permission: the card is keyed).
    let armed = StoredValue::new(false);
    Effect::new(move |_| {
        let el: Option<web_sys::HtmlElement> = card_section.get().map(Into::into).or_else(|| card_div.get().map(Into::into));
        if let Some(el) = el {
            if !armed.get_value() {
                armed.set_value(true);
                interop::arm_permission(&el);
            }
        }
    });
    if tool_kinds::is_edit(&permission.tool) && !question && !elicit && !plan {
        let (tool, input) = (permission.tool.clone(), permission.input.clone());
        spawn_local(async move {
            match ipc::edit_diff_from_input(&tool, &input).await {
                Ok(d) => diff.set(Some(d)),
                Err(e) => leptos::logging::error!("edit_diff_from_input: {e}"),
            }
        });
    }

    // mode: set before allowing (ExitPlanMode approvals); message: a deny's text for the model ("keep planning").
    let answer = {
        let rid = rid.clone();
        move |d: Decision, mode: Option<&'static str>, message: Option<String>| {
            if answered.get_untracked() || (d == Decision::AllowSession && !can_session) {
                return;
            }
            answered.set(true);
            let args = AnswerArgs {
                request_id: rid.clone(),
                decision: d,
                mode: mode.map(str::to_string),
                message,
                input: None,
                destination: Some(destination.get_untracked()),
            };
            let id = session.id();
            spawn_local(async move {
                if let Err(e) = ipc::session_answer(&id, &args).await {
                    error.set(Some(t!("Perm.AnswerFailed", e)));
                    answered.set(false);
                }
            });
        }
    };
    let answer = StoredValue::new(answer);
    let ans = move |d: Decision, mode: Option<&'static str>, message: Option<String>| answer.with_value(|f| f(d, mode, message));
    let keep_planning = move || ans(Decision::Deny, None, Some(feedback.get_untracked()));

    on_relayed_key(move |key| match key_action(key, plan, question || elicit) {
        KeyAction::Answer(d, mode) => ans(d, mode, None),
        KeyAction::FocusFeedback => {
            if let Some(el) = feedback_box.get_untracked() {
                let _ = el.focus();
            }
        }
        KeyAction::None => {}
    });

    // The decision queue across sessions: this card's place, and the others.
    let queue = Memo::new(move |_| app.summary.with(|s| s.decision_queue.clone()));
    let sid = session.id();
    let pos = {
        let (rid, sid) = (rid.clone(), sid.clone());
        move || {
            queue.with(|q| {
                let i = q.iter().position(|e| e.session_id == sid && e.permission.request_id == rid).map_or(0, |i| i + 1);
                t!("OfN", i, q.len())
            })
        }
    };
    let others = {
        let (rid, sid) = (rid.clone(), sid.clone());
        move || {
            let rows: Vec<_> = queue.with(|q| {
                q.iter().filter(|e| !(e.session_id == sid && e.permission.request_id == rid)).cloned().collect()
            });
            (!rows.is_empty()).then(|| {
                let rows = rows
                    .into_iter()
                    .map(|e| {
                        let p = &e.permission;
                        let who = match &e.agent_label {
                            Some(a) => format!("{} · {a} · ", e.session_name),
                            None => format!("{} · ", e.session_name),
                        };
                        view! {
                            <a class="q" href=format!("session/{}", e.session_id) style="grid-template-columns:auto 84px minmax(0,1fr);padding:0;border:0">
                                <span class="dot wait"></span>
                                <span class="k" style=format!("color:{}", tool_kinds::color(&p.tool)) title=p.tool.clone()>{tool_kinds::label(&p.tool)}</span>
                                <span class="p" style="color:var(--fg-2)">{who}<span style="color:var(--fg)">{tool_kinds::target(&p.tool, &p.input, &e.session_cwd)}</span></span>
                            </a>
                        }
                    })
                    .collect_view();
                view! { <div class="section last"><span class="cap">{t!("Perm.AlsoWaiting")}</span>{rows}</div> }
            })
        }
    };

    let heading = if question {
        t!("Ask.Title")
    } else if elicit {
        t!("Elicit.Title")
    } else {
        t!("Perm.Title")
    };
    let sub = if elicit { json::string(&permission.input, "mcp_server_name") } else { permission.tool.clone() };
    let err_box = move || error.get().map(|e| view! { <div class="errbox" role="alert">{e}</div> });
    let shift_enter = t!("Key.ShiftEnter");

    let body = if plan {
        let fb_id = format!("plan-fb-{rid}");
        let title_id = format!("plan-title-{rid}");
        let (title_ref, fb_ref) = (title_id.clone(), fb_id.clone());
        view! {
            // ExitPlanMode: approving sets the mode the user picks (set_permission_mode, then allow); "keep planning"
            // denies with the feedback, which the model receives verbatim.
            <section class="section" data-permission="" tabindex="-1" role="group" aria-labelledby=title_ref node_ref=card_section>
                <h3 id=title_id>{t!("Plan.Title")}</h3>
                <Markdown text=json::string(&permission.input, "plan") />
                <div class="acts wrap">
                    <button class="btn primary" disabled=answered on:click=move |_| ans(Decision::Allow, Some("acceptEdits"), None)>{t!("Plan.ApproveAuto")}" "<kbd>"⏎"</kbd></button>
                    <button class="btn" disabled=answered on:click=move |_| ans(Decision::Allow, Some("default"), None)>{t!("Plan.ApproveReview")}" "<kbd>{shift_enter.clone()}</kbd></button>
                </div>
                <div class="field">
                    <label for=fb_ref>{t!("Plan.Feedback")}</label>
                    <textarea id=fb_id class="plan-fb" node_ref=feedback_box prop:value=feedback placeholder=t!("Plan.FeedbackHint") disabled=answered
                        on:input=move |e| feedback.set(event_target_value(&e))
                        on:keydown=move |e: web_sys::KeyboardEvent| {
                            if e.key() == "Enter" && (e.ctrl_key() || e.meta_key()) {
                                keep_planning();
                            }
                        }></textarea>
                </div>
                <div class="acts">
                    <button class="btn" disabled=answered on:click=move |_| keep_planning()>{t!("Plan.Keep")}" "<kbd>{i18n::kbd("Ctrl ⏎")}</kbd></button>
                </div>
                {err_box}
                <p class="keys"><kbd>"⏎"</kbd>" "{t!("Plan.HintAuto")}" · "<kbd>{shift_enter.clone()}</kbd>" "{t!("Plan.HintReview")}" · "<kbd>{t!("Key.Del")}</kbd>" "{t!("Plan.HintFeedback")}<span class="perm-off">" · "{t!("Perm.HintFocus")}</span></p>
            </section>
        }
        .into_any()
    } else {
        let title = title(&permission);
        let label = if question {
            t!("Ask.Title")
        } else if elicit {
            t!("Elicit.Title")
        } else {
            title.clone()
        };
        let inner = if question {
            view! { <QuestionForm session=session permission=permission.clone() /> }.into_any()
        } else if elicit {
            view! { <ElicitForm session=session permission=permission.clone() /> }.into_any()
        } else {
            let p = permission.clone();
            let asked_by = {
                let p = p.clone();
                move || {
                    session.with_items(|items| session_rules::asking_agent(items, &p).map(session_rules::agent_label))
                        .map(|a| view! { <p>{t!("Perm.AskedBy", a)}</p> })
                }
            };
            let desc = json::str(&p.input, "description").filter(|d| !d.is_empty()).map(|d| view! { <p>{d.to_string()}</p> });
            let why = why(&p).map(|w| view! { <p class="why">{w}</p> });
            let blocked = p.info.as_ref().and_then(|i| i.blocked_path.clone()).map(|b| {
                view! { <p class="why">{t!("Perm.BlockedPath")}" "<code>{b}</code></p> }
            });
            let shell = tool_kinds::is_shell(&p.tool);
            let edit = tool_kinds::is_edit(&p.tool);
            let content = {
                let p = p.clone();
                move || {
                    if let Some(lines) = diff.get() {
                        let path = session.meta.with_untracked(|m| tool_kinds::target(&p.tool, &p.input, &m.cwd));
                        view! { <DiffView path=path lines=lines /> }.into_any()
                    } else if edit {
                        ().into_any() // the diff is being read
                    } else if shell {
                        view! { <pre class="cmd">{json::string(&p.input, "command")}</pre> }.into_any()
                    } else {
                        view! { <Markdown text=format!("```json\n{}\n```", pretty(&p.input)) /> }.into_any()
                    }
                }
            };
            let picks = (can_session && !sets_mode(&p)).then(|| {
                let dest_id = format!("perm-dest-{rid}");
                let dest_ref = dest_id.clone();
                let opts = DESTINATIONS
                    .iter()
                    .map(|d| view! { <option value=*d selected=move || destination.get() == *d>{t!(&format!("Perm.Dest.{d}"))}</option> })
                    .collect_view();
                // Where an "always" rule is kept: the session by default, a settings file if the user picks one.
                view! {
                    <div class="picks">
                        <label for=dest_ref style="font-size:12px">{t!("Perm.SaveIn")}</label>
                        <select id=dest_id prop:value=destination disabled=answered on:change=move |e| destination.set(event_target_value(&e))>{opts}</select>
                    </div>
                }
            });
            let grant = can_session.then(|| view! { <p style="font-size:12px">{session_grant(&p)}</p> });
            view! {
                <h3>{title}</h3>
                {asked_by}
                {desc}
                {why}
                {blocked}
                {content}
                <div class="acts">
                    <button class="btn primary" disabled=answered on:click=move |_| ans(Decision::Allow, None, None)>{t!("Perm.Allow")}" "<kbd>"⏎"</kbd></button>
                    {can_session.then(|| view! {
                        <button class="btn" disabled=answered on:click=move |_| ans(Decision::AllowSession, None, None)>{t!("Perm.WholeSession")}</button>
                    })}
                    <button class="btn danger" disabled=answered on:click=move |_| ans(Decision::Deny, None, None)>{t!("Perm.Deny")}</button>
                </div>
                {picks}
                {grant}
                {err_box}
                <p class="keys">
                    <kbd>"⏎"</kbd>" "{t!("Perm.HintAllow")}" · "
                    {can_session.then(|| view! { <kbd>{shift_enter.clone()}</kbd>{format!(" {} · ", t!("Perm.HintSession"))}})}
                    <kbd>{t!("Key.Del")}</kbd>" "{t!("Perm.HintDeny")}<span class="perm-off">" · "{t!("Perm.HintFocus")}</span>
                </p>
            }
            .into_any()
        };
        view! { <div class="section" data-permission="" tabindex="-1" role="group" aria-label=label node_ref=card_div>{inner}</div> }.into_any()
    };

    view! {
        <div class="ph"><h2 class="ttl">{heading}</h2><span class="sub">{sub}</span>
            <span class="end"><span class="pill wait">{pos}</span></span>
        </div>
        <div class="scroll">
            {body}
            {others}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn perm(tool: &str, input: Value, suggestions: Option<Value>, info: Option<PermissionInfo>) -> PendingPermission {
        PendingPermission {
            request_id: "r".into(),
            tool: tool.into(),
            input,
            description: None,
            tool_use_id: None,
            suggestions,
            at: Time::default(),
            info,
            target_exists: None,
        }
    }

    #[test]
    fn always_allow() {
        i18n::set_thread_lang(Some(i18n::Lang::En));
        let rules = json!([{"type":"addRules","rules":[{"toolName":"Bash","ruleContent":"git push:*"},{"toolName":"Read"}]},
                           {"type":"addDirectories","directories":["/w/x"]}]);
        let p = perm("Bash", json!({"command":"git push"}), Some(rules), None);
        assert!(can_allow_session(&p) && !sets_mode(&p));
        assert_eq!(session_grant(&p), "Allows Bash(git push:*), Read, /w/x");
        let m = perm("Bash", json!({}), Some(json!([{"type":"setMode","mode":"acceptEdits"}])), None);
        assert!(sets_mode(&m));
        assert_eq!(session_grant(&m), "Switches the session to Accept edits mode");
        let no = perm("Bash", json!({}), Some(json!([{"type":"setMode"}])), Some(PermissionInfo { no_always_allow: true, ..Default::default() }));
        assert!(!can_allow_session(&no));
        assert!(!can_allow_session(&perm("Bash", json!({}), Some(json!([])), None)));
        i18n::set_thread_lang(None);
    }

    #[test]
    fn titles_and_reasons() {
        i18n::set_thread_lang(Some(i18n::Lang::En));
        let mut w = perm("Write", json!({"file_path":"/w/a/notes.txt"}), None, None);
        assert_eq!(title(&w), "Create notes.txt?");
        w.target_exists = Some(true);
        assert_eq!(title(&w), "Overwrite notes.txt?");
        assert_eq!(title(&perm("Bash", json!({}), None, None)), "Run this command?");
        assert_eq!(title(&perm("WebFetch", json!({}), None, None)), "Allow WebFetch?");
        let info = PermissionInfo { title: Some("Custom".into()), reason_type: Some("workingDir".into()), ..Default::default() };
        let p = perm("Edit", json!({}), None, Some(info));
        assert_eq!(title(&p), "Custom");
        assert_eq!(why(&p).as_deref(), Some("The path is outside the session's folders."));
        let r = PermissionInfo { reason: Some("\u{1b}[1mbecause\u{1b}[0m".into()), ask_rule: Some("x".into()), ..Default::default() };
        assert_eq!(why(&perm("Bash", json!({}), None, Some(r))).as_deref(), Some("because"));
        assert_eq!(why(&perm("Bash", json!({}), None, None)), None);
        i18n::set_thread_lang(None);
    }

    #[test]
    fn keys() {
        assert_eq!(key_action("Enter", true, false), KeyAction::Answer(Decision::Allow, Some("acceptEdits")));
        assert_eq!(key_action("Delete", true, false), KeyAction::FocusFeedback);
        assert_eq!(key_action("Shift+Enter", false, false), KeyAction::Answer(Decision::AllowSession, None));
        assert_eq!(key_action("Delete", false, false), KeyAction::Answer(Decision::Deny, None));
        assert_eq!(key_action("Enter", false, true), KeyAction::None);
        assert_eq!(key_action("Escape", false, false), KeyAction::None);
    }
}
