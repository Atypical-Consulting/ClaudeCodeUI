//! Port of `Core/Md.cs` (Markdown → safe HTML) — spec 05 §5. Owner: shared-pure-b.
//! pulldown-cmark (GFM: tables, task lists, strikethrough, footnotes, alerts), raw HTML escaped, images turned into
//! links, only http/https/mailto links kept (http(s) get target=_blank rel="noopener noreferrer"), other links/
//! autolinks reduced to their label text. Then the exact app.css wrapping:
//! `<div class="cb"><div class="h"><span>{label}</span><button class="copy" type="button" tabindex="0"><svg class="i sm"><use href="#i-copy"/></svg><span>{Md.Copy}</span></button></div><pre tabindex="0"><code class="language-x">…</code></pre></div>`
//! and alert titles `<div class="markdown-alert markdown-alert-x"><p class="markdown-alert-title"><svg…octicon/>{Md.Alert.X}</p>`.
//! Labels are localized: callers cache per (language, text).

/// Render model markdown to the final HTML string (set once with `inner_html` inside `<div class="prose md">`).
pub fn render(text: &str) -> String {
    todo!("port: Md.Render")
}

/// Code-block label: `Md.PlainText` for none, friendly name (csharp/cs → C#, bash → Bash, sh/shell → Shell,
/// powershell/ps1 → PowerShell, json → JSON, diff → Diff, javascript/js → JavaScript, typescript/ts → TypeScript,
/// xml → XML, html → HTML, css → CSS, sql → SQL, yaml → YAML, python → Python), else the HTML-encoded language.
pub fn label(lang: Option<&str>) -> String {
    todo!("port: Md.Label")
}

/// http/https (target blank) or mailto; trimmed absolute URI only.
pub fn external(url: &str) -> bool {
    todo!("port: Md.External")
}
