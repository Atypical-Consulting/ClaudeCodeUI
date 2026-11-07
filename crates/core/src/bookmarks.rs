//! Port of `Core/Bookmarks.cs` — spec 03 §4. Owner: core-store-sessions.
//! `<AppData>/bookmarks.json`, PascalCase `[{"Session","Uuid","Gist","At"}]` (read files the C# app wrote).
//! Cached once under a lock; missing/IO/bad JSON → []. Rewritten whole on toggle (ponytail: fine for hundreds).

use crate::CoreResult;
use ccui_shared::api::Bookmark;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// One entry of `bookmarks.json` as the C# app wrote it (`record Bookmark(string Session, string Uuid, string Gist,
/// DateTimeOffset At)`, System.Text.Json PascalCase, `At` with its offset). Converted to/from `api::Bookmark` (UTC).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct BookmarkFile {
    session: String,
    uuid: String,
    gist: String,
    at: chrono::DateTime<chrono::FixedOffset>,
}

/// (file the cache was read from, entries): "the context panel asks on every render".
static CACHE: Mutex<Option<(PathBuf, Vec<BookmarkFile>)>> = Mutex::new(None);

fn file() -> PathBuf {
    crate::paths::app_data().join("bookmarks.json")
}

fn to_api(b: &BookmarkFile) -> Bookmark {
    Bookmark { session: b.session.clone(), uuid: b.uuid.clone(), gist: b.gist.clone(), at: b.at.to_utc() }
}

/// Run `f` on the cached list (read once per file; unreadable → []).
fn with_cache<R>(f: impl FnOnce(&mut Vec<BookmarkFile>) -> R) -> R {
    let path = file();
    let mut cache = CACHE.lock();
    if cache.as_ref().is_none_or(|(p, _)| *p != path) {
        let list = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        *cache = Some((path, list));
    }
    f(&mut cache.as_mut().expect("filled above").1)
}

pub fn all() -> Vec<Bookmark> {
    with_cache(|l| l.iter().map(to_api).collect())
}

pub fn of(session: &str) -> Vec<Bookmark> {
    with_cache(|l| l.iter().filter(|b| b.session == session).map(to_api).collect())
}

/// Remove matching, else add (now); write the file (errors propagate); returns "bookmarked afterwards".
/// The caller emits `Push::Bookmarks`.
pub fn toggle(session: &str, uuid: &str, gist: &str) -> CoreResult<bool> {
    let path = file();
    with_cache(|list| {
        let mut next = list.clone();
        next.retain(|b| !(b.session == session && b.uuid == uuid));
        let on = next.len() == list.len();
        if on {
            next.push(BookmarkFile {
                session: session.into(),
                uuid: uuid.into(),
                gist: gist.into(),
                at: chrono::Local::now().fixed_offset(),
            });
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, serde_json::to_string(&next).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        *list = next;
        Ok(on)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_and_read() {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(Some(dir.path()));
        assert!(all().is_empty(), "none without a file");
        assert!(toggle("s1", "u1", "first").unwrap() && toggle("s2", "u2", "other").unwrap());
        let s1 = of("s1");
        assert!(s1.len() == 1 && s1[0].uuid == "u1" && s1[0].gist == "first", "added, per session");
        assert!(!toggle("s1", "u1", "first").unwrap() && of("s1").is_empty());
        let rest = all();
        assert!(rest.len() == 1 && rest[0].session == "s2", "toggled off");
        let written = std::fs::read_to_string(file()).unwrap();
        assert!(written.starts_with(r#"[{"Session":"s2","Uuid":"u2","Gist":"other","At":""#), "{written}");

        // A file the C# app wrote (System.Text.Json: PascalCase, offset, non-ASCII escaped).
        std::fs::write(file(), r#"[{"Session":"s9","Uuid":"u9","Gist":"réponse","At":"2026-10-09T14:03:11.1234567+02:00"}]"#).unwrap();
        *CACHE.lock() = None;
        let b = all();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].gist, "réponse");
        assert_eq!(b[0].at.to_rfc3339(), "2026-10-09T12:03:11.123456700+00:00");

        std::fs::write(file(), "not json").unwrap();
        *CACHE.lock() = None;
        assert!(all().is_empty(), "unreadable file: none");
        crate::paths::set_test_root(None);
    }
}
