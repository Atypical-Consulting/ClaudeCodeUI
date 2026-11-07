//! Port of `Components/Session/QuestionForm.razor` — spec 07 §27, 02 §8. Owner: ui-session-inspector.
//! AskUserQuestion, in place of the Allow/Deny body of the PermissionCard: one fieldset per question (radios, or
//! checkboxes when multiSelect) plus a free-text "Other". Answer allows the tool with `ask_user::with_answers`; Skip
//! denies it. Keys: ↑↓ move between options (native in a radio group, list_nav for checkboxes), Space ticks, ⏎
//! answers, Del skips; ⏎ outside any field (relayed by the card) focuses the first option. An option's preview
//! (markdown: a mockup, a snippet) shows under the question while that option has focus, else the picked one's.

use super::permission_card::on_relayed_key;
use crate::common::markdown::Markdown;
use crate::ipc;
use crate::state::SessionHandle;
use ccui_shared::api::AnswerArgs;
use ccui_shared::ask_user::{self, Question};
use ccui_shared::model::*;
use ccui_shared::{list_nav, t};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::BTreeSet;
use wasm_bindgen::JsCast;

/// One question's state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Picks {
    pub picked: BTreeSet<usize>,
    pub other_on: bool,
    pub other: String,
}

/// `Pick(q, o, on)`: `o == options.len()` is "Other". A radio only reports being picked; a checkbox its new state.
pub fn pick(q: &Question, p: &mut Picks, o: usize, on: bool) {
    if !q.multi {
        p.picked.clear();
        p.other_on = false;
    }
    if o == q.options.len() {
        p.other_on = on;
    } else if on {
        p.picked.insert(o);
    } else {
        p.picked.remove(&o);
    }
}

/// The answer text of one question ("" = unanswered).
pub fn value(q: &Question, p: &Picks) -> String {
    let picked: Vec<usize> = p.picked.iter().copied().collect();
    ask_user::answer(q, &picked, p.other_on.then_some(p.other.as_str()))
}

/// The preview shown: the focused option's, else the first picked one's.
pub fn preview(q: &Question, focused: Option<usize>, p: &Picks) -> Option<String> {
    let o = focused.filter(|f| *f < q.options.len()).or_else(|| p.picked.iter().next().copied())?;
    q.options.get(o)?.preview.clone()
}

fn focus_id(id: &str) {
    if let Some(el) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(id))
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.focus();
    }
}

