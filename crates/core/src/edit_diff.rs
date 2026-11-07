//! Port of the I/O parts of `Core/EditDiff.cs` (`Read`, `FromInput`) and the /diff panel's git read — spec 04 §7,
//! spec 07 §17 (DiffPanel). Owner: core-worktrees-files. Pure diffing: `ccui_shared::edit_diff`.

use ccui_shared::api::GitDiff;
use ccui_shared::edit_diff::{self as pure, DiffKind, DiffLine, FileDiff, MAX_READ};
use serde_json::Value;
use std::path::Path;

/// A bigger new file is listed, not shown (DiffPanel).
const MAX_UNTRACKED: u64 = 256 * 1024;

/// File text for diffing: symlink → final target (non-file → None); missing or > MAX_READ → None; length 0 → ""
/// WITHOUT opening (devices, FIFOs); UTF-8, `\r\n` normalized; any error → None.
pub fn read(path: &str) -> Option<String> {
    if path.is_empty() {
        return None;
    }
    // A symlink's own length is the link's size: check its final target instead.
    let mut meta = std::fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() {
        meta = std::fs::metadata(path).ok()?;
    }
    if !meta.is_file() || meta.len() > MAX_READ {
        return None;
    }
    // Length 0: never open it. Devices and FIFOs report 0 and would block or never end.
    if meta.len() == 0 {
        return Some(String::new());
    }
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    Some(pure::norm(text.strip_prefix('\u{feff}').unwrap_or(&text)))
}

/// Diff of an Edit / Write / MultiEdit input before approval (reads the file).
pub fn from_input(tool: &str, input: &Value) -> Vec<DiffLine> {
    let path = ccui_shared::json::str(input, "file_path").unwrap_or("");
    let file = read(path);
    let exists = !path.is_empty() && Path::new(path).is_file();
    pure::from_input_with(tool, input, file.as_deref(), exists)
}

/// Uncommitted changes of the session folder (`git diff HEAD` incl. untracked as new files), parsed by `from_git`.
pub async fn git_diff(cwd: &str) -> GitDiff {
    use crate::git::output;
    // Whole repository, paths from its root, as git status gives them; HEAD is missing before the first commit.
    let Some(root) = output(cwd, &["rev-parse", "--show-toplevel"]).await.map(|r| r.trim().to_string()).filter(|r| !r.is_empty())
    else {
        return GitDiff::default();
    };
    // Plain a/ b/ paths whatever the user's config (quotePath, mnemonicPrefix/noprefix).
    let opts = ["--no-color", "--no-ext-diff", "--src-prefix=a/", "--dst-prefix=b/"];
    let with = |head: bool| {
        let mut a = vec!["-c", "core.quotePath=false", "diff"];
        if head {
            a.push("HEAD");
        }
        a.extend(opts);
        a
    };
    let diff = match output(&root, &with(true)).await {
        Some(d) => d,
        None => output(&root, &with(false)).await.unwrap_or_default(),
    };
    let added = output(&root, &["ls-files", "--others", "--exclude-standard", "-z"]).await.unwrap_or_default();
    let mut files = pure::from_git(&diff);
    files.extend(added.split('\0').filter(|p| !p.is_empty()).map(|p| untracked(&root, p)));
    GitDiff { root: Some(root), files }
}

