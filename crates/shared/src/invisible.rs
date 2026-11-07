//! Port of `Core/Invisible.cs` (strip hidden characters from pasted prompts) — spec 05 §8. Owner: shared-pure-b.

/// U+200B, U+2060–2064, U+FEFF, U+202A–202E, U+2066–2069, U+E0000–E007F. ZWJ/ZWNJ are kept.
pub fn hidden(c: char) -> bool {
    matches!(c, '\u{200B}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{E0000}'..='\u{E007F}')
}

/// `(cleaned, removed)`; a subdivision flag (U+1F3F4 + tag letters + U+E007F) is kept whole. Unchanged input when
/// nothing was removed.
pub fn strip(text: &str) -> (String, usize) {
    let r: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut n = 0;
    let mut i = 0;
    while i < r.len() {
        // England, Scotland, Wales: U+1F3F4, tag letters, then the cancel tag U+E007F.
        if r[i] == '\u{1F3F4}' {
            let mut j = i + 1;
            while j < r.len() && ('\u{E0020}'..='\u{E007E}').contains(&r[j]) {
                j += 1;
            }
            if j > i + 1 && j < r.len() && r[j] == '\u{E007F}' {
                out.extend(&r[i..=j]);
                i = j + 1;
                continue;
            }
        }
        if hidden(r[i]) {
            n += 1;
        } else {
            out.push(r[i]);
        }
        i += 1;
    }
    if n == 0 {
        (text.to_string(), 0)
    } else {
        (out, n)
    }
}

#[cfg(test)]
mod tests {
    use super::strip;

    #[test]
    fn invisible_check() {
        let s = |t: &str| strip(t);
        assert_eq!(s("plain text, é, 👨\u{200D}👩\u{200D}👧"), ("plain text, é, 👨\u{200D}👩\u{200D}👧".into(), 0));
        assert_eq!(s("ok\u{200B}\u{FEFF}"), ("ok".into(), 2));
        assert_eq!(s("a\u{202E}b\u{2066}c\u{2069}"), ("abc".into(), 3));
        assert_eq!(s("hi\u{E0049}\u{E0067}\u{E007F}"), ("hi".into(), 3));
        let england = "\u{1F3F4}\u{E0067}\u{E0062}\u{E0065}\u{E006E}\u{E0067}\u{E007F}";
        assert_eq!(s(&format!("go {england}")), (format!("go {england}"), 0));
        assert_eq!(s("\u{1F3F4}\u{E0067}x"), ("\u{1F3F4}x".into(), 1));
    }
}
