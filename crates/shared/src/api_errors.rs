//! Port of `Core/ApiErrors.cs` — spec 02 §13. Owner: shared-pure-a.
//! The reducer turns the CLI's synthetic "API Error: …" assistant text into an `ApiErrorItem`; title/hint are
//! resolved at display time in the UI language (store the kind, never the text).

use crate::i18n;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ApiErrorKind {
    Auth,
    ContentFilter,
    Generic,
    Network,
    Overloaded,
    RateLimit,
    Server,
    TooLong,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ApiError {
    pub kind: ApiErrorKind,
    /// The CLI's text, untouched.
    pub raw: String,
}

/// First match wins, most specific first.
const TABLE: [(&[&str], ApiErrorKind); 7] = [
    (&["content filtering"], ApiErrorKind::ContentFilter),
    (&["prompt is too long", "413"], ApiErrorKind::TooLong),
    (&["rate_limit", "429"], ApiErrorKind::RateLimit),
    (&["overloaded", "529"], ApiErrorKind::Overloaded),
    (&["authentication", "invalid api key", "oauth", "401", "403"], ApiErrorKind::Auth),
    (&["timed out", "timeout", "connection error", "ECONNRESET", "fetch failed"], ApiErrorKind::Network),
    (&["500", "api_error", "internal server error"], ApiErrorKind::Server),
];

/// Numeric status codes match as whole tokens ("prompt used 1500 tokens" is not a 500); text needles stay
/// case-insensitive substrings.
fn hit(text: &str, lower: &str, n: &str) -> bool {
    if !n.bytes().all(|b| b.is_ascii_digit()) {
        return lower.contains(&n.to_lowercase());
    }
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
    text.match_indices(n).any(|(i, _)| !word(text[..i].chars().next_back()) && !word(text[i + n.len()..].chars().next()))
}

impl ApiError {
    /// None unless `text.trim_start()` starts with "API Error"; first matching needle row wins (most specific first).
    /// All-digit needles match on word boundaries (`\b500\b`), others case-insensitively as substrings.
    pub fn parse(text: &str) -> Option<ApiError> {
        if !text.trim_start().starts_with("API Error") {
            return None;
        }
        let lower = text.to_lowercase();
        let kind = TABLE
            .iter()
            .find(|(needles, _)| needles.iter().any(|n| hit(text, &lower, n)))
            .map_or(ApiErrorKind::Generic, |(_, k)| *k);
        Some(ApiError { kind, raw: text.to_string() })
    }

    /// i18n `ApiError.{Kind}.Title` in the current language.
    pub fn title(&self) -> String {
        i18n::t(&format!("ApiError.{:?}.Title", self.kind))
    }

    /// i18n `ApiError.{Kind}.Hint` in the current language.
    pub fn hint(&self) -> String {
        i18n::t(&format!("ApiError.{:?}.Hint", self.kind))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{set_thread_lang, Lang};

    #[test]
    fn check() {
        set_thread_lang(Some(Lang::Fr));
        let cf = "API Error: Output blocked by content filtering policy";
        let e = ApiError::parse(cf).unwrap();
        assert!(e.raw == cf && e.title().contains("filtre de contenu"), "content filter");
        assert_eq!(
            ApiError::parse(r#"API Error: 429 {"type":"error","error":{"type":"rate_limit_error"}}"#).unwrap().title(),
            "Limite de requêtes atteinte."
        );
        assert_eq!(ApiError::parse("API Error: something new").unwrap().title(), "L’API Claude a renvoyé une erreur.");
        assert!(ApiError::parse("Hello").is_none() && ApiError::parse("").is_none(), "non-errors");
        assert_eq!(ApiError::parse("  API Error: 529 overloaded").unwrap().title(), "L’API Claude est surchargée.");
        assert_eq!(ApiError::parse("API Error: prompt used 1500 tokens").unwrap().kind, ApiErrorKind::Generic);
        assert_eq!(ApiError::parse("API Error: 500 oops").unwrap().kind, ApiErrorKind::Server);
        assert_eq!(ApiError::parse("API Error: Connection ERROR").unwrap().kind, ApiErrorKind::Network);
        assert!(!ApiError::parse("API Error: 500").unwrap().hint().is_empty());
        set_thread_lang(None);
    }
}
