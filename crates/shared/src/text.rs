//! Port of the text/path helpers of `Core/TranscriptStore.cs` (shared with Overview, CommandPalette, NewSession) —
//! spec 03 §1.9, plus `NewSession.Chip` (spec 06 §6.1). Owner: core-store-sessions.
//! Lengths are in chars (C# counted UTF-16 units); always slice on char boundaries.

use regex::Regex;
use std::sync::LazyLock;
use unicode_general_category::{get_general_category, GeneralCategory};
use unicode_normalization::UnicodeNormalization;

fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// First `n` chars of `s`.
fn take(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(i, _)| &s[..i])
}

/// `s` from char `n` on.
fn skip(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or("", |(i, _)| &s[i..])
}

/// Char index of byte offset `b`.
fn char_at(s: &str, b: usize) -> usize {
    s[..b].chars().count()
}

/// Collapse whitespace runs; > 60 chars → first 59 + "…"; blank → None.
pub fn trunc(s: Option<&str>) -> Option<String> {
    let s = collapse(s?);
    if s.is_empty() {
        return None;
    }
    Some(if s.chars().count() <= 60 { s } else { format!("{}…", take(&s, 59)) })
}

/// ≤ 220 chars → s, else first 219 trimmed + "…".
pub fn cut(s: &str) -> String {
    if s.chars().count() <= 220 {
        s.to_string()
    } else {
        format!("{}…", take(s, 219).trim_end())
    }
}

/// Markdown → plain one line (fences, link labels, `plain`, list/quote markers, emoji incl. every char > U+FFFF).
pub fn prose(md: &str) -> String {
    static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)```.*?(```|$)").unwrap());
    static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"!?\[([^\]]*)\]\([^)]*\)").unwrap());
    static MARKER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]*(>|[-*+]|\d+\.)[ \t]+").unwrap());
    // .NET's \p{Cs} (UTF-16 surrogates) drops every char above U+FFFF.
    static EMOJI: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"[\p{So}\x{FE0F}\x{200D}\x{10000}-\x{10FFFF}]+").unwrap());
    let md = FENCE.replace_all(md, " ");
    let md = LINK.replace_all(&md, "${1}");
    let md = plain(&md);
    let md = MARKER.replace_all(&md, "");
    let md = EMOJI.replace_all(&md, "");
    collapse(&md)
}

/// Markdown as read: headings, `**`, backticks and table rules removed; table rows → cells joined " · ".
pub fn plain(t: &str) -> String {
    static MARKS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)^[ \t]*#{1,6}[ \t]+|\*\*|`|^[ \t|:-]*-{3}[ \t|:-]*$").unwrap());
    static ROW: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[ \t]*\|.*$").unwrap());
    let t = MARKS.replace_all(t, "");
    ROW.replace_all(&t, |c: &regex::Captures| {
        c[0].split('|').map(str::trim).filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ")
    })
    .into_owned()
}

/// The last ~max chars started at a sentence (or the trailing question) when possible.
pub fn tail(s: &str, max: usize) -> String {
    static QUESTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^.!?…]*\?").unwrap());
    static SENTENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.!?…]\s+").unwrap());
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    if n <= max * 6 / 5 && !s.starts_with('…') {
        return s.to_string();
    }
    let s = s.trim_start_matches('…');
    let n = s.chars().count();
    // A question near the end is the ask (C# `(?=\s|$)` checked by hand).
    let ask = QUESTION
        .find_iter(s)
        .filter(|m| s[m.end()..].chars().next().is_none_or(char::is_whitespace))
        .last();
    if let Some(q) = ask {
        let trimmed = q.as_str().trim();
        let len = trimmed.chars().count();
        if char_at(s, q.start()) as i64 >= n as i64 - 2 * max as i64 && (8..=max).contains(&len) {
            return trimmed.to_string();
        }
    }
    let t = skip(s, n.saturating_sub(max - 1));
    let head = take(t, max * 17 / 20);
    // `(?=\S)` inside the head: the whitespace run must stop before the head's end.
    if let Some(m) = SENTENCE.find_iter(head).find(|m| m.end() < head.len()) {
        return t[m.end()..].to_string();
    }
    match t.find(' ').map(|b| (b, char_at(t, b))) {
        Some((b, w)) if w < max / 3 => format!("…{}", &t[b + 1..]),
        _ => format!("…{t}"),
    }
}

