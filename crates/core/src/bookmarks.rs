//! Port of `Core/Bookmarks.cs` — spec 03 §4. Owner: core-store-sessions.
//! `<AppData>/bookmarks.json`, PascalCase `[{"Session","Uuid","Gist","At"}]` (read files the C# app wrote).
//! Cached once under a lock; missing/IO/bad JSON → []. Rewritten whole on toggle (ponytail: fine for hundreds).

use crate::CoreResult;
use ccui_shared::api::Bookmark;

pub fn all() -> Vec<Bookmark> {
    todo!("port: Bookmarks.All")
}

pub fn of(session: &str) -> Vec<Bookmark> {
    todo!("port: Bookmarks.Of")
}

/// Remove matching, else add (now); write the file (errors propagate); returns "bookmarked afterwards".
/// The caller emits `Push::Bookmarks`.
pub fn toggle(session: &str, uuid: &str, gist: &str) -> CoreResult<bool> {
    todo!("port: Bookmarks.Toggle")
}
