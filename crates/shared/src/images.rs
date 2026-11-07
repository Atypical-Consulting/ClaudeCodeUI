//! Port of `Core/Images.cs` (image and document blocks of user messages) — spec 04 §6. Owner: core-worktrees-files.
//! Pure: the UI reads the file in the browser and runs `from` in wasm; bytes cross IPC once in `session_send`.

use crate::json;
use crate::model::UserImage;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde_json::{json, Map, Value};

/// The UI's own cap per file (protects the UI's memory, not the API).
pub const MAX_BYTES: usize = 5 * 1024 * 1024;
pub const MAX_COUNT: usize = 10;

const PDF: &str = "application/pdf";
const TEXT: &str = "text/plain";

/// Image media type from magic bytes (png, jpeg, gif, webp), never from the browser's claim.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    match bytes {
        [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, ..] => Some("image/png"),
        [0xFF, 0xD8, 0xFF, ..] => Some("image/jpeg"),
        [0x47, 0x49, 0x46, 0x38, 0x37 | 0x39, 0x61, ..] => Some("image/gif"), // GIF87a, GIF89a
        [0x52, 0x49, 0x46, 0x46, _, _, _, _, 0x57, 0x45, 0x42, 0x50, ..] => Some("image/webp"), // RIFF….WEBP
        _ => None,
    }
}

