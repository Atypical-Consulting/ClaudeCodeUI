//! Port of `wwwroot/app.js` (`window.claudeUi` + the global listeners) — spec 06 §9, spec 07 §0.5. Owner: ui-shell.
//! Port each to Rust/web-sys here, or to `ui/assets/interop.js` bound with `#[wasm_bindgen(module = ...)]` where the
//! DOM code is simpler in JS (implementer's call; highlight.js stays a JS asset loaded by index.html).
//!
//! RULE (from app.js): never replace a node Leptos rendered. JS-side code may only mutate `code.lc` / `pre>code`
//! contents (hljs), classes/tabindex/aria it owns, and copy-button label text nodes (temporarily). Elements whose
//! inner HTML is mutated here must be rendered static (non-reactive) by their component.

use web_sys::{Element, HtmlDialogElement, HtmlElement, HtmlInputElement, HtmlTextAreaElement};

/// Install every global behaviour once at boot: copy buttons (`.cb .copy`, `[data-copy]`, label from
/// `body[data-copied]`), syntax highlighting (MutationObserver + IntersectionObserver → hljs), roving tabs/radios,
/// small key rules, table keys (`.wtt`, `.bgt`, `.mcp`), Extensions focus return, Overview list keys, external links
/// (`a[target=_blank]` → `ipc::open_url`, internal hrefs through the router).
pub fn install_globals() {
    todo!("port: app.js top-level listeners")
}

/// Global shortcuts → MainLayout (`Ctrl+K`, `Ctrl+I`, `Ctrl+N`/`Alt+N`, `Ctrl+Shift+O`, `Ctrl+Shift+A`, Escape, and
/// the relayed Enter / Shift+Enter / Delete on an armed `[data-permission]` card). metaKey counts as Ctrl.
pub fn register_shortcuts(on_key: impl Fn(String) + 'static) {
    todo!("port: claudeUi.registerShortcuts + global keydown")
}

/// Splitters (`.split[data-split=rail|insp]`): drag/keys resize `--rail-w` / `--insp-w`, sync aria values; on
/// release `on_resize(which, Some("Npx") | None for reset)`.
pub fn register_splitters(on_resize: impl Fn(String, Option<String>) + 'static) {
    todo!("port: app.js splitters (§9.9)")
}

/// After every navigation: focus the page's `h1` (tabIndex -1 if missing), preventScroll (FocusOnNavigate).
pub fn focus_h1() {
    todo!("port: FocusOnNavigate Selector=h1")
}

/// `<html data-theme>` (validated, default graphite).
pub fn set_theme(theme: &str) {
    todo!("port: claudeUi.setTheme")
}

pub fn get_theme() -> String {
    todo!("port: claudeUi.getTheme")
}

/// `--code-size` on `<html>`.
pub fn set_code_size(size: &str) {
    todo!("port: claudeUi.setCodeSize")
}

pub fn get_code_size() -> String {
    todo!("port: claudeUi.getCodeSize")
}

/// Clipboard write (no clipboard → false; never claim "Copied").
pub async fn copy(text: &str) -> bool {
    todo!("port: claudeUi.copy")
}

pub fn focus(el: &HtmlElement) {
    todo!("port: claudeUi.focus")
}

/// `container.querySelector('#'+id).scrollIntoView({block})`; with focus: tabIndex -1 if none, focus preventScroll.
pub fn reveal(container: &Element, id: &str, block: &str, focus: bool) {
    todo!("port: claudeUi.reveal")
}

/// Focus `el` if nothing / body / an h1 is focused.
pub fn focus_if_idle(el: &HtmlElement) {
    todo!("port: claudeUi.focusIfIdle")
}

/// Focus the first `a, button, [tabindex]` inside.
pub fn focus_first(el: &Element) {
    todo!("port: claudeUi.focusFirst")
}

/// ResizeObserver toggling `.clip` on `.ex.said` children that overflow (Overview cards).
pub fn clip_tops(el: &Element) {
    todo!("port: claudeUi.clipTops")
}

/// Palette modal: remember the opener, `.app.inert`, Tab trap, refocus input; on close give focus back.
pub fn modal(open: bool, input: Option<&HtmlInputElement>) {
    todo!("port: claudeUi.modal")
}

/// `scrollLeft = scrollWidth` (a long path shows its distinguishing end).
pub fn scroll_to_end(el: &HtmlInputElement) {
    todo!("port: claudeUi.scrollToEnd")
}

/// Thread pinning: keep `.thread` at the bottom unless the user scrolled UP; `.dock .jump` shown when new output
/// arrives while free; `force` re-pins.
pub fn scroll_end(el: &Element, force: bool) {
    todo!("port: claudeUi.scrollEnd")
}

/// Native modal `<dialog>`: showModal / close then focus `then`.
pub fn dialog(el: &HtmlDialogElement, open: bool, then: Option<&str>) {
    todo!("port: claudeUi.dialog")
}

/// Keys inert 300 ms on a new permission card; focus it unless the user is typing / in a dialog.
pub fn arm_permission(el: &HtmlElement) {
    todo!("port: claudeUi.armPermission")
}

/// Composer attachments: paperclip → file input; paste / drop files into the input (`box.dataset.drop` while
/// dragging Files).
pub fn images(r#box: &HtmlElement, input: &HtmlInputElement, button: &HtmlElement) {
    todo!("port: claudeUi.images")
}

/// Composer ↑/↓ recall on the first/last line → `recall(+1 | -1, value)` after the caret did not move.
pub fn history(textarea: &HtmlTextAreaElement, recall: impl Fn(i32, String) + 'static) {
    todo!("port: claudeUi.history")
}
