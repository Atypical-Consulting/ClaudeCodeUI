//! Port of `wwwroot/app.js` (`window.claudeUi` + the global listeners) — spec 06 §9, spec 07 §0.5. Owner: ui-shell.
//! The small helpers are Rust/web-sys here; the DOM-level listeners (highlighting, copy buttons, table / list keys,
//! splitters, shortcut capture, palette modal, thread pinning, composer images / history) stay JS in
//! `ui/assets/interop.js`, bound below — they keep per-element state as expando properties that the other
//! listeners read (`_armedAt`, `_free`, `_ri`), which is simplest kept on one side. highlight.js stays a JS asset.
//!
//! RULE (from app.js): never replace a node Leptos rendered. JS-side code may only mutate `code.lc` / `pre>code`
//! contents (hljs), classes/tabindex/aria it owns, and copy-button label text nodes (temporarily). Elements whose
//! inner HTML is mutated here must be rendered static (non-reactive) by their component.

use ccui_shared::api::THEMES;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlDialogElement, HtmlElement, HtmlInputElement, HtmlTextAreaElement};

const THEME_KEY: &str = "claude-ui.theme";
const SIZE_KEY: &str = "claude-ui.code-size";

#[wasm_bindgen(module = "/assets/interop.js")]
extern "C" {
    #[wasm_bindgen(js_name = installGlobals)]
    fn js_install_globals();
    #[wasm_bindgen(js_name = registerShortcuts)]
    fn js_register_shortcuts(cb: &js_sys::Function);
    #[wasm_bindgen(js_name = registerSplitters)]
    fn js_register_splitters(cb: &js_sys::Function);
    #[wasm_bindgen(js_name = images)]
    fn js_images(r#box: &HtmlElement, input: &HtmlInputElement, button: &HtmlElement);
    #[wasm_bindgen(js_name = history)]
    fn js_history(el: &HtmlTextAreaElement, recall: &js_sys::Function);
    #[wasm_bindgen(js_name = armPermission)]
    fn js_arm_permission(el: &HtmlElement);
    #[wasm_bindgen(js_name = clipTops)]
    fn js_clip_tops(el: &Element);
    #[wasm_bindgen(js_name = modal)]
    fn js_modal(open: bool, el: Option<HtmlInputElement>);
    #[wasm_bindgen(js_name = scrollEnd)]
    fn js_scroll_end(el: &Element, force: bool);
}

fn document() -> Option<web_sys::Document> {
    web_sys::window().and_then(|w| w.document())
}

fn html() -> Option<HtmlElement> {
    document().and_then(|d| d.document_element()).and_then(|e| e.dyn_into().ok())
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

fn focus_quiet(el: &HtmlElement) {
    let opts = web_sys::FocusOptions::new();
    opts.set_prevent_scroll(true);
    let _ = el.focus_with_options(&opts);
}

/// A plain JS function from a Rust closure that lives as long as the page (registered once).
fn forever<F: ?Sized + wasm_bindgen::closure::WasmClosure>(cb: Closure<F>) -> js_sys::Function {
    let f = cb.as_ref().unchecked_ref::<js_sys::Function>().clone();
    cb.forget();
    f
}

/// Install every global behaviour once at boot: copy buttons (`.cb .copy`, `[data-copy]`, label from
/// `body[data-copied]`), syntax highlighting (MutationObserver + IntersectionObserver → hljs), roving tabs/radios,
/// small key rules, table keys (`.wtt`, `.bgt`, `.mcp`), Extensions focus return, Overview list keys, external links
/// (`a[target=_blank]` → `ipc::open_url`, internal hrefs through the router).
pub fn install_globals() {
    js_install_globals();
    // External links never open a webview window: the OS browser gets them. Internal hrefs are the router's.
    let Some(doc) = document() else { return };
    let cb = Closure::<dyn FnMut(web_sys::MouseEvent)>::new(|e: web_sys::MouseEvent| {
        let Some(a) = e
            .target()
            .and_then(|t| t.dyn_into::<Element>().ok())
            .and_then(|t| t.closest("a[target=_blank]").ok().flatten())
            .and_then(|a| a.dyn_into::<web_sys::HtmlAnchorElement>().ok())
        else {
            return;
        };
        e.prevent_default();
        let url = a.href();
        if url.is_empty() {
            return;
        }
        leptos::task::spawn_local(async move {
            if let Err(err) = crate::ipc::open_url(&url).await {
                leptos::logging::error!("open_url: {err}");
            }
        });
    });
    let _ = doc.add_event_listener_with_callback("click", &forever(cb));
}

/// Global shortcuts → MainLayout (`Ctrl+K`, `Ctrl+I`, `Ctrl+N`/`Alt+N`, `Ctrl+Shift+O`, `Ctrl+Shift+A`, Escape, and
/// the relayed Enter / Shift+Enter / Delete on an armed `[data-permission]` card). metaKey counts as Ctrl.
pub fn register_shortcuts(on_key: impl Fn(String) + 'static) {
    js_register_shortcuts(&forever(Closure::<dyn Fn(String)>::new(on_key)));
}

/// Splitters (`.split[data-split=rail|insp]`): drag/keys resize `--rail-w` / `--insp-w`, sync aria values; on
/// release `on_resize(which, Some("Npx") | None for reset)`.
pub fn register_splitters(on_resize: impl Fn(String, Option<String>) + 'static) {
    js_register_splitters(&forever(Closure::<dyn Fn(String, Option<String>)>::new(on_resize)));
}

/// After every navigation: focus the page's `h1` (tabIndex -1 if missing), preventScroll (FocusOnNavigate).
pub fn focus_h1() {
    let Some(h1) = document()
        .and_then(|d| d.query_selector("h1").ok().flatten())
        .and_then(|e| e.dyn_into::<HtmlElement>().ok())
    else {
        return;
    };
    if !h1.has_attribute("tabindex") {
        h1.set_tab_index(-1);
    }
    focus_quiet(&h1);
}

/// The theme `<html data-theme>` takes: one of `THEMES`, else graphite.
pub fn valid_theme(theme: &str) -> &str {
    if THEMES.contains(&theme) {
        theme
    } else {
        "graphite"
    }
}

/// `<html data-theme>` (validated, default graphite).
pub fn set_theme(theme: &str) {
    let t = valid_theme(theme);
    if let Some(h) = html() {
        let _ = h.set_attribute("data-theme", t);
    }
    if let Some(s) = storage() {
        let _ = s.set_item(THEME_KEY, t);
    }
}

pub fn get_theme() -> String {
    html()
        .and_then(|h| h.get_attribute("data-theme"))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "graphite".to_string())
}

/// `--code-size` on `<html>`.
pub fn set_code_size(size: &str) {
    if let Some(h) = html() {
        let _ = h.style().set_property("--code-size", size);
    }
    if let Some(s) = storage() {
        let _ = s.set_item(SIZE_KEY, size);
    }
}

pub fn get_code_size() -> String {
    storage()
        .and_then(|s| s.get_item(SIZE_KEY).ok().flatten())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "13px".to_string())
}

