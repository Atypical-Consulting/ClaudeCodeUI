//! Port of `Core/ApiErrors.cs` — spec 02 §13. Owner: shared-pure-a.
//! The reducer turns the CLI's synthetic "API Error: …" assistant text into an `ApiErrorItem`; title/hint are
//! resolved at display time in the UI language (store the kind, never the text).

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

impl ApiError {
    /// None unless `text.trim_start()` starts with "API Error"; first matching needle row wins (most specific first).
    /// All-digit needles match on word boundaries (`\b500\b`), others case-insensitively as substrings.
    pub fn parse(text: &str) -> Option<ApiError> {
        todo!("port: ApiErrors.Parse")
    }

    /// i18n `ApiError.{Kind}.Title` in the current language.
    pub fn title(&self) -> String {
        todo!("port: ApiError.Title")
    }

    /// i18n `ApiError.{Kind}.Hint` in the current language.
    pub fn hint(&self) -> String {
        todo!("port: ApiError.Hint")
    }
}
