//! Port of `Core/Md.cs` (Markdown → safe HTML) — spec 05 §5. Owner: shared-pure-b.
//! pulldown-cmark (GFM: tables, task lists, strikethrough, footnotes, alerts), raw HTML escaped, images turned into
//! links, only http/https/mailto links kept (http(s) get target=_blank rel="noopener noreferrer"), other links/
//! autolinks reduced to their label text. Then the exact app.css wrapping:
//! `<div class="cb"><div class="h"><span>{label}</span><button class="copy" type="button" tabindex="0"><svg class="i sm"><use href="#i-copy"/></svg><span>{Md.Copy}</span></button></div><pre tabindex="0"><code class="language-x">…</code></pre></div>`
//! and alert titles `<div class="markdown-alert markdown-alert-x"><p class="markdown-alert-title"><svg…octicon/>{Md.Alert.X}</p>`.
//! Labels are localized: callers cache per (language, text).
//!
//! Markdig parity notes: raw HTML blocks are re-parsed as a paragraph (Markdig with DisableHtml never sees an HTML
//! block, so `<div>*x*</div>` → `<p>&lt;div&gt;<em>x</em>&lt;/div&gt;</p>`); bare `https://…` / `www.…` URLs are
//! linked like Markdig's AutoLinks extension. Not ported (rare in model output): heading auto-ids, math, abbreviations,
//! figures, custom containers; footnote definitions render where defined (pulldown) instead of in a closing section.

use crate::i18n::t;
use pulldown_cmark::{BlockQuoteKind, CowStr, Event, LinkType, Options, Parser, Tag, TagEnd};
use regex::{Captures, Regex};
use std::collections::HashSet;
use std::sync::OnceLock;

/// Render model markdown to the final HTML string (set once with `inner_html` inside `<div class="prose md">`).
pub fn render(text: &str) -> String {
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, transform(text).into_iter());
    wrap(&html)
}

/// Code-block label: `Md.PlainText` for none, friendly name (csharp/cs → C#, bash → Bash, sh/shell → Shell,
/// powershell/ps1 → PowerShell, json → JSON, diff → Diff, javascript/js → JavaScript, typescript/ts → TypeScript,
/// xml → XML, html → HTML, css → CSS, sql → SQL, yaml → YAML, python → Python), else the HTML-encoded language.
pub fn label(lang: Option<&str>) -> String {
    label_encoded(lang.map(|l| html_escape::encode_double_quoted_attribute(l).into_owned()).as_deref())
}

/// `label` for a language already HTML-encoded (as captured from the rendered `<code class="language-…">`).
fn label_encoded(lang: Option<&str>) -> String {
    let Some(lang) = lang else { return t("Md.PlainText") };
    let friendly = match lang.to_lowercase().as_str() {
        "csharp" | "cs" => "C#",
        "bash" => "Bash",
        "sh" | "shell" => "Shell",
        "powershell" | "ps1" => "PowerShell",
        "json" => "JSON",
        "diff" => "Diff",
        "javascript" | "js" => "JavaScript",
        "typescript" | "ts" => "TypeScript",
        "xml" => "XML",
        "html" => "HTML",
        "css" => "CSS",
        "sql" => "SQL",
        "yaml" => "YAML",
        "python" => "Python",
        _ => return lang.to_string(),
    };
    friendly.to_string()
}

/// http/https (target blank) or mailto; trimmed absolute URI only.
pub fn external(url: &str) -> bool {
    scheme(url).is_some()
}

/// `Some(web)` when `url` is an absolute http/https (web = true) or mailto URI, like `Uri.TryCreate(Absolute)`.
fn scheme(url: &str) -> Option<bool> {
    let u = url.trim();
    let (s, rest) = u.split_once(':')?;
    match s.to_ascii_lowercase().as_str() {
        "http" | "https" => {
            let host = rest.strip_prefix("//")?;
            let host = host.split(['/', '?', '#']).next().unwrap_or("");
            (!host.is_empty() && !host.contains(char::is_whitespace)).then_some(true)
        }
        "mailto" => (!rest.is_empty()).then_some(false),
        _ => None,
    }
}

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_DEFINITION_LIST
        | Options::ENABLE_SUPERSCRIPT
        | Options::ENABLE_SUBSCRIPT
}