/// Whitespace-collapsed `text[at-ctx .. at+length+ctx]` with "…" on cut sides (`at`, `length` in chars).
pub fn snippet(text: &str, at: usize, length: usize, ctx: usize) -> String {
    let n = text.chars().count();
    let from = at.saturating_sub(ctx);
    let to = n.min(at + length + ctx);
    let s = collapse(take(skip(text, from), to.saturating_sub(from)));
    format!("{}{}{}", if from > 0 { "…" } else { "" }, s, if to < n { "…" } else { "" })
}

/// Every char not `[A-Za-z0-9]` → '-' (the CLI's project-dir naming).
pub fn slug(cwd: &str) -> String {
    // Per UTF-16 unit, like the CLI: a char above U+FFFF becomes "--".
    cwd.chars()
        .flat_map(|c| match c {
            c if c.is_ascii_alphanumeric() => [Some(c), None],
            c if (c as u32) > 0xFFFF => [Some('-'), Some('-')],
            _ => [Some('-'), None],
        })
        .flatten()
        .collect()
}

/// Last segment, cut at `/.claude/worktrees/` (both separators accepted).
pub fn repo_name(cwd: &str) -> String {
    let cwd = cwd.replace('\\', "/");
    let cwd = cwd.trim_end_matches('/');
    let cut = match cwd.to_ascii_lowercase().find("/.claude/worktrees/") {
        Some(i) if i > 0 => &cwd[..i],
        _ => cwd,
    };
    cut.rsplit('/').next().unwrap_or_default().to_string()
}

/// cwd before `[\\/]\.claude[\\/]worktrees[\\/]` (ignore case), else cwd.
pub fn root_of(cwd: &str) -> String {
    static WT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)[\\/]\.claude[\\/]worktrees[\\/]").unwrap());
    match WT.find(cwd) {
        Some(m) if m.start() > 0 => cwd[..m.start()].to_string(),
        _ => cwd.to_string(),
    }
}

/// Ordinal ignore-case char equality (simple upper mapping, like .NET's OrdinalIgnoreCase).
pub fn eq_ci(a: char, b: char) -> bool {
    a == b || {
        let up = |c: char| {
            let mut u = c.to_uppercase();
            match (u.next(), u.next()) {
                (Some(x), None) => x,
                _ => c,
            }
        };
        up(a) == up(b)
    }
}

/// Char index of the first ordinal ignore-case occurrence of `needle` in `hay` (`IndexOf(…, OrdinalIgnoreCase)`).
pub fn find_ci(hay: &str, needle: &str) -> Option<usize> {
    let h: Vec<char> = hay.chars().collect();
    let n: Vec<char> = needle.chars().collect();
    if n.is_empty() {
        return Some(0);
    }
    (0..(h.len() + 1).saturating_sub(n.len())).find(|&i| h[i..i + n.len()].iter().zip(&n).all(|(a, b)| eq_ci(*a, *b)))
}

/// `home + sep` prefix (case-insensitive) → "~" + rest.
pub fn short(path: &str, home: &str, sep: &str) -> String {
    let prefix = format!("{home}{sep}");
    let k = prefix.chars().count();
    let head = take(path, k);
    if head.chars().count() == k && head.chars().zip(prefix.chars()).all(|(a, b)| eq_ci(a, b)) {
        format!("~{}", skip(path, home.chars().count()))
    } else {
        path.to_string()
    }
}