/// A new file: all of it added, unless it is binary, too large or a link (never followed: it could point at a device).
fn untracked(root: &str, rel: &str) -> FileDiff {
    let p = Path::new(root).join(rel);
    let text = std::fs::symlink_metadata(&p)
        .ok()
        .filter(|m| !m.file_type().is_symlink() && m.len() <= MAX_UNTRACKED)
        .and_then(|_| std::fs::read(&p).ok())
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .filter(|t| !t.contains('\0'));
    let lines = match text {
        Some(t) => pure::created(&t),
        None => vec![DiffLine { kind: DiffKind::Hunk, n: None, text: ccui_shared::t!("Diff.Binary") }],
    };
    FileDiff { path: rel.to_string(), lines }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn show(d: &[DiffLine]) -> String {
        d.iter()
            .map(|l| format!("{:?}{}:{}", l.kind, l.n.map(|n| n.to_string()).unwrap_or_default(), l.text))
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn from_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        let a = d.join("a.txt");
        std::fs::write(&a, "1\r\n2\r\n3\r\n4\r\n5\r\n6\r\n7\r\n8\r\n9\r\n10\r\n").unwrap();
        let f = a.to_string_lossy();
        let mid = from_input("Edit", &json!({"file_path": f, "old_string": "5", "new_string": "cinq"}));
        assert_eq!(show(&mid), "Hunk:@@ -3,5 +3,5 @@|Ctx3:3|Ctx4:4|Del5:5|Add5:cinq|Ctx6:6|Ctx7:7");
        let created = from_input("Write", &json!({"file_path": format!("{f}.absent"), "content": "x\ny\n"}));
        assert_eq!(show(&created), "Hunk:@@ -0,0 +1,2 @@|Add1:x|Add2:y");

        let big = d.join("big.txt");
        std::fs::write(&big, format!("a\n{}", "z".repeat(MAX_READ as usize))).unwrap();
        let b = big.to_string_lossy();
        assert_eq!(show(&from_input("Edit", &json!({"file_path": b, "old_string": "a", "new_string": "b"}))), "Del:a|Add:b");
        assert_eq!(show(&from_input("Write", &json!({"file_path": b, "content": "c"}))), "Add:c");
        #[cfg(unix)]
        {
            let lnk = d.join("big.lnk");
            std::os::unix::fs::symlink(&big, &lnk).unwrap();
            assert_eq!(show(&from_input("Edit", &json!({"file_path": lnk, "old_string": "a", "new_string": "b"}))), "Del:a|Add:b");
            let zero = d.join("zero.lnk");
            std::os::unix::fs::symlink("/dev/zero", &zero).unwrap();
            assert_eq!(show(&from_input("Edit", &json!({"file_path": zero, "old_string": "a", "new_string": "b"}))), "Del:a|Add:b");
        }
        let gone = from_input("Edit", &json!({"file_path": "C:\\nope\\x.cs", "old_string": "a", "new_string": "b"}));
        assert_eq!(show(&gone), "Del:a|Add:b");

        let h = d.join("h.txt");
        std::fs::write(&h, "hello\n").unwrap();
        let input = json!({"file_path": h, "old_string": "hello", "new_string": "bye"});
        assert_eq!(show(&from_input("Edit", &input)), "Hunk:@@ -1,1 +1,1 @@|Del1:hello|Add1:bye");
        std::fs::write(&h, "bye\n").unwrap(); // replay after the edit was applied
        assert_eq!(show(&from_input("Edit", &input)), "Hunk:@@ -1,1 +1,1 @@|Del1:hello|Add1:bye");
        assert_eq!(read(""), None);
        assert_eq!(read(&d.to_string_lossy()), None);
    }

    #[tokio::test]
    async fn diff_panel() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        assert_eq!(git_diff(&d.to_string_lossy()).await, GitDiff::default());
        crate::git::tests::g(d, &["init", "-q"]);
        std::fs::write(d.join("t.txt"), "a\n").unwrap();
        crate::git::tests::g(d, &["add", "t.txt"]);
        crate::git::tests::g(d, &["commit", "-q", "-m", "i"]);
        std::fs::write(d.join("t.txt"), "b\n").unwrap();
        std::fs::write(d.join("new.md"), "hi\n").unwrap();
        std::fs::write(d.join("bin.dat"), [0u8, 1, 2]).unwrap();
        let gd = git_diff(&d.to_string_lossy()).await;
        assert!(gd.root.is_some());
        let by = |p: &str| gd.files.iter().find(|f| f.path == p).map(|f| show(&f.lines)).unwrap_or_default();
        assert_eq!(by("t.txt"), "Hunk:@@ -1 +1 @@|Del1:a|Add1:b");
        assert_eq!(by("new.md"), "Hunk:@@ -0,0 +1,1 @@|Add1:hi");
        assert!(by("bin.dat").starts_with("Hunk:") && !by("bin.dat").contains('|'));
    }
}
