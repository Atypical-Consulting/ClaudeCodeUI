//! Port of `Core/UserSettings.cs` (default model of new sessions) — spec 03 §6. Owner: core-store-sessions.
//! `model` in `config_dir()/settings.json`, which `update_settings` refuses to write. Atomic: write
//! `settings.json.ccui-tmp`, rename over. A file that does not parse is left alone (Err says why).

use crate::CoreResult;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

pub fn file() -> PathBuf {
    crate::paths::config_dir().join("settings.json")
}

pub fn set_model(model: &str) -> CoreResult<()> {
    let file = file();
    let json = if file.exists() { fs::read_to_string(&file).map_err(|e| e.to_string())? } else { "{}".to_string() };
    let text = with_model(&json, model)?;
    if let Some(dir) = file.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = file.with_file_name("settings.json.ccui-tmp");
    fs::write(&tmp, text).map_err(|e| e.to_string())?;
    // Never a half-written settings file.
    fs::rename(&tmp, &file).map_err(|e| e.to_string())
}

/// Every other key kept (order too), "default" removes the key; indented. Not an object → Err.
pub fn with_model(json: &str, model: &str) -> CoreResult<String> {
    // A BOM (Notepad, PowerShell 5) is dropped like C# File.ReadAllText did; written back without one, like WriteAllText.
    let json = json.strip_prefix('\u{feff}').unwrap_or(json);
    let mut v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let o = v.as_object_mut().ok_or("settings.json is not an object")?;
    if model == "default" {
        o.shift_remove("model");
    } else {
        o.insert("model".into(), Value::from(model));
    }
    serde_json::to_string_pretty(&v).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_model_cases() {
        let set: Value = serde_json::from_str(
            &with_model(r#"{"effortLevel":"xhigh","model":"opus[1m]","modelSettings":{"claude-fable-5-1":{"effortLevel":"high"}}}"#, "sonnet").unwrap(),
        )
        .unwrap();
        assert_eq!(set["model"], "sonnet");
        assert_eq!(set["modelSettings"]["claude-fable-5-1"]["effortLevel"], "high");
        assert_eq!(set["effortLevel"], "xhigh");
        assert_eq!(set.as_object().unwrap().keys().collect::<Vec<_>>(), ["effortLevel", "model", "modelSettings"], "key order kept");
        let cleared: Value = serde_json::from_str(&with_model(r#"{"model":"opus"}"#, "default").unwrap()).unwrap();
        assert!(cleared.as_object().unwrap().is_empty());
        assert!(with_model("[1]", "x").is_err());
        assert_eq!(with_model("\u{feff}{\"model\":\"opus\"}", "sonnet").unwrap(), "{\n  \"model\": \"sonnet\"\n}", "BOM dropped");
    }

    #[test]
    fn set_model_writes_atomically() {
        let dir = tempfile::tempdir().unwrap();
        crate::paths::set_test_root(Some(dir.path()));
        set_model("opus").unwrap();
        assert_eq!(serde_json::from_str::<Value>(&fs::read_to_string(file()).unwrap()).unwrap()["model"], "opus");
        fs::write(file(), "not json").unwrap();
        assert!(set_model("sonnet").is_err());
        assert_eq!(fs::read_to_string(file()).unwrap(), "not json", "left alone");
        crate::paths::set_test_root(None);
    }
}