/// The link/image/HTML policy as an event rewrite (Markdig did it on the AST).
fn transform(text: &str) -> Vec<Event<'_>> {
    let events = merge_text(Parser::new_ext(text, options()).collect());

    // Lists holding a task item get `contains-task-list`, task items `task-list-item` (Markdig markup, app.css hooks).
    let is_task = |i: usize| match (events.get(i + 1), events.get(i + 2)) {
        (Some(Event::TaskListMarker(_)), _) => true,
        (Some(Event::Start(Tag::Paragraph)), Some(Event::TaskListMarker(_))) => true,
        _ => false,
    };
    let mut task_lists = HashSet::new();
    let mut stack = Vec::new();
    for (i, e) in events.iter().enumerate() {
        match e {
            Event::Start(Tag::List(_)) => stack.push(i),
            Event::End(TagEnd::List(_)) => {
                stack.pop();
            }
            Event::Start(Tag::Item) if is_task(i) => {
                if let Some(&l) = stack.last() {
                    task_lists.insert(l);
                }
            }
            _ => {}
        }
    }

    let mut out = Vec::with_capacity(events.len());
    let mut links: Vec<bool> = Vec::new(); // per open link/image: did we emit an <a>?
    let mut lists: Vec<bool> = Vec::new(); // per open list: custom tag emitted?
    let mut items: Vec<bool> = Vec::new();
    let mut in_code = 0usize;
    let mut i = 0;
    while i < events.len() {
        let e = events[i].clone();
        match e {
            Event::Start(Tag::HtmlBlock) => {
                // Gather the raw block, escape its leading `<` so it is no longer an HTML block, render it as markdown.
                let mut raw = String::new();
                i += 1;
                while i < events.len() && !matches!(events[i], Event::End(TagEnd::HtmlBlock)) {
                    if let Event::Html(s) | Event::Text(s) = &events[i] {
                        raw.push_str(s);
                    }
                    i += 1;
                }
                if let Some(p) = raw.find('<') {
                    raw.insert(p, '\\');
                }
                let mut html = String::new();
                pulldown_cmark::html::push_html(&mut html, transform(&raw).into_iter());
                out.push(Event::Html(html.into()));
            }
            Event::Html(s) | Event::InlineHtml(s) => out.push(Event::Text(s)),
            Event::Start(Tag::Link { link_type, dest_url, title, .. })
            | Event::Start(Tag::Image { link_type, dest_url, title, .. }) => {
                let url = if link_type == LinkType::Email { format!("mailto:{dest_url}") } else { dest_url.to_string() };
                match scheme(&url) {
                    Some(web) => {
                        let mut a = format!("<a href=\"{}\"", escape_href(&url));
                        if web {
                            a.push_str(" target=\"_blank\" rel=\"noopener noreferrer\"");
                        }
                        if !title.is_empty() {
                            a.push_str(&format!(" title=\"{}\"", html_escape::encode_double_quoted_attribute(&title)));
                        }
                        a.push('>');
                        out.push(Event::InlineHtml(a.into()));
                        links.push(true);
                    }
                    None => links.push(false), // tagless: the label children render as text
                }
            }
            Event::End(TagEnd::Link) | Event::End(TagEnd::Image) => {
                if links.pop() == Some(true) {
                    out.push(Event::InlineHtml("</a>".into()));
                }
            }
            Event::Start(Tag::BlockQuote(Some(kind))) => {
                let (class, word, path) = alert(kind);
                out.push(Event::Html(
                    format!(
                        "<div class=\"markdown-alert markdown-alert-{class}\">\n<p class=\"markdown-alert-title\"><svg viewBox=\"0 0 16 16\" version=\"1.1\" width=\"16\" height=\"16\" aria-hidden=\"true\"><path d=\"{path}\"></path></svg>{}</p>\n",
                        t(&format!("Md.Alert.{word}"))
                    )
                    .into(),
                ));
            }
            Event::End(TagEnd::BlockQuote(Some(_))) => out.push(Event::Html("</div>\n".into())),
            Event::Start(Tag::List(start)) => {
                let task = task_lists.contains(&i);
                lists.push(task);
                if task {
                    let tag = match start {
                        None => "<ul class=\"contains-task-list\">\n".to_string(),
                        Some(1) => "<ol class=\"contains-task-list\">\n".to_string(),
                        Some(n) => format!("<ol class=\"contains-task-list\" start=\"{n}\">\n"),
                    };
                    out.push(Event::Html(tag.into()));
                } else {
                    out.push(Event::Start(Tag::List(start)));
                }
            }
            Event::End(TagEnd::List(ordered)) => {
                if lists.pop() == Some(true) {
                    out.push(Event::Html(if ordered { "</ol>\n" } else { "</ul>\n" }.into()));
                } else {
                    out.push(Event::End(TagEnd::List(ordered)));
                }
            }
            Event::Start(Tag::Item) => {
                let task = is_task(i);
                items.push(task);
                out.push(if task { Event::Html("<li class=\"task-list-item\">".into()) } else { e });
            }
            Event::End(TagEnd::Item) => {
                out.push(if items.pop() == Some(true) { Event::Html("</li>\n".into()) } else { e });
            }
            Event::Start(Tag::CodeBlock(_)) => {
                in_code += 1;
                out.push(e);
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = in_code.saturating_sub(1);
                out.push(e);
            }
            Event::Text(s) if in_code == 0 && links.is_empty() => linkify(&s, &mut out),
            e => out.push(e),
        }
        i += 1;
    }
    out
}