/// NFD, marks dropped, lowercase, `[a-z0-9]+` words, first `words`, joined '-'.
pub fn name_from(text: &str, words: usize) -> String {
    static WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[a-z0-9]+").unwrap());
    let plain: String =
        text.nfd().filter(|c| get_general_category(*c) != GeneralCategory::NonspacingMark).collect::<String>().to_lowercase();
    WORD.find_iter(&plain).take(words).map(|m| m.as_str()).collect::<Vec<_>>().join("-")
}

/// `name_from(typed, 8)` or None when empty.
pub fn worktree_name(typed: &str) -> Option<String> {
    Some(name_from(typed, 8)).filter(|n| !n.is_empty())
}

/// `adj-ing-animal` from the three fixed 20-word lists; `seed` picks (the UI passes a random u64).
pub fn random_name(seed: u64) -> String {
    const ADJ: [&str; 20] = [
        "brave", "calm", "eager", "fuzzy", "gentle", "happy", "jolly", "keen", "lazy", "lucky", "mellow", "nimble",
        "quiet", "rapid", "shiny", "snug", "sunny", "swift", "tidy", "witty",
    ];
    const ING: [&str; 20] = [
        "baking", "dancing", "dozing", "drifting", "floating", "gliding", "hiding", "humming", "juggling", "napping",
        "painting", "roaming", "sailing", "singing", "skating", "snacking", "spinning", "wandering", "whistling",
        "zooming",
    ];
    const ANIMAL: [&str; 20] = [
        "badger", "beaver", "bison", "crane", "falcon", "ferret", "gecko", "heron", "hopper", "koala", "lemur", "lynx",
        "marmot", "otter", "panda", "puffin", "quokka", "raven", "tapir", "walrus",
    ];
    let i = |k: u64| (seed / k % 20) as usize;
    format!("{}-{}-{}", ADJ[i(1)], ING[i(20)], ANIMAL[i(400)])
}

/// PR number of `42`, `#42`, `…/pull/42/`, `…/-/merge_requests/7`; None for issues or `v42`.
pub fn pr_number(s: &str) -> Option<String> {
    static PR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?:^#?|/(?:pull|pulls|merge_requests|pull-requests)/)(\d+)/?$").unwrap());
    PR.captures(s.trim()).map(|c| c[1].to_string())
}

