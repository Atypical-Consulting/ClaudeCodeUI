//! Port of `Core/FolderPicker.cs` semantics — spec 04 §2. Owner: core-worktrees-files.
//! The dialog itself is `tauri-plugin-dialog` and "Show in folder" is `tauri-plugin-opener::reveal_item_in_dir`
//! (desktop commands `pick_folder`, `reveal_path`); this module keeps the rules around them.
//! Never open a file Claude sent: reveal only.

use std::path::Path;

/// Start folder: `start` when non-empty, absolute and an existing directory; else home.
pub fn pick_start(start: Option<&str>) -> String {
    match start {
        Some(s) if !s.is_empty() && Path::new(s).is_absolute() && Path::new(s).is_dir() => s.to_string(),
        _ => dirs::home_dir().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default(),
    }
}

/// A picked folder without its trailing separator ("/" stays "/").
pub fn trim_picked(path: &str) -> String {
    let p = path.trim_matches(['\r', '\n']);
    let is_root = Path::new(p).has_root() && Path::new(p).parent().is_none();
    match p.strip_suffix(['/', '\\']) {
        Some(t) if !is_root && !t.is_empty() => t.to_string(),
        _ => p.to_string(),
    }
}

/// Err(`File.Gone(path)`) when the path is neither an existing file nor directory.
pub fn reveal_check(path: &str) -> Result<(), String> {
    if Path::new(path).exists() {
        Ok(())
    } else {
        Err(ccui_shared::t!("File.Gone", path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules() {
        assert_eq!(trim_picked("/Users/a/repo/\n"), "/Users/a/repo");
        assert_eq!(trim_picked("/\n"), "/");
        assert_eq!(trim_picked("/Users/a"), "/Users/a");
        let tmp = tempfile::tempdir().unwrap();
        let t = tmp.path().to_string_lossy().into_owned();
        assert_eq!(pick_start(Some(&t)), t);
        let home = dirs::home_dir().unwrap().to_string_lossy().into_owned();
        assert_eq!(pick_start(Some("relative/x")), home);
        assert_eq!(pick_start(Some(&format!("{t}/nope"))), home);
        assert_eq!(pick_start(None), home);
        assert!(reveal_check(&t).is_ok());
        assert!(reveal_check(&format!("{t}/nope")).unwrap_err().contains("nope"));
    }
}
