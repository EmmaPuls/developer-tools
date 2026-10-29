use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;

/// Claude config dir: `$CLAUDE_CONFIG_DIR` if set, else `~/.claude`.
pub fn claude_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    dirs::home_dir()
        .map(|h| h.join(".claude"))
        .context("Could not determine home directory")
}

pub fn settings_path() -> Result<PathBuf> {
    Ok(claude_dir()?.join("settings.json"))
}

/// Read settings.json. Missing file yields an empty object; malformed JSON is an error
/// so we never overwrite a file we couldn't parse.
pub fn read_settings() -> Result<Value> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(json!({}));
    }
    let text = fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))?;
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    let value: Value = serde_json::from_str(&text).with_context(|| {
        format!(
            "{} is not valid JSON; refusing to modify it",
            path.display()
        )
    })?;
    if !value.is_object() {
        bail!(
            "{} is not a JSON object; refusing to modify it",
            path.display()
        );
    }
    Ok(value)
}

/// Write settings.json atomically (temp file + rename).
pub fn write_settings(value: &Value) -> Result<()> {
    let path = settings_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("json.dt-tmp");
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    fs::write(&tmp, text).with_context(|| format!("Writing {}", tmp.display()))?;
    fs::rename(&tmp, &path).with_context(|| format!("Replacing {}", path.display()))?;
    Ok(())
}

/// The current `skillOverrides` map (name -> state).
pub fn skill_overrides(settings: &Value) -> Map<String, Value> {
    settings
        .get("skillOverrides")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

/// Set a skill's override in-place. `None` removes the key (absent = on),
/// and an emptied `skillOverrides` map is removed entirely.
pub fn set_override(settings: &mut Value, name: &str, state: Option<&str>) {
    let Some(root) = settings.as_object_mut() else {
        return;
    };
    match state {
        Some(s) => {
            let entry = root
                .entry("skillOverrides")
                .or_insert_with(|| Value::Object(Map::new()));
            if let Some(map) = entry.as_object_mut() {
                map.insert(name.to_string(), Value::String(s.to_string()));
            }
        }
        None => {
            if let Some(map) = root
                .get_mut("skillOverrides")
                .and_then(Value::as_object_mut)
            {
                map.remove(name);
                if map.is_empty() {
                    root.remove("skillOverrides");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_remove_preserves_other_keys() {
        let mut s = json!({"model": "opus", "enabledPlugins": {"a": true}});
        set_override(&mut s, "foo", Some("off"));
        assert_eq!(s["skillOverrides"]["foo"], "off");
        assert_eq!(s["model"], "opus");
        set_override(&mut s, "foo", None);
        assert!(s.get("skillOverrides").is_none());
        assert_eq!(s["enabledPlugins"]["a"], true);
    }

    #[test]
    fn remove_keeps_other_overrides() {
        let mut s = json!({"skillOverrides": {"a": "off", "b": "off"}});
        set_override(&mut s, "a", None);
        assert_eq!(skill_overrides(&s).len(), 1);
        assert_eq!(s["skillOverrides"]["b"], "off");
    }
}
