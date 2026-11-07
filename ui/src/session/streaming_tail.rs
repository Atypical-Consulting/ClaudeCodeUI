//! Port of `Components/Session/StreamingTail.razor` — spec 07 §11. Owner: ui-session-thread-composer.

use crate::interop;
use crate::state::SessionHandle;
use leptos::prelude::*;

/// Streaming text split on blank lines (finished paragraphs untouched) + caret; scroll_end after each render.
/// aria-hidden: the token stream is hidden from AT until it lands as a TextItem (the role=log announces that).
#[component]
pub fn StreamingTail(session: SessionHandle) -> impl IntoView {
    let el = NodeRef::<leptos::html::Div>::new();
    let parts = Memo::new(move |_| session.stream.with(|t| parts(t).into_iter().map(str::to_string).collect::<Vec<_>>()));
    let count = Memo::new(move |_| parts.with(Vec::len));
    let shown = Memo::new(move |_| session.stream.with(|t| !t.is_empty()));
    Effect::new(move |_| {
        session.stream.track();
        if let Some(div) = el.get() {
            // After the text node updated: pin the thread unless the user scrolled up.
            request_animation_frame(move || interop::scroll_end(&div, false));
        }
    });
    view! {
        <Show when=move || shown.get()>
            <div class="prose md" aria-hidden="true" node_ref=el>
                <p class="stream">
                    // One text node per paragraph: a finished paragraph keeps its exact text, only the last one grows.
                    <For each=move || 0..count.get() key=|i| *i let:i>
                        {move || parts.with(|p| p.get(i).cloned().unwrap_or_default())}
                    </For>
                    <span class="caret"></span>
                </p>
            </div>
        </Show>
    }
}

/// Split on `"\n\n"`, each separator kept with the paragraph before it; the last part is the remainder.
pub fn parts(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    while let Some(i) = text[start..].find("\n\n") {
        out.push(&text[start..start + i + 2]);
        start += i + 2;
    }
    out.push(&text[start..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraphs() {
        assert_eq!(parts("a\n\nb\n\nc"), ["a\n\n", "b\n\n", "c"]);
        assert_eq!(parts("a\n\n"), ["a\n\n", ""]);
        assert_eq!(parts("abc"), ["abc"]);
    }
}