/// `NewSession.Chip`: `short(dir)` with `\` → `/`; ≤ 32 chars as is, else "…/" + last two segments.
pub fn chip(dir: &str, home: &str, sep: &str) -> String {
    let s = short(dir, home, sep).replace('\\', "/");
    if s.chars().count() <= 32 {
        return s;
    }
    let parts: Vec<&str> = s.split('/').collect();
    format!("…/{}", parts[parts.len().saturating_sub(2)..].join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prose_and_plain() {
        assert_eq!(prose("✅ Merged 🚀 : ok"), "Merged : ok");
        assert_eq!(prose("## Fait\n\n- **PR #12** mergée, voir [la PR](https://x/12)."), "Fait PR #12 mergée, voir la PR.");
        assert_eq!(prose("a ```code\nx``` b"), "a b");
        let p = plain("## Plan\n- **Dir:** `/a/b` use it\n\n| # | x |\n|---|---|\n| | 3 | Contrôle |");
        assert_eq!(collapse(&p), "Plan - Dir: /a/b use it # · x 3 · Contrôle");
    }

    #[test]
    fn tail_cases() {
        let tl = tail(&format!("{}. Tout est vert. On publie la 0.4 maintenant ?", "a".repeat(600)), 220);
        assert!(tl.ends_with("maintenant ?") && tl.chars().count() <= 221, "{tl}");
        let s = format!("Veux-tu que j'enquête ? {}", vec!["Elle a été enregistrée sans identifiant."; 4].join(" "));
        assert_eq!(tail(&s, 110), "Veux-tu que j'enquête ?");
        assert!(tail(&format!("Le build passe. {}", "b".repeat(110)), 110).starts_with("Le build"));
        let tw = tail(&vec!["hierarchy"; 40].join(" "), 110);
        assert!(tw.starts_with("…hierarchy") && tw.chars().count() <= 111, "{tw}");
        assert_eq!(tail("short", 220), "short");
    }

    #[test]
    fn snippet_trunc_cut() {
        assert_eq!(snippet("a  b\nauth c", 6, 4, 40), "a b auth c");
        let long = format!("{}auth{}", "x".repeat(100), "y".repeat(100));
        assert_eq!(snippet(&long, 100, 4, 3), "…xxxauthyyy…");
        assert_eq!(trunc(Some("corrige   le test\nd'auth")).as_deref(), Some("corrige le test d'auth"));
        assert_eq!(trunc(Some("  \n")), None);
        assert_eq!(trunc(None), None);
        assert_eq!(trunc(Some(&"é".repeat(70))).unwrap(), format!("{}…", "é".repeat(59)));
        assert_eq!(cut(&"a".repeat(220)), "a".repeat(220));
        assert_eq!(cut(&format!("{} {}", "a".repeat(218), "b".repeat(10))), format!("{}…", "a".repeat(218)));
    }

    #[test]
    fn names() {
        assert_eq!(name_from("Ajoute la persistance des sessions : un fichier", 4), "ajoute-la-persistance-des");
        assert_eq!(name_from("Évite l'échec", 4), "evite-l-echec");
        for seed in 0..50u64 {
            let n = random_name(seed * 7919 + 13);
            assert_eq!(n.split('-').count(), 3);
            assert_eq!(worktree_name(&n).as_deref(), Some(n.as_str()));
        }
        assert_eq!(worktree_name(" Fix Auth! ").as_deref(), Some("fix-auth"));
        assert_eq!(worktree_name(" -!- "), None);
    }

    #[test]
    fn paths() {
        assert_eq!(repo_name(r"C:\repo\api\.claude\worktrees\x"), "api");
        assert_eq!(repo_name(r"C:\repo\api"), "api");
        assert_eq!(repo_name("/r/app/"), "app");
        assert_eq!(root_of("/r/api/.claude/worktrees/x"), "/r/api");
        assert_eq!(root_of(r"C:\r\api\.Claude\Worktrees\x"), r"C:\r\api");
        assert_eq!(root_of("/r/api"), "/r/api");
        assert_eq!(slug("/Users/me/repo/.claude/worktrees/x"), "-Users-me-repo--claude-worktrees-x");
        assert_eq!(slug("a😀b"), "a--b");
        assert_eq!(short("/Users/Me/repo", "/users/me", "/"), "~/repo");
        assert_eq!(short("/Users/meh", "/Users/me", "/"), "/Users/meh");
        assert_eq!(chip("/Users/me/repo", "/Users/me", "/"), "~/repo");
        assert_eq!(chip("/very/long/path/that/goes/on/and/on/x/y", "/Users/me", "/"), "…/x/y");
        assert_eq!(chip(r"C:\Users\me\a", r"C:\Users\me", "\\"), "~/a");
        assert_eq!(find_ci("corrige le test d'AUTH", "auth"), Some(18));
        assert_eq!(find_ci("é Auth", "AUTH"), Some(2));
    }

    #[test]
    fn pr_numbers() {
        assert_eq!(pr_number("42").as_deref(), Some("42"));
        assert_eq!(pr_number(" #42 ").as_deref(), Some("42"));
        assert_eq!(pr_number("https://github.com/o/r/pull/42/").as_deref(), Some("42"));
        assert_eq!(pr_number("https://gitlab.com/g/s/r/-/merge_requests/7").as_deref(), Some("7"));
        assert_eq!(pr_number("https://github.com/o/r/issues/42"), None);
        assert_eq!(pr_number("v42"), None);
    }
}
