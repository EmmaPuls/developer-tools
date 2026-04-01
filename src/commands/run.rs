use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use dialoguer::FuzzySelect;
use serde::Deserialize;

use crate::commands::add;

// ── Data types ────────────────────────────────────────────────────────────────

/// One entry in the scripts directory.
pub struct ScriptEntry {
    pub filename: String,
    pub description: Option<String>,
}

impl ScriptEntry {
    /// Label shown in the FuzzySelect list: "filename  —  description"
    pub fn display_label(&self) -> String {
        match &self.description {
            Some(d) => format!("{}  —  {}", self.filename, d),
            None => self.filename.clone(),
        }
    }
}

/// Sidecar JSON shape (mirrors ScriptMeta in add.rs).
#[derive(Deserialize)]
struct ScriptMeta {
    description: String,
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Discovers all scripts in `dir`, reading optional sidecar `.meta.json` files
/// for descriptions. Entries are sorted by filename for a deterministic UI order.
pub fn discover_scripts(dir: &Path) -> Result<Vec<ScriptEntry>> {
    let mut entries: Vec<ScriptEntry> = Vec::new();

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };

    for entry in read_dir {
        let entry = entry?;
        let filename = entry.file_name().to_string_lossy().to_string();

        // Skip sidecar meta files — they are not scripts.
        if filename.ends_with(".meta.json") {
            continue;
        }

        // Attempt to read the sidecar .meta.json for this script.
        let meta_path = dir.join(format!("{}.meta.json", filename));
        let description = if meta_path.exists() {
            match std::fs::read_to_string(&meta_path) {
                Ok(content) => serde_json::from_str::<ScriptMeta>(&content)
                    .ok()
                    .map(|m| m.description),
                Err(_) => None,
            }
        } else {
            None
        };

        entries.push(ScriptEntry {
            filename,
            description,
        });
    }

    // Sort for deterministic order in the interactive list.
    entries.sort_by(|a, b| a.filename.cmp(&b.filename));

    Ok(entries)
}

/// Returns the `PathBuf` for `name` inside `dir`, or an error if it does not exist.
pub fn resolve_script(dir: &Path, name: &str) -> Result<PathBuf> {
    let path = dir.join(name);
    if path.exists() {
        Ok(path)
    } else {
        Err(anyhow!(
            "Script '{}' not found in {}",
            name,
            dir.display()
        ))
    }
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Main entry point for `dt run`.
pub fn execute(script_name: Option<String>) -> Result<()> {
    let scripts_dir = add::resolve_scripts_dir()?;

    let script_path = if let Some(name) = script_name {
        // Named mode: resolve directly.
        resolve_script(&scripts_dir, &name)?
    } else {
        // Interactive mode: show fuzzy list.
        let entries = discover_scripts(&scripts_dir)?;

        if entries.is_empty() {
            return Err(anyhow!(
                "No scripts found in {}. Run `dt add` first.",
                scripts_dir.display()
            ));
        }

        let labels: Vec<String> = entries.iter().map(|e| e.display_label()).collect();

        let selection = FuzzySelect::new()
            .with_prompt("Select a script to run")
            .items(&labels)
            .interact()?;

        scripts_dir.join(&entries[selection].filename)
    };

    // Execute the script, inheriting stdio so it feels like a native command.
    let status = std::process::Command::new(&script_path).status()?;

    if !status.success() {
        return Err(anyhow!(
            "Script exited with non-zero status: {}",
            status
        ));
    }

    Ok(())
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_entry_display_with_description() {
        let entry = ScriptEntry {
            filename: "backup_notes.sh".to_string(),
            description: Some("Copies .md files to backup".to_string()),
        };
        let display = entry.display_label();
        assert!(display.contains("backup_notes.sh"));
        assert!(display.contains("Copies .md files to backup"));
    }

    #[test]
    fn test_script_entry_display_without_description() {
        let entry = ScriptEntry {
            filename: "backup_notes.sh".to_string(),
            description: None,
        };
        let display = entry.display_label();
        assert!(display.contains("backup_notes.sh"));
        // Must not panic or show "None"
        assert!(!display.contains("None"));
    }

    #[test]
    fn test_discover_scripts_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let entries = discover_scripts(dir.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_discover_scripts_finds_sh_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("foo.sh"), "#!/bin/bash\necho hi").unwrap();
        std::fs::write(dir.path().join("bar.sh"), "#!/bin/bash\necho bye").unwrap();
        // A non-script file that should be ignored
        std::fs::write(dir.path().join("foo.sh.meta.json"), r#"{"description":"d"}"#).unwrap();

        let entries = discover_scripts(dir.path()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.filename.as_str()).collect();
        assert!(names.contains(&"foo.sh"));
        assert!(names.contains(&"bar.sh"));
        assert_eq!(entries.len(), 2, "meta.json sidecars must not appear as script entries");
    }

    #[test]
    fn test_discover_scripts_reads_description() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("foo.sh"), "#!/bin/bash\necho hi").unwrap();
        std::fs::write(
            dir.path().join("foo.sh.meta.json"),
            r#"{"description":"Does something useful"}"#,
        )
        .unwrap();

        let entries = discover_scripts(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].description.as_deref(), Some("Does something useful"));
    }

    #[test]
    fn test_discover_scripts_missing_meta_gives_none() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("foo.sh"), "#!/bin/bash\necho hi").unwrap();
        // No sidecar .meta.json

        let entries = discover_scripts(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].description.is_none());
    }

    #[test]
    fn test_resolve_script_exact_match() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("foo.sh"), "#!/bin/bash\necho hi").unwrap();

        let path = resolve_script(dir.path(), "foo.sh").unwrap();
        assert_eq!(path.file_name().unwrap(), "foo.sh");
    }

    #[test]
    fn test_resolve_script_not_found() {
        let dir = tempfile::tempdir().unwrap();
        // No scripts in dir
        let result = resolve_script(dir.path(), "nonexistent.sh");
        assert!(result.is_err());
    }
}
