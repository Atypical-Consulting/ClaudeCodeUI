//! Port of `Components/Shared/Icon.razor` — spec 06 §4. Owner: ui-shell. IMPLEMENTED.
//! `<svg class="i [sm] [class]" aria-hidden="true"><use href="#i-{name}"/></svg>` (symbols from IconSprite).

use leptos::prelude::*;

#[component]
pub fn Icon(
    #[prop(into)] name: String,
    #[prop(optional)] sm: bool,
    #[prop(optional, into)] class: Option<String>,
) -> impl IntoView {
    let mut cls = String::from("i");
    if sm {
        cls.push_str(" sm");
    }
    if let Some(c) = class.filter(|c| !c.is_empty()) {
        cls.push(' ');
        cls.push_str(&c);
    }
    view! { <svg class=cls aria-hidden="true"><use href=format!("#i-{name}")></use></svg> }
}
