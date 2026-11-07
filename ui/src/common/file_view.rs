//! Port of `Components/Shared/FileView.razor` — spec 06 §7. Owner: ui-shell.
//! A Read tool result ("    12→text" lines) with its line numbers. Callers key it by tool id: interop.js rewrites
//! code.lc, so the rows are rendered static, once.

use ccui_shared::edit_diff;
use leptos::prelude::*;

/// `(number, text)` of every `^\s*(\d+)[→\t](.*?)\r?$` line (multiline), parsed by hand (no regex in the UI crate).
pub fn numbered(text: &str) -> Vec<(String, String)> {
    text.split('\n')
        .filter_map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line).trim_start();
            let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            if digits == 0 {
                return None;
            }
            let (n, rest) = line.split_at(digits);
            let body = rest.strip_prefix('→').or_else(|| rest.strip_prefix('\t'))?;
            Some((n.to_string(), body.to_string()))
        })
        .collect()
}

/// Read tool output as numbered lines (`^\\s*(\\d+)[→\\t](.*?)\\r?$`) else `<pre class="cmdlog">`.
/// Rendered static once (app.js rewrites code.lc); keyed by tool id by callers.
#[component]
pub fn FileView(
    path: String,
    text: String,
) -> impl IntoView {
    let lines = numbered(&text);
    if lines.is_empty() {
        return view! { <pre class="cmdlog">{text}</pre> }.into_any();
    }
    let rows = lines
        .into_iter()
        .map(|(n, t)| view! { <div><span class="n">{n}</span><code class="lc">{t}</code></div> })
        .collect_view();
    view! { <div class="filev" data-hl=edit_diff::lang(&path)>{rows}</div> }.into_any()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_lines() {
        let got = numbered("     1→fn main() {\r\n     2\t    x();\n   10→}");
        assert_eq!(
            got,
            [("1".into(), "fn main() {".into()), ("2".into(), "    x();".into()), ("10".into(), "}".into())]
                as [(String, String); 3]
        );
        assert!(numbered("plain output").is_empty());
    }
}
