//! Port of `Core/FolderPicker.cs` semantics — spec 04 §2. Owner: core-worktrees-files.
//! The dialog itself is `tauri-plugin-dialog` and "Show in folder" is `tauri-plugin-opener::reveal_item_in_dir`
//! (desktop commands `pick_folder`, `reveal_path`); this module keeps the rules around them.
//! Never open a file Claude sent: reveal only.

/// Start folder: `start` when non-empty, absolute and an existing directory; else home.
pub fn pick_start(start: Option<&str>) -> String {
    todo!("port: FolderPicker.Pick (start fallback)")
}

/// A picked folder without its trailing separator ("/" stays "/").
pub fn trim_picked(path: &str) -> String {
    todo!("port: FolderPicker.Parse")
}

/// Err(`File.Gone(path)`) when the path is neither an existing file nor directory.
pub fn reveal_check(path: &str) -> Result<(), String> {
    todo!("port: FolderPicker.Reveal (existence check)")
}