/// AskUserQuestion form (options + Other, multi), answers via session_answer with `ask_user::with_answers`.
#[component]
pub fn QuestionForm(
    session: SessionHandle,
    permission: PendingPermission,
) -> impl IntoView {
    let questions = StoredValue::new(ask_user::parse(&permission.input));
    let n = questions.with_value(Vec::len);
    let picks = RwSignal::new(vec![Picks::default(); n]);
    // Per question, the option that last had focus.
    let focused = RwSignal::new(vec![None::<usize>; n]);
    let answered = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let rid = StoredValue::new(permission.request_id.clone());
    let input = StoredValue::new(permission.input.clone());
    let id_of = move |q: usize, o: Option<usize>| {
        let base = format!("ask-{}-{q}", rid.get_value());
        match o {
            Some(o) => format!("{base}-{o}"),
            None => base,
        }
    };
    let complete = move || {
        n > 0 && questions.with_value(|qs| picks.with(|ps| qs.iter().zip(ps).all(|(q, p)| !value(q, p).is_empty())))
    };

    let send = move |args: AnswerArgs| {
        let id = session.id();
        spawn_local(async move {
            if let Err(e) = ipc::session_answer(&id, &args).await {
                error.set(Some(t!("Perm.AnswerFailed", e)));
                answered.set(false);
            }
        });
    };
    let submit = move || {
        if answered.get_untracked() || !complete() {
            return;
        }
        answered.set(true);
        let answers: Vec<(String, String)> =
            questions.with_value(|qs| picks.with_untracked(|ps| qs.iter().zip(ps).map(|(q, p)| (q.text.clone(), value(q, p))).collect()));
        send(AnswerArgs {
            request_id: rid.get_value(),
            decision: Decision::Allow,
            mode: None,
            message: None,
            input: Some(input.with_value(|i| ask_user::with_answers(i, &answers))),
            destination: None,
        });
    };
    let skip = move || {
        if answered.get_untracked() {
            return;
        }
        answered.set(true);
        send(AnswerArgs {
            request_id: rid.get_value(),
            decision: Decision::Deny,
            mode: None,
            message: None,
            input: None,
            destination: None,
        });
    };
    // Not on render: a question arriving mid-draft must not steal the composer's focus. ⏎ outside a field lands here.
    on_relayed_key(move |key| match key {
        "Enter" if n > 0 => focus_id(&id_of(0, Some(0))),
        "Delete" => skip(),
        _ => {}
    });

    let fieldsets = (0..n)
        .map(|qi| {
            let q = questions.with_value(|qs| qs[qi].clone());
            let count = q.options.len();
            let multi = q.multi;
            let legend = view! {
                <legend>
                    {(!q.header.is_empty()).then(|| view! { <span class="cap">{q.header.clone()}</span> })}
                    <span class="qt">{q.text.clone()}</span>
                </legend>
            };
            let opts = (0..=count)
                .map(|oj| {
                    let other = oj == count;
                    let on = move || picks.with(|ps| if other { ps[qi].other_on } else { ps[qi].picked.contains(&oj) });
                    let class = move || match (other, on()) {
                        (true, true) => "opt other on",
                        (true, false) => "opt other",
                        (false, true) => "opt on",
                        (false, false) => "opt",
                    };
                    let do_pick = move |o: usize, v: bool| {
                        questions.with_value(|qs| picks.update(|ps| pick(&qs[qi], &mut ps[qi], o, v)));
                    };
                    let id = id_of(qi, Some(oj));
                    let field = if other {
                        let q_text = q.text.clone();
                        view! {
                            <input class="otxt" type="text" placeholder=t!("Ask.OtherPlaceholder") aria-label=t!("Ask.OtherAnswer", q_text)
                                prop:value=move || picks.with(|ps| ps[qi].other.clone())
                                on:input=move |e| {
                                    let v = event_target_value(&e);
                                    picks.update(|ps| ps[qi].other = v);
                                    // Typing an "Other" answer picks it (and, single-select, unpicks the options).
                                    let (blank, on) = picks.with_untracked(|ps| (ps[qi].other.trim().is_empty(), ps[qi].other_on));
                                    if !blank && !on {
                                        do_pick(count, true);
                                    }
                                } />
                        }
                        .into_any()
                    } else {
                        let o = &q.options[oj];
                        let desc = (!o.description.is_empty()).then(|| view! { <span>{o.description.clone()}</span> });
                        view! { <label for=id.clone()><b>{o.label.clone()}</b>{desc}</label> }.into_any()
                    };
                    view! {
                        <div class=class>
                            <input id=id type=if multi { "checkbox" } else { "radio" } name=id_of(qi, None)
                                prop:checked=on aria-label=other.then(|| t!("Ask.Other"))
                                on:change=move |e| do_pick(oj, !multi || event_target_checked(&e))
                                on:keydown=move |e: web_sys::KeyboardEvent| match e.key().as_str() {
                                    "Enter" => {
                                        e.prevent_default();
                                        submit();
                                    }
                                    "Delete" => skip(),
                                    // radios move natively
                                    k @ ("ArrowDown" | "ArrowUp") if multi => {
                                        e.prevent_default();
                                        let next = list_nav::next(k, oj as i32, count as i32 + 1);
                                        if next >= 0 {
                                            focus_id(&id_of(qi, Some(next as usize)));
                                        }
                                    }
                                    _ => {}
                                }
                                on:focus=move |_| focused.update(|f| f[qi] = Some(oj)) />
                            {field}
                        </div>
                    }
                })
                .collect_view();
            let pv = move || {
                let p = questions.with_value(|qs| picks.with(|ps| preview(&qs[qi], focused.with(|f| f[qi]), &ps[qi])));
                p.map(|p| view! { <div class="ask-preview"><Markdown text=p /></div> })
            };
            view! { <fieldset disabled=answered>{legend}{opts}{pv}</fieldset> }
        })
        .collect_view();

    view! {
        <form class="ask" on:submit=move |e| {
            e.prevent_default();
            submit();
        }>
            {fieldsets}
            <div class="acts">
                <button type="submit" class="btn primary" disabled=move || answered.get() || !complete()>{t!("Ask.Submit")}" "<kbd>"⏎"</kbd></button>
                <button type="button" class="btn danger" disabled=answered on:click=move |_| skip()>{t!("Ask.Skip")}</button>
            </div>
            {move || error.get().map(|e| view! { <div class="errbox" role="alert">{e}</div> })}
            <p class="keys"><kbd>"↑↓"</kbd>" "{t!("Ask.Hint.Move")}" · "<kbd>{t!("Key.Space")}</kbd>" "{t!("Ask.Hint.Pick")}" · "<kbd>"⏎"</kbd>" "{t!("Ask.Hint.Submit")}" · "<kbd>{t!("Key.Del")}</kbd>" "{t!("Ask.Hint.Skip")}<span class="perm-off">" · "{t!("Perm.HintFocus")}</span></p>
        </form>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ccui_shared::ask_user::AskOption;

    fn q(multi: bool) -> Question {
        let opt = |l: &str, pv: Option<&str>| AskOption { label: l.into(), description: String::new(), preview: pv.map(str::to_string) };
        Question { text: "Which?".into(), header: String::new(), options: vec![opt("A", Some("pa")), opt("B", None)], multi }
    }

    #[test]
    fn picking() {
        let single = q(false);
        let mut p = Picks::default();
        pick(&single, &mut p, 1, true);
        pick(&single, &mut p, 0, true);
        assert_eq!(value(&single, &p), "A");
        pick(&single, &mut p, 2, true); // Other
        p.other = " mine ".into();
        assert_eq!(value(&single, &p), "mine");
        assert!(p.picked.is_empty());

        let multi = q(true);
        let mut m = Picks::default();
        pick(&multi, &mut m, 1, true);
        pick(&multi, &mut m, 0, true);
        assert_eq!(value(&multi, &m), "A, B");
        pick(&multi, &mut m, 0, false);
        assert_eq!(value(&multi, &m), "B");
        assert_eq!(value(&multi, &Picks::default()), "");
    }

    #[test]
    fn previews() {
        let single = q(false);
        let mut p = Picks::default();
        assert_eq!(preview(&single, None, &p), None);
        assert_eq!(preview(&single, Some(0), &p).as_deref(), Some("pa"));
        p.picked.insert(0);
        assert_eq!(preview(&single, Some(1), &p), None);
        assert_eq!(preview(&single, Some(2), &p).as_deref(), Some("pa")); // Other focused: the picked one's
    }
}
