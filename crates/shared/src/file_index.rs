//! Port of the pure parts of `Core/FileIndex.cs` ("@" mentions in the Composer) — spec 04 §3.3.
//! Owner: core-worktrees-files. The scan (git ls-files / walk) and `inside` are `ccui_core::file_index`.
//! The UI fetches the index once per cwd (`file_index_scan`) and runs these on each keystroke.

pub const MAX_ENTRIES: usize = 20_000;
pub const MAX_RESULTS: usize = 10;

/// The live `@` query: after the last '@' preceded by whitespace (or at start), no whitespace or '"' in it.
/// `"look at @src/Fo"` → `"src/Fo"`, `"@"` → `""`, `"mail me@host.com"` → None.
pub fn query(text: &str) -> Option<String> {
    let at = text.rfind('@')?;
    if text[..at].chars().next_back().is_some_and(|c| !c.is_whitespace()) {
        return None; // an e-mail address is not a mention
    }
    let q = &text[at + 1..];
    (!q.chars().any(|c| c.is_whitespace() || c == '"')).then(|| q.to_string())
}

/// Replace from the last '@' with `@path ` (or `@"path" ` when it has whitespace); no '@' → unchanged.
pub fn insert(text: &str, path: &str) -> String {
    let Some(at) = text.rfind('@') else { return text.to_string() }; // a late click after the '@' was deleted
    if path.chars().any(char::is_whitespace) {
        format!("{}@\"{path}\" ", &text[..at])
    } else {
        format!("{}@{path} ", &text[..at])
    }
}

/// Last segment, keeping a folder's trailing '/': `"src/Bar/"` → `"Bar/"`.
pub fn name(path: &str) -> &str {
    let end = path.len().saturating_sub(1).max(path.len().min(1)); // search [0, max(len-2, 0)]
    match path.as_bytes()[..end].iter().rposition(|&b| b == b'/') {
        Some(i) => &path[i + 1..],
        None => path,
    }
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// 0 name starts with q, 1 name contains, 2 path contains, 3 subsequence, -1 none (case-insensitive).
pub fn score(path: &str, q: &str) -> i32 {
    let (lq, lname, lpath) = (q.to_lowercase(), name(path).to_lowercase(), path.to_lowercase());
    if lname.starts_with(&lq) {
        return 0;
    }
    if lname.contains(&lq) {
        return 1;
    }
    if lpath.contains(&lq) {
        return 2;
    }
    let qs: Vec<char> = q.chars().map(lower).collect();
    let mut i = 0;
    for c in path.chars() {
        if i < qs.len() && lower(c) == qs[i] {
            i += 1;
        }
    }
    if i == qs.len() {
        3
    } else {
        -1
    }
}

/// OrdinalIgnoreCase ordering key.
fn ci(s: &str) -> String {
    s.to_uppercase()
}

/// Empty query: by depth then case-insensitively; else score ≥ 0 by (score, len, path). Take `max`.
pub fn filter(paths: &[String], query: &str, max: usize) -> Vec<String> {
    if query.is_empty() {
        let mut v: Vec<&String> = paths.iter().collect();
        v.sort_by_cached_key(|p| (p.trim_end_matches('/').matches('/').count(), ci(p)));
        return v.into_iter().take(max).cloned().collect();
    }
    let mut v: Vec<(i32, &String)> = paths.iter().map(|p| (score(p, query), p)).filter(|(s, _)| *s >= 0).collect();
    v.sort_by_cached_key(|(s, p)| (*s, p.encode_utf16().count(), ci(p)));
    v.into_iter().take(max).map(|(_, p)| p.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn query_insert_name() {
        assert_eq!(query("look at @src/Fo").as_deref(), Some("src/Fo"));
        assert_eq!(query("@").as_deref(), Some(""));
        assert_eq!(query("mail me@host.com"), None);
        assert_eq!(query("@src/Foo.cs then"), None);
        assert_eq!(query("no mention"), None);
        assert_eq!(query("fix @src/Foo.cs").as_deref(), Some("src/Foo.cs"));
        assert_eq!(insert("fix @Fo", "src/Foo.cs"), "fix @src/Foo.cs ");
        assert_eq!(insert("@my", "my notes/a b.txt"), "@\"my notes/a b.txt\" ");
        assert_eq!(insert("no mention left", "src/Foo.cs"), "no mention left");
        assert_eq!(name("src/Bar/"), "Bar/");
        assert_eq!(name("src/Foo.cs"), "Foo.cs");
        assert_eq!(name("a"), "a");
        assert_eq!(name(""), "");
        assert_eq!(name("/"), ""); // C#: LastIndexOf('/', 0) finds index 0
    }

    #[test]
    fn filter_ranking() {
        let paths = v(&["README.md", "src/", "src/Foo.cs", "src/Bar/FooBar.cs", "docs/foo-guide.md", "tests/", "tests/FooTests.cs", "Fmt.cs"]);
        assert_eq!(filter(&paths, "foo", MAX_RESULTS), v(&["src/Foo.cs", "docs/foo-guide.md", "src/Bar/FooBar.cs", "tests/FooTests.cs"]));
        assert_eq!(filter(&paths, "sfc", MAX_RESULTS), v(&["src/Foo.cs", "src/Bar/FooBar.cs", "tests/FooTests.cs"]));
        assert_eq!(filter(&paths, "src/", MAX_RESULTS), v(&["src/", "src/Foo.cs", "src/Bar/FooBar.cs"]));
        assert_eq!(filter(&paths, "", 4), v(&["Fmt.cs", "README.md", "src/", "tests/"]));
        assert!(filter(&paths, "zzz", MAX_RESULTS).is_empty());
        assert_eq!(filter(&paths, "", 2).len(), 2);
        assert_eq!(filter(&v(&["agent-statusline-setup", "tests/"]), "test", MAX_RESULTS), v(&["tests/", "agent-statusline-setup"]));
    }
}
