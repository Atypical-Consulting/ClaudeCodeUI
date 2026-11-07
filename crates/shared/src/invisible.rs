//! Port of `Core/Invisible.cs` (strip hidden characters from pasted prompts) — spec 05 §8. Owner: shared-pure-b.

/// U+200B, U+2060–2064, U+FEFF, U+202A–202E, U+2066–2069, U+E0000–E007F. ZWJ/ZWNJ are kept.
pub fn hidden(c: char) -> bool {
    todo!("port: Invisible.Hidden")
}

/// `(cleaned, removed)`; a subdivision flag (U+1F3F4 + tag letters + U+E007F) is kept whole. Unchanged input when
/// nothing was removed.
pub fn strip(text: &str) -> (String, usize) {
    todo!("port: Invisible.Strip")
}
