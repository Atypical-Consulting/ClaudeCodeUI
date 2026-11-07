//! Port of `Components/Shared/Markdown.razor` — spec 06 §7, 05 §5. Owner: ui-shell.
//! Rendered once per text; the `.cb` wrapping comes from `md::render`, highlighting from interop.js.

use ccui_shared::i18n;
use ccui_shared::md;
use leptos::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;

/// Rendered HTML per (UI language, text): remounting a thread (switching sessions) does not re-render its messages,
/// and the code-block labels are localized. ponytail: cleared wholesale past CAP entries (the C# cache was a weak
/// table freed with each string); an LRU if long sessions ever thrash it.
const CAP: usize = 4000;

thread_local! {
    static RENDERED: RefCell<HashMap<(&'static str, String), String>> = RefCell::new(HashMap::new());
}

/// `md::render(text)` through the cache.
pub fn rendered(text: &str) -> String {
    let key = (i18n::lang().code(), text.to_string());
    if let Some(html) = RENDERED.with(|c| c.borrow().get(&key).cloned()) {
        return html;
    }
    let html = md::render(text);
    RENDERED.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= CAP {
            c.clear();
        }
        c.insert(key, html.clone());
    });
    html
}

/// `<div class="prose md">` with `md::render(text)` set once via inner_html, cached per (language, text).
#[component]
pub fn Markdown(
    text: String,
) -> impl IntoView {
    view! { <div class="prose md" inner_html=rendered(&text)></div> }
}