/// pulldown splits text at brackets, escapes, …; merge so a bare URL is seen whole.
fn merge_text(events: Vec<Event<'_>>) -> Vec<Event<'_>> {
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    for e in events {
        if let (Event::Text(s), Some(Event::Text(prev))) = (&e, out.last_mut()) {
            *prev = CowStr::from(format!("{prev}{s}"));
            continue;
        }
        out.push(e);
    }
    out
}

/// Markdig AutoLinks: bare `http(s)://…` / `www.…` after start, whitespace or `*_~(`, trailing punctuation and an
/// unbalanced `)` left out. ftp:/tel: would not be External, so they stay text anyway.
fn linkify<'a>(s: &str, out: &mut Vec<Event<'a>>) {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?i)(?:https?://|www\.)[^\s<]+").unwrap());
    let mut last = 0;
    for m in re.find_iter(s) {
        if s[..m.start()].chars().next_back().is_some_and(|c| !(c.is_whitespace() || "*_~(".contains(c))) {
            continue;
        }
        let mut u = m.as_str();
        loop {
            let trimmed = u.trim_end_matches(|c| "?!.,:;*_~'\"".contains(c));
            let trimmed = if trimmed.ends_with(')') && trimmed.matches(')').count() > trimmed.matches('(').count() {
                &trimmed[..trimmed.len() - 1]
            } else {
                trimmed
            };
            if trimmed.len() == u.len() {
                break;
            }
            u = trimmed;
        }
        let href = if u.to_ascii_lowercase().starts_with("www.") { format!("http://{u}") } else { u.to_string() };
        if scheme(&href).is_none() || (u.len() <= 4 && u.starts_with("www")) {
            continue;
        }
        if m.start() > last {
            out.push(Event::Text(s[last..m.start()].to_string().into()));
        }
        out.push(Event::InlineHtml(
            format!("<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">", escape_href(&href)).into(),
        ));
        out.push(Event::Text(u.to_string().into()));
        out.push(Event::InlineHtml("</a>".into()));
        last = m.start() + u.len();
    }
    if last < s.len() {
        out.push(Event::Text(s[last..].to_string().into()));
    }
}

/// pulldown-cmark-escape's `escape_href` (not a direct dependency): `&` and `'` as entities, unsafe bytes as %XX.
fn escape_href(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'&' => out.push_str("&amp;"),
            b'\'' => out.push_str("&#x27;"),
            b'!' | b'#'..=b'%' | b'('..=b';' | b'=' | b'?'..=b'Z' | b'^' | b'_' | b'a'..=b'z' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// GitHub alert: class suffix, `Md.Alert.*` word, octicon path (info, light-bulb, report, alert, stop) as Markdig emits.
/// ponytail: octicon paths reproduced from GitHub's markup; diff against a captured Markdig corpus if pixels matter.
fn alert(kind: BlockQuoteKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        BlockQuoteKind::Note => ("note", "Note", "M0 8a8 8 0 1 1 16 0A8 8 0 0 1 0 8Zm8-6.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13ZM6.5 7.75A.75.75 0 0 1 7.25 7h1a.75.75 0 0 1 .75.75v2.75h.25a.75.75 0 0 1 0 1.5h-2a.75.75 0 0 1 0-1.5h.25v-2h-.25a.75.75 0 0 1-.75-.75ZM8 6a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z"),
        BlockQuoteKind::Tip => ("tip", "Tip", "M8 1.5c-2.363 0-4 1.69-4 3.75 0 .984.424 1.625.984 2.304l.214.253c.223.264.47.556.673.848.284.411.537.896.621 1.49a.75.75 0 0 1-1.484.211c-.04-.282-.163-.547-.37-.847a8.456 8.456 0 0 0-.542-.68c-.084-.1-.173-.205-.268-.32C3.201 7.75 2.5 6.766 2.5 5.25 2.5 2.31 4.863 0 8 0s5.5 2.31 5.5 5.25c0 1.516-.701 2.5-1.328 3.259-.095.115-.184.22-.268.319-.207.245-.383.453-.541.681-.208.3-.33.565-.37.847a.751.751 0 0 1-1.485-.212c.084-.593.337-1.078.621-1.489.203-.292.45-.584.673-.848.075-.088.147-.173.213-.253.561-.679.985-1.32.985-2.304 0-2.06-1.637-3.75-4-3.75ZM5.75 12h4.5a.75.75 0 0 1 0 1.5h-4.5a.75.75 0 0 1 0-1.5ZM6 15.25a.75.75 0 0 1 .75-.75h2.5a.75.75 0 0 1 0 1.5h-2.5a.75.75 0 0 1-.75-.75Z"),
        BlockQuoteKind::Important => ("important", "Important", "M0 1.75C0 .784.784 0 1.75 0h12.5C15.216 0 16 .784 16 1.75v9.5A1.75 1.75 0 0 1 14.25 13H8.06l-2.573 2.573A1.458 1.458 0 0 1 3 14.543V13H1.75A1.75 1.75 0 0 1 0 11.25Zm1.75-.25a.25.25 0 0 0-.25.25v9.5c0 .138.112.25.25.25h2a.75.75 0 0 1 .75.75v2.19l2.72-2.72a.749.749 0 0 1 .53-.22h6.5a.25.25 0 0 0 .25-.25v-9.5a.25.25 0 0 0-.25-.25Zm7 2.25v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 9a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z"),
        BlockQuoteKind::Warning => ("warning", "Warning", "M6.457 1.047c.659-1.234 2.427-1.234 3.086 0l6.082 11.378A1.75 1.75 0 0 1 14.082 15H1.918a1.75 1.75 0 0 1-1.543-2.575Zm1.763.707a.25.25 0 0 0-.44 0L1.698 13.132a.25.25 0 0 0 .22.368h12.164a.25.25 0 0 0 .22-.368Zm.53 3.996v2.5a.75.75 0 0 1-1.5 0v-2.5a.75.75 0 0 1 1.5 0ZM9 11a1 1 0 1 1-2 0 1 1 0 0 1 2 0Z"),
        BlockQuoteKind::Caution => ("caution", "Caution", "M4.47.22A.749.749 0 0 1 5 0h6c.199 0 .389.079.53.22l4.25 4.25c.141.14.22.331.22.53v6a.749.749 0 0 1-.22.53l-4.25 4.25A.749.749 0 0 1 11 16H5a.749.749 0 0 1-.53-.22L.22 11.53A.749.749 0 0 1 0 11V5c0-.199.079-.389.22-.53Zm.84 1.28L1.5 5.31v5.38l3.81 3.81h5.38l3.81-3.81V5.31L10.69 1.5ZM8 4a.75.75 0 0 1 .75.75v3.5a.75.75 0 0 1-1.5 0v-3.5A.75.75 0 0 1 8 4Zm0 8a1 1 0 1 1 0-2 1 1 0 0 1 0 2Z"),
    }
}

/// The `.cb` wrapping, string-level exactly as Md.cs (raw HTML is escaped, so no model text can match).
fn wrap(html: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#"<pre><code(?: class="language-([^"]+)")?>"#).unwrap());
    let copy = t("Md.Copy");
    let html = re.replace_all(html, |c: &Captures| {
        format!(
            "<div class=\"cb\"><div class=\"h\"><span>{}</span><button class=\"copy\" type=\"button\" tabindex=\"0\"><svg class=\"i sm\"><use href=\"#i-copy\"/></svg><span>{copy}</span></button></div>{}",
            label_encoded(c.get(1).map(|m| m.as_str())),
            c[0].replace("<pre>", "<pre tabindex=\"0\">")
        )
    });
    html.replace("</code></pre>", "</code></pre></div>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{set_thread_lang, Lang};

    #[test]
    fn md_check() {
        set_thread_lang(Some(Lang::Fr));
        let cs = render("```csharp\nvar x = 1;\n```");
        assert!(cs.starts_with(r#"<div class="cb"><div class="h"><span>C#</span><button class="copy" "#), "{cs}");
        assert!(cs.contains(r#"<code class="language-csharp">"#) && cs.trim_end().ends_with("</code></pre></div>"), "{cs}");
        assert!(cs.contains(r#"<pre tabindex="0"><code"#) && cs.contains("<span>Copier</span>"), "{cs}");

        let bare = render("```\nplain\n```");
        assert!(bare.contains("<span>texte</span>") && bare.trim_end().ends_with("</div>"), "{bare}");

        let warn = render("> [!WARNING]\n> attention ici");
        assert!(warn.contains("markdown-alert-warning") && warn.contains("Attention</p>") && !warn.contains("Warning</p>"), "{warn}");
        assert!(warn.contains(r#"<div class="markdown-alert markdown-alert-warning">"#) && warn.contains("<p class=\"markdown-alert-title\"><svg"), "{warn}");

        assert!(render("<script>x</script>").contains("&lt;script&gt;"));

        let at = render("# a {onclick=x}") + &render("[x](https://a){onmouseover=\"y\"}");
        assert!(!at.contains(" onclick=\"") && !at.contains(" onmouseover=\""), "{at}");

        let js = render("[x](javascript:alert(1)) <javascript:alert(2)> [y](data:text/html,z) [r][1]\n\n[1]: javascript:alert(3)");
        assert!(!js.contains("href=\"javascript:") && !js.contains("href=\"data:") && !js.contains("<a "), "{js}");

        let img = render("![a](https://evil.example/?d=s) [ok](https://x.y/z) [rel](docs/a.md) [*f*](Program.cs:12) [#](#top)");
        assert!(!img.contains("<img"), "{img}");
        assert!(img.contains(r#"href="https://x.y/z" target="_blank" rel="noopener noreferrer""#), "{img}");
        assert!(img.contains("rel <em>f</em> #"), "{img}");
        assert_eq!(img.split("<a ").count(), 3, "{img}");

        let odd = render("```foo\"><b\nx\n```");
        assert!(!odd.contains("<b>"), "{odd}");
        set_thread_lang(None);
    }

    #[test]
    fn extras() {
        set_thread_lang(Some(Lang::En));
        // raw HTML block re-parsed as a paragraph, markdown inside still applies
        assert_eq!(render("<div>*x*</div>").trim(), "<p>&lt;div&gt;<em>x</em>&lt;/div&gt;</p>");
        // mailto kept without target, email autolink
        let m = render("[m](mailto:a@b.c) <a@b.c>");
        assert!(m.contains(r#"<a href="mailto:a@b.c">m</a>"#) && m.contains(r#"<a href="mailto:a@b.c">a@b.c</a>"#), "{m}");
        // bare URLs linked, trailing punctuation left out, not inside code
        let b = render("see https://x.y/a_(b). and www.z.io, `https://no.pe`");
        assert!(b.contains(r#"<a href="https://x.y/a_(b)" target="_blank" rel="noopener noreferrer">https://x.y/a_(b)</a>."#), "{b}");
        assert!(b.contains(r#"<a href="http://www.z.io" target="_blank" rel="noopener noreferrer">www.z.io</a>,"#), "{b}");
        assert!(b.contains("<code>https://no.pe</code>"), "{b}");
        // task list markup for app.css
        let tl = render("- [x] done\n- [ ] todo\n");
        assert!(tl.contains(r#"<ul class="contains-task-list">"#) && tl.contains(r#"<li class="task-list-item"><input"#), "{tl}");
        assert!(render("- a\n- b\n").contains("<ul>\n<li>a</li>"));
        // alerts localized per language
        assert!(render("> [!TIP]\n> x").contains("Tip</p>"));
        assert_eq!(label(Some("PS1")), "PowerShell");
        assert_eq!(label(Some("a\"b")), "a&quot;b");
        assert_eq!(label(None), "text");
        assert!(external(" HTTPS://x.y ") && external("mailto:a@b.c") && !external("http://") && !external("docs/a.md"));
        set_thread_lang(None);
    }
}
