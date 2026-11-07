//! Port of `Core/Images.cs` (image and document blocks of user messages) — spec 04 §6. Owner: core-worktrees-files.
//! Pure: the UI reads the file in the browser and runs `from` in wasm; bytes cross IPC once in `session_send`.

use crate::model::UserImage;
use serde_json::Value;

/// The UI's own cap per file (protects the UI's memory, not the API).
pub const MAX_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_COUNT: usize = 10;

/// Image media type from magic bytes (png, jpeg, gif, webp), never from the browser's claim.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    todo!("port: Images.Sniff")
}

/// Non-empty, no NUL, strict UTF-8.
pub fn is_text(bytes: &[u8]) -> bool {
    todo!("port: Images.IsText")
}

/// sniff, else pdf (`%PDF-`), else text/plain when is_text, else None (refused). name kept for pdf/text only.
pub fn from(bytes: &[u8], name: Option<&str>) -> Option<UserImage> {
    todo!("port: Images.From")
}

/// `message.content` of the stdin user message: plain string without images, else blocks
/// (text block if non-empty, then image/document blocks; key order type, source{type,media_type,data}, title).
pub fn content(text: &str, images: &[UserImage]) -> Value {
    todo!("port: Images.Content")
}

/// A transcript/stream `image` or `document` content block → UserImage (placeholder when not inline base64).
pub fn parse(block: &Value) -> UserImage {
    todo!("port: Images.Parse")
}