/// `--rail-w` / `--insp-w` on `<html>` (`which` = rail | insp) from a pref written elsewhere; None or a value that
/// is not `Npx` goes back to the CSS default.
pub fn set_pane_width(which: &str, width: Option<&str>) {
    let Some(h) = html() else { return };
    let var = format!("--{which}-w");
    match width.filter(|w| is_px(w)) {
        Some(w) => {
            let _ = h.style().set_property(&var, w);
        }
        None => {
            let _ = h.style().remove_property(&var);
        }
    }
}

/// `/^\d+px$/`.
pub fn is_px(v: &str) -> bool {
    v.strip_suffix("px").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Clipboard write (no clipboard → false; never claim "Copied").
pub async fn copy(text: &str) -> bool {
    let Some(nav) = web_sys::window().map(|w| w.navigator()) else { return false };
    let has = js_sys::Reflect::get(&nav, &JsValue::from_str("clipboard")).map(|c| !c.is_undefined() && !c.is_null());
    if !has.unwrap_or(false) {
        return false;
    }
    wasm_bindgen_futures::JsFuture::from(nav.clipboard().write_text(text)).await.is_ok()
}

pub fn focus(el: &HtmlElement) {
    let _ = el.focus();
}

/// `container.querySelector('#'+id).scrollIntoView({block})`; with focus: tabIndex -1 if none, focus preventScroll.
pub fn reveal(container: &Element, id: &str, block: &str, focus: bool) {
    let Some(r) = container.query_selector(&format!("#{id}")).ok().flatten() else { return };
    let opts = web_sys::ScrollIntoViewOptions::new();
    opts.set_block(match block {
        "start" => web_sys::ScrollLogicalPosition::Start,
        "center" => web_sys::ScrollLogicalPosition::Center,
        "end" => web_sys::ScrollLogicalPosition::End,
        _ => web_sys::ScrollLogicalPosition::Nearest,
    });
    r.scroll_into_view_with_scroll_into_view_options(&opts);
    if focus {
        if let Ok(r) = r.dyn_into::<HtmlElement>() {
            if !r.has_attribute("tabindex") {
                r.set_tab_index(-1); // a rendered tabindex is the component's: keep it
            }
            focus_quiet(&r);
        }
    }
}

/// Focus `el` if nothing / body / an h1 is focused.
pub fn focus_if_idle(el: &HtmlElement) {
    let Some(doc) = document() else { return };
    let idle = match doc.active_element() {
        None => true,
        Some(a) => doc.body().is_some_and(|b| *b == a) || a.matches("h1").unwrap_or(false),
    };
    if idle {
        let _ = el.focus();
    }
}

/// Focus the first `a, button, [tabindex]` inside.
pub fn focus_first(el: &Element) {
    if let Some(f) = el.query_selector("a,button,[tabindex]").ok().flatten().and_then(|f| f.dyn_into::<HtmlElement>().ok()) {
        let _ = f.focus();
    }
}

/// ResizeObserver toggling `.clip` on `.ex.said` children that overflow (Overview cards).
pub fn clip_tops(el: &Element) {
    js_clip_tops(el);
}

/// Palette modal: remember the opener, `.app.inert`, Tab trap, refocus input; on close give focus back.
pub fn modal(open: bool, input: Option<&HtmlInputElement>) {
    js_modal(open, input.cloned());
}

/// `scrollLeft = scrollWidth` (a long path shows its distinguishing end).
pub fn scroll_to_end(el: &HtmlInputElement) {
    el.set_scroll_left(el.scroll_width());
}

/// Thread pinning: keep `.thread` at the bottom unless the user scrolled UP; `.dock .jump` shown when new output
/// arrives while free; `force` re-pins.
pub fn scroll_end(el: &Element, force: bool) {
    js_scroll_end(el, force);
}

/// Native modal `<dialog>`: showModal / close then focus `then`.
pub fn dialog(el: &HtmlDialogElement, open: bool, then: Option<&str>) {
    if open {
        if !el.open() {
            let _ = el.show_modal();
        }
        return;
    }
    if el.open() {
        el.close();
    }
    if let Some(sel) = then {
        if let Some(t) = document()
            .and_then(|d| d.query_selector(sel).ok().flatten())
            .and_then(|t| t.dyn_into::<HtmlElement>().ok())
        {
            let _ = t.focus();
        }
    }
}

/// Keys inert 300 ms on a new permission card; focus it unless the user is typing / in a dialog.
pub fn arm_permission(el: &HtmlElement) {
    js_arm_permission(el);
}

/// Composer attachments: paperclip → file input; paste / drop files into the input (`box.dataset.drop` while
/// dragging Files).
pub fn images(r#box: &HtmlElement, input: &HtmlInputElement, button: &HtmlElement) {
    js_images(r#box, input, button);
}

/// Composer ↑/↓ recall on the first/last line → `recall(+1 | -1, value)` after the caret did not move.
pub fn history(textarea: &HtmlTextAreaElement, recall: impl Fn(i32, String) + 'static) {
    if js_sys::Reflect::get(textarea, &JsValue::from_str("_history")).is_ok_and(|v| v.is_truthy()) {
        return; // already wired: no second closure
    }
    // ponytail: the closure outlives its textarea (one small leak per Composer mount); drop it on unmount if it adds up.
    js_history(textarea, &forever(Closure::<dyn Fn(i32, String)>::new(recall)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_validation() {
        assert_eq!(valid_theme("encre"), "encre");
        assert_eq!(valid_theme("contraste"), "contraste");
        assert_eq!(valid_theme("neon"), "graphite");
        assert_eq!(valid_theme(""), "graphite");
    }

    #[test]
    fn px_values() {
        assert!(is_px("240px"));
        assert!(!is_px("px"));
        assert!(!is_px("24.5px"));
        assert!(!is_px("240"));
        assert!(!is_px("-2px"));
    }
}