/// Non-empty, no NUL, strict UTF-8.
pub fn is_text(bytes: &[u8]) -> bool {
    !bytes.is_empty() && !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

/// sniff, else pdf (`%PDF-`), else text/plain when is_text, else None (refused). name kept for pdf/text only.
pub fn from(bytes: &[u8], name: Option<&str>) -> Option<UserImage> {
    let kind = sniff(bytes).or_else(|| {
        if bytes.starts_with(b"%PDF-") {
            Some(PDF)
        } else if is_text(bytes) {
            Some(TEXT)
        } else {
            None
        }
    })?;
    Some(UserImage {
        media_type: kind.to_string(),
        url: Some(format!("data:{kind};base64,{}", B64.encode(bytes))),
        name: if kind == PDF || kind == TEXT { name.map(String::from) } else { None },
    })
}

/// `message.content` of the stdin user message: plain string without images, else blocks
/// (text block if non-empty, then image/document blocks; key order type, source{type,media_type,data}, title).
pub fn content(text: &str, images: &[UserImage]) -> Value {
    if images.is_empty() {
        return Value::String(text.to_string());
    }
    let mut blocks = Vec::new();
    if !text.is_empty() {
        blocks.push(json!({"type": "text", "text": text}));
    }
    for i in images {
        let source = if i.media_type == TEXT {
            let decoded = B64.decode(i.data()).ok().map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
            json!({"type": "text", "media_type": TEXT, "data": decoded})
        } else {
            json!({"type": "base64", "media_type": i.media_type, "data": i.data()})
        };
        let mut block = Map::new();
        block.insert("type".into(), Value::from(if i.document() { "document" } else { "image" }));
        block.insert("source".into(), source);
        if let (true, Some(n)) = (i.document(), &i.name) {
            block.insert("title".into(), Value::from(n.as_str()));
        }
        blocks.push(Value::Object(block));
    }
    Value::Array(blocks)
}

/// A transcript/stream `image` or `document` content block → UserImage (placeholder when not inline base64).
pub fn parse(block: &Value) -> UserImage {
    let empty = Value::Null;
    let src = json::prop(block, "source").unwrap_or(&empty);
    let kind = json::str(src, "media_type").unwrap_or("");
    let title = json::str(block, "title").map(String::from);
    let img = |media_type: &str, url: Option<String>, name: Option<String>| UserImage { media_type: media_type.into(), url, name };
    if json::str(block, "type") == Some("document") {
        return match (json::str(src, "type"), json::str(src, "data")) {
            (Some("base64"), Some(d)) if !d.is_empty() && kind == PDF => img(PDF, Some(format!("data:{PDF};base64,{d}")), title),
            (Some("text"), Some(t)) => img(TEXT, Some(format!("data:{TEXT};base64,{}", B64.encode(t.as_bytes()))), title),
            _ => img(if kind.is_empty() { PDF } else { kind }, None, title),
        };
    }
    match json::str(src, "data") {
        Some(data)
            if json::str(src, "type") == Some("base64")
                && matches!(kind, "image/png" | "image/jpeg" | "image/gif" | "image/webp")
                && !data.is_empty() =>
        {
            img(kind, Some(format!("data:{kind};base64,{data}")), None)
        }
        _ => img(kind, None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: [u8; 9] = [0x89, b'P', b'N', b'G', 13, 10, 26, 10, 1];

    #[test]
    fn sniff_and_from() {
        assert_eq!(sniff(&PNG), Some("image/png"));
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
        assert_eq!(sniff(b"GIF89a.."), Some("image/gif"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8L"), Some("image/webp"));
        assert_eq!(sniff(b"RIFF\0\0\0\0WAVE"), None);
        assert_eq!(sniff(b"<svg"), None);
        assert_eq!(sniff(&[]), None);
        let img = from(&PNG, None).unwrap();
        assert_eq!(img.media_type, "image/png");
        assert_eq!(img.url.as_deref(), Some("data:image/png;base64,iVBORw0KGgoB"));
        assert_eq!(img.data(), "iVBORw0KGgoB");
        let pdf = from(b"%PDF-1.4\n", Some("a.pdf")).unwrap();
        assert!(pdf.media_type == PDF && pdf.document() && pdf.name.as_deref() == Some("a.pdf"));
        let txt = from(b"The word is GIRAFFE88.", Some("notes.txt")).unwrap();
        assert!(txt.media_type == TEXT && txt.name.as_deref() == Some("notes.txt"));
        assert!(from(&[0x41, 0, 0x42], Some("x.bin")).is_none());
        assert!(from(&[0xC3, 0x28], Some("bad.txt")).is_none());
        let named = from(&PNG, Some("p.png")).unwrap();
        assert!(named.name.is_none() && !named.document());
    }

    #[test]
    fn content_blocks() {
        assert_eq!(content("hi", &[]), Value::String("hi".into()));
        let img = from(&PNG, None).unwrap();
        let c = content("what is this?", &[img.clone(), img.clone()]);
        assert_eq!(c[0], json!({"type": "text", "text": "what is this?"}));
        assert_eq!(c[1]["source"]["data"], "iVBORw0KGgoB");
        assert_eq!(content("", &[img.clone()]).as_array().unwrap().len(), 1);
        let pdf = from(b"%PDF-1.4\n", Some("a.pdf")).unwrap();
        let txt = from(b"The word is GIRAFFE88.", Some("notes.txt")).unwrap();
        let sent = content("both words?", &[pdf, txt.clone()]).to_string();
        assert!(sent.contains(r#""type":"document","source":{"type":"base64","media_type":"application/pdf""#), "{sent}");
        assert!(sent.contains(r#""title":"a.pdf""#));
        assert!(sent.contains(r#"{"type":"text","media_type":"text/plain","data":"The word is GIRAFFE88."}"#));
        // Read back through parse: same images.
        let blocks = content("both words?", &[txt.clone()]);
        let back = parse(&blocks[1]);
        assert_eq!((back.media_type.as_str(), back.name.as_deref(), back.data()), (TEXT, Some("notes.txt"), txt.data()));
        assert_eq!(parse(&c[1]), img);
    }

    #[test]
    fn parse_placeholders() {
        let v: Value = serde_json::from_str(
            r#"[{"type":"image","source":{"type":"url","url":"https://x/y.png"}},{"type":"image","source":{"type":"base64","media_type":"image/svg+xml","data":"PHN2Zz4="}},{"type":"image"},{"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}},{"type":"document","source":{"type":"url"},"title":"t"}]"#,
        )
        .unwrap();
        let p: Vec<UserImage> = v.as_array().unwrap().iter().map(parse).collect();
        assert!(p[..3].iter().all(|i| i.url.is_none()));
        assert_eq!(p[3].url.as_deref(), Some("data:image/png;base64,iVBORw0KGgo="));
        assert_eq!((p[4].media_type.as_str(), p[4].url.is_none(), p[4].name.as_deref()), (PDF, true, Some("t")));
    }
}
