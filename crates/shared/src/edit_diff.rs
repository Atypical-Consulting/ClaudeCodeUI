//! Port of the pure parts of `Core/EditDiff.cs` — spec 04 §7. Owner: core-worktrees-files.
//! No LCS: the edited block is cut to whole lines; common leading/trailing lines become context.
//! `read(path)` and `from_input` (which reads the file) are `ccui_core::edit_diff`.
//! Indices: work on byte offsets of `&str` consistently (only line splits matter).

use crate::json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CONTEXT: usize = 2;
/// A bigger file is diffed without line numbers.
pub const MAX_READ: u64 = 2_000_000;
/// replace_all previews only the first 20 occurrences.
pub const MAX_OCCURRENCES: usize = 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiffKind {
    Ctx,
    Add,
    Del,
    Hunk,
    More,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DiffLine {
    pub kind: DiffKind,
    /// Old number for Del, new number otherwise; More: occurrences not shown.
    pub n: Option<i32>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FileDiff {
    pub path: String,
    pub lines: Vec<DiffLine>,
}

fn line(kind: DiffKind, n: Option<i32>, text: &str) -> DiffLine {
    DiffLine { kind, n, text: text.to_string() }
}

/// `\r\n` → `\n`.
pub fn norm(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// [] for empty; else drop one trailing `\n` and split on `\n`.
pub fn lines(s: &str) -> Vec<&str> {
    if s.is_empty() {
        return Vec::new();
    }
    s.strip_suffix('\n').unwrap_or(s).split('\n').collect()
}

/// `Hunk("@@ -0,0 +1,{n} @@")` then `Add(i+1, line)`.
pub fn created(content: &str) -> Vec<DiffLine> {
    let content = norm(content);
    let ls = lines(&content);
    let mut res = vec![line(DiffKind::Hunk, None, &format!("@@ -0,0 +1,{} @@", ls.len()))];
    res.extend(ls.iter().enumerate().map(|(i, l)| line(DiffKind::Add, Some(i as i32 + 1), l)));
    res
}

/// Hunks for `(old, new)` edits against `file` (None = unreadable/unknown → unnumbered Del/Add); the
/// "already applied" rule rebuilds the pre-edit text so a replay shows the same diff.
pub fn edits(file: Option<&str>, edits: &[(String, String)], replace_all: bool) -> Vec<DiffLine> {
    let mut res = Vec::new();
    let mut text: Option<String> = file.map(String::from);
    let (mut shift, mut from) = (0isize, 0usize);
    for (old, new) in edits {
        if !replace_all {
            from = 0;
        }
        let find = |t: &str, pat: &str| t.get(from..).and_then(|rest| rest.find(pat)).map(|i| i + from);
        let mut pos = match &text {
            None => None,
            Some(_) if old.is_empty() => Some(0),
            Some(t) => find(t, old),
        };
        if pos.is_none() && !new.is_empty() {
            if let Some(p) = text.as_deref().and_then(|t| find(t, new)) {
                // already applied: rebuild the text before the edit
                let t = text.as_deref().unwrap();
                text = Some(format!("{}{}{}", &t[..p], old, &t[p + new.len()..]));
                pos = Some(p);
            }
        }
        let (Some(pos), Some(t)) = (pos, text.as_deref()) else {
            res.extend(lines(old).iter().map(|l| line(DiffKind::Del, None, l)));
            res.extend(lines(new).iter().map(|l| line(DiffKind::Add, None, l)));
            continue;
        };

        // Whole lines around the replaced text, before and after.
        let all = lines(t);
        let end = pos + old.len();
        let block_start = t[..pos].rfind('\n').map_or(0, |i| i + 1);
        let block_end = t[end..].find('\n').map_or(t.len(), |i| i + end);
        let edited = format!("{}{}{}", &t[..pos], new, &t[end..]);
        let old_block = lines(&t[block_start..block_end]);
        let new_block = lines(&edited[block_start..block_end + new.len() - old.len()]);
        let first = t[..block_start].matches('\n').count() as isize;
        let last = first + old_block.len() as isize - 1;

        let mut pre = 0usize;
        while pre < old_block.len() && pre < new_block.len() && old_block[pre] == new_block[pre] {
            pre += 1;
        }
        let mut post = 0usize;
        while post < old_block.len() - pre
            && post < new_block.len() - pre
            && old_block[old_block.len() - 1 - post] == new_block[new_block.len() - 1 - post]
        {
            post += 1;
        }

        let ctx = CONTEXT as isize;
        let (del_from, del_to) = (first + pre as isize, last - post as isize); // inclusive, 0-based, old text
        let ctx_from = (del_from - ctx).max(0);
        let add_count = new_block.len() as isize - pre as isize - post as isize;
        let ctx_after = ctx.min(all.len() as isize - 1 - del_to);
        let old_len = (del_to - ctx_from + 1) + ctx_after;
        let new_len = (del_from - ctx_from) + add_count + ctx_after;
        let n = |i: isize| Some(i as i32);
        res.push(line(DiffKind::Hunk, None, &format!("@@ -{},{old_len} +{},{new_len} @@", ctx_from + 1 - shift, ctx_from + 1)));
        for i in ctx_from..del_from {
            res.push(line(DiffKind::Ctx, n(i + 1), all[i as usize]));
        }
        for i in del_from..=del_to {
            res.push(line(DiffKind::Del, n(i + 1 - shift), all[i as usize]));
        }
        for i in 0..add_count {
            res.push(line(DiffKind::Add, n(del_from + i + 1), new_block[pre + i as usize]));
        }
        let delta = add_count - (del_to - del_from + 1);
        for i in 1..=ctx_after {
            res.push(line(DiffKind::Ctx, n(del_to + i + 1 + delta), all[(del_to + i) as usize]));
        }
        shift += delta;
        from = pos + new.len();
        text = Some(edited);
    }
    res
}

/// `from_input` given the file text the backend read (`file`) and whether the path exists.
pub fn from_input_with(tool: &str, input: &Value, file: Option<&str>, exists: bool) -> Vec<DiffLine> {
    let s = |v: &Value, k: &str| norm(json::str(v, k).unwrap_or(""));
    match tool {
        "Write" => {
            let content = s(input, "content");
            match file {
                // unreadable: not a new file
                None if exists => edits(None, &[(String::new(), content)], false),
                None => created(&content),
                Some(f) => edits(Some(f), &[(f.to_string(), content)], false),
            }
        }
        "MultiEdit" if json::prop(input, "edits").is_some_and(Value::is_array) => {
            let list: Vec<(String, String)> = input["edits"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| (s(e, "old_string"), s(e, "new_string")))
                .collect();
            edits(file, &list, false)
        }
        _ => {
            let (old, new) = (s(input, "old_string"), s(input, "new_string"));
            let file = match file {
                Some(f) if input.get("replace_all") == Some(&Value::Bool(true)) && !old.is_empty() => f,
                _ => return edits(file, &[(old, new)], false),
            };
            let count = file.matches(old.as_str()).count().max(1);
            let reps = vec![(old, new); count.min(MAX_OCCURRENCES)];
            let mut res = edits(Some(file), &reps, true); // one hunk per occurrence
            if count > MAX_OCCURRENCES {
                res.push(line(DiffKind::More, Some((count - MAX_OCCURRENCES) as i32), ""));
            }
            res
        }
    }
}

fn int(v: &Value, k: &str) -> i32 {
    v.get(k).and_then(Value::as_i64).unwrap_or(0) as i32
}

/// `tool_use_result.structuredPatch` hunks.
pub fn from_patch(structured_patch: &Value) -> Vec<DiffLine> {
    let mut res = Vec::new();
    let Some(hunks) = structured_patch.as_array() else { return res };
    for h in hunks {
        let (mut o, mut n) = (int(h, "oldStart"), int(h, "newStart"));
        res.push(line(DiffKind::Hunk, None, &format!("@@ -{o},{} +{n},{} @@", int(h, "oldLines"), int(h, "newLines"))));
        let Some(ls) = json::prop(h, "lines").and_then(Value::as_array) else { continue };
        for l in ls.iter().map(|x| x.as_str().unwrap_or("")) {
            let first = l.chars().next();
            let rest = first.map_or("", |c| &l[c.len_utf8()..]);
            match first {
                Some('-') => {
                    res.push(line(DiffKind::Del, Some(o), rest));
                    o += 1;
                }
                Some('+') => {
                    res.push(line(DiffKind::Add, Some(n), rest));
                    n += 1;
                }
                Some('\\') => {} // "\ No newline at end of file"
                _ => {
                    res.push(line(DiffKind::Ctx, Some(n), rest));
                    n += 1;
                    o += 1;
                }
            }
        }
    }
    res
}

/// `tool_use_result.bashEditDiff.files[]`; a file with no hunks gets `Hunk(None, Diff.NoHunks)`.
pub fn from_bash(structured: &Value) -> Vec<FileDiff> {
    let Some(files) = json::prop(structured, "bashEditDiff").and_then(|b| json::prop(b, "files")).and_then(Value::as_array) else {
        return Vec::new();
    };
    files
        .iter()
        .map(|f| {
            let mut lines = from_patch(json::get(f, "hunks"));
            if lines.is_empty() {
                lines.push(line(DiffKind::Hunk, None, &crate::t!("Diff.NoHunks")));
            }
            FileDiff { path: json::string(f, "filePath"), lines }
        })
        .collect()
}

static HUNK: std::sync::LazyLock<regex::Regex> =
    std::sync::LazyLock::new(|| regex::Regex::new(r"^@@ -(\d+)(?:,\d+)? \+(\d+)").unwrap());

/// Unified `git diff` output → files (the /diff panel).
pub fn from_git(unified: &str) -> Vec<FileDiff> {
    let mut files: Vec<FileDiff> = Vec::new();
    let mut hunk = false;
    let (mut o, mut n) = (0i32, 0i32);
    for raw in unified.split('\n') {
        let l = raw.trim_end_matches('\r');
        if let Some(rest) = l.strip_prefix("diff --git ") {
            let path = match l.rfind(" b/") {
                Some(bi) if bi > 0 => &l[bi + 3..],
                _ => rest,
            };
            files.push(FileDiff { path: path.to_string(), lines: Vec::new() });
            hunk = false;
            continue;
        }
        let Some(cur) = files.last_mut() else { continue };
        if !hunk && l.starts_with("+++ ") && l != "+++ /dev/null" {
            cur.path = l.strip_prefix("+++ b/").unwrap_or(&l[4..]).to_string();
        } else if !hunk && l.starts_with("--- a/") && cur.path.is_empty() {
            cur.path = l[6..].to_string();
        } else if !hunk && l.starts_with("Binary files ") {
            cur.lines.push(line(DiffKind::Hunk, None, l));
        } else if let Some(m) = HUNK.captures(l) {
            o = m[1].parse().unwrap_or(0);
            n = m[2].parse().unwrap_or(0);
            hunk = true;
            cur.lines.push(line(DiffKind::Hunk, None, l));
        } else if hunk {
            match l.as_bytes().first() {
                Some(b'-') => {
                    cur.lines.push(line(DiffKind::Del, Some(o), &l[1..]));
                    o += 1;
                }
                Some(b'+') => {
                    cur.lines.push(line(DiffKind::Add, Some(n), &l[1..]));
                    n += 1;
                }
                Some(b' ') => {
                    cur.lines.push(line(DiffKind::Ctx, Some(n), &l[1..]));
                    n += 1;
                    o += 1;
                }
                _ => {}
            }
        }
    }
    files
}

/// highlight.js language by extension (`data-hl`), "" when unknown.
pub fn lang(path: &str) -> &'static str {
    let seg = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let ext = match seg.rfind('.') {
        Some(i) if i + 1 < seg.len() => seg[i..].to_lowercase(),
        _ => String::new(),
    };
    match ext.as_str() {
        ".cs" | ".razor" | ".cshtml" => "csharp",
        ".js" | ".mjs" | ".cjs" => "javascript",
        ".ts" | ".tsx" => "typescript",
        ".json" => "json",
        ".css" => "css",
        ".html" | ".htm" | ".xml" | ".csproj" | ".props" | ".targets" | ".svg" => "xml",
        ".md" => "markdown",
        ".sh" => "bash",
        ".ps1" | ".psm1" => "powershell",
        ".yml" | ".yaml" => "yaml",
        ".py" => "python",
        ".sql" => "sql",
        _ => "",
    }
}

/// `Kind N:Text` joined by `|` (the C# Check's Show).
#[cfg(test)]
pub(crate) fn show(d: &[DiffLine]) -> String {
    d.iter()
        .map(|l| format!("{:?}{}:{}", l.kind, l.n.map(|n| n.to_string()).unwrap_or_default(), l.text))
        .collect::<Vec<_>>()
        .join("|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEN: &str = "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n";

    fn edit(old: &str, new: &str) -> Value {
        json!({"file_path": "/x", "old_string": old, "new_string": new})
    }

    #[test]
    fn edits_on_a_file() {
        let f = Some(TEN);
        assert_eq!(show(&from_input_with("Edit", &edit("5", "cinq"), f, true)), "Hunk:@@ -3,5 +3,5 @@|Ctx3:3|Ctx4:4|Del5:5|Add5:cinq|Ctx6:6|Ctx7:7");
        assert_eq!(show(&from_input_with("Edit", &edit("2\n", "2\nbis\n"), f, true)), "Hunk:@@ -1,4 +1,5 @@|Ctx1:1|Ctx2:2|Add3:bis|Ctx4:3|Ctx5:4");
        let w = json!({"file_path": "/x.absent", "content": "x\ny\n"});
        assert_eq!(show(&from_input_with("Write", &w, None, false)), "Hunk:@@ -0,0 +1,2 @@|Add1:x|Add2:y");
        let m = json!({"file_path": "/x", "edits": [{"old_string": "2", "new_string": "deux\ndeux bis"}, {"old_string": "9", "new_string": "neuf"}]});
        let multi = from_input_with("MultiEdit", &m, f, true);
        let s = show(&multi);
        assert_eq!(multi.iter().filter(|l| l.kind == DiffKind::Hunk).count(), 2);
        assert!(s.contains("Del2:2|Add2:deux|Add3:deux bis") && s.contains("Hunk:@@ -7,4 +8,4 @@|Ctx8:7|Ctx9:8|Del9:9|Add10:neuf"), "{s}");
        let every = from_input_with("Edit", &json!({"file_path": "/x", "old_string": "1", "new_string": "1x", "replace_all": true}), f, true);
        let s = show(&every);
        assert!(every.iter().filter(|l| l.kind == DiffKind::Add).count() == 2 && s.contains("Add1:1x") && s.contains("Add10:1x0"), "{s}");
        let many = "x\n".repeat(MAX_OCCURRENCES + 5);
        let capped = from_input_with("Edit", &json!({"file_path": "/m", "old_string": "x", "new_string": "y", "replace_all": true}), Some(&many), true);
        assert_eq!(capped.iter().filter(|l| l.kind == DiffKind::Hunk).count(), MAX_OCCURRENCES);
        assert_eq!((capped.last().unwrap().kind, capped.last().unwrap().n), (DiffKind::More, Some(5)));
        // Unreadable (too big, link to a device, missing): unnumbered block; a Write over it is not a creation.
        assert_eq!(show(&from_input_with("Edit", &edit("a", "b"), None, true)), "Del:a|Add:b");
        assert_eq!(show(&from_input_with("Write", &json!({"file_path": "/b", "content": "c"}), None, true)), "Add:c");
        assert_eq!(show(&from_input_with("Edit", &edit("a", "b"), None, false)), "Del:a|Add:b");
        // Before approval = structuredPatch after; the same after the fact (already applied).
        let patch = from_patch(&json!([{"oldStart": 1, "oldLines": 1, "newStart": 1, "newLines": 1, "lines": ["-hello", "+bye"]}]));
        assert_eq!(show(&patch), "Hunk:@@ -1,1 +1,1 @@|Del1:hello|Add1:bye");
        assert_eq!(show(&from_input_with("Edit", &edit("hello", "bye"), Some("hello\n"), true)), show(&patch));
        assert_eq!(show(&from_input_with("Edit", &edit("hello", "bye"), Some("bye\n"), true)), show(&patch));
        // Write over an existing (here empty) file.
        assert_eq!(show(&from_input_with("Write", &json!({"file_path": "/e", "content": "x\n"}), Some(""), true)), "Hunk:@@ -1,0 +1,1 @@|Add1:x");
    }

    #[test]
    fn patches_bash_git() {
        let ctx = from_patch(&json!([{"oldStart": 3, "oldLines": 3, "newStart": 3, "newLines": 4, "lines": [" a", "-b", "+B", "+C", " d", "\\ No newline at end of file"]}]));
        assert_eq!(show(&ctx), "Hunk:@@ -3,3 +3,4 @@|Ctx3:a|Del4:b|Add4:B|Add5:C|Ctx6:d");
        assert!(from_patch(&json!({"x": 1})).is_empty());
        let b: Value = serde_json::from_str(r#"{"stdout":"","stderr":"","interrupted":false,"bashEditDiff":{"files":[{"filePath":"/w/a.txt","hunks":[{"oldStart":1,"oldLines":1,"newStart":1,"newLines":1,"lines":["-hello","+bye"]}]}],"moreFiles":0}}"#).unwrap();
        let fb = from_bash(&b);
        assert_eq!(fb.len(), 1);
        assert_eq!((fb[0].path.as_str(), show(&fb[0].lines)), ("/w/a.txt", "Hunk:@@ -1,1 +1,1 @@|Del1:hello|Add1:bye".to_string()));
        assert!(from_bash(&json!({"stdout": "x"})).is_empty() && from_bash(&Value::Null).is_empty());
        let nh = from_bash(&json!({"bashEditDiff": {"files": [{"filePath": "/w/a.png", "hunks": [], "created": true}], "moreFiles": 0}}));
        assert!(nh.len() == 1 && nh[0].lines.len() == 1 && nh[0].lines[0].kind == DiffKind::Hunk && nh[0].lines[0].n.is_none());

        let git = from_git(
            &[
                "diff --git a/src/a.cs b/src/a.cs", "index 1111111..2222222 100644", "--- a/src/a.cs", "+++ b/src/a.cs",
                "@@ -3,3 +3,3 @@ class A", " a", "-b", "+B", " c",
                "diff --git a/old.txt b/old.txt", "deleted file mode 100644", "index 3333333..0000000", "--- a/old.txt", "+++ /dev/null", "@@ -1 +0,0 @@", "-gone",
                "diff --git a/logo.png b/logo.png", "index 4444444..5555555 100644", "Binary files a/logo.png and b/logo.png differ",
                "diff --git a/n.md b/n.md", "new file mode 100644", "--- /dev/null", "+++ b/n.md", "@@ -0,0 +1 @@", "+hi", "\\ No newline at end of file", "",
            ]
            .join("\n"),
        );
        let paths: Vec<&str> = git.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["src/a.cs", "old.txt", "logo.png", "n.md"]);
        assert_eq!(show(&git[0].lines), "Hunk:@@ -3,3 +3,3 @@ class A|Ctx3:a|Del4:b|Add4:B|Ctx5:c");
        assert_eq!(show(&git[1].lines), "Hunk:@@ -1 +0,0 @@|Del1:gone");
        assert!(git[2].lines.len() == 1 && git[2].lines[0].kind == DiffKind::Hunk);
        assert_eq!(show(&git[3].lines), "Hunk:@@ -0,0 +1 @@|Add1:hi");
        assert!(from_git("").is_empty());
    }

    #[test]
    fn helpers() {
        assert_eq!(lines(""), Vec::<&str>::new());
        assert_eq!(lines("a\nb\n"), ["a", "b"]);
        assert_eq!(norm("a\r\nb"), "a\nb");
        assert_eq!(lang("src/A.CS"), "csharp");
        assert_eq!(lang("x/.md"), "markdown");
        assert_eq!(lang("Makefile"), "");
        assert_eq!(lang("a.d/b"), "");
    }
}
