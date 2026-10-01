use std::io::IsTerminal;
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

/// Discovers all scripts in `dir` using the per-script folder layout:
///
/// ```text
/// dir/
///   my_script/
///     my_script.sh    ← the script file
///     meta.json       ← optional description
///     resources/      ← optional script-specific assets
/// ```
///
/// Each subdirectory of `dir` is treated as one script entry. The script file
/// inside the subdirectory must share the stem of the folder name (with any
/// supported extension). `meta.json` inside the subfolder supplies the description.
/// Entries are sorted by folder name for a deterministic UI order.
pub fn discover_scripts(dir: &Path) -> Result<Vec<ScriptEntry>> {
    let mut entries: Vec<ScriptEntry> = Vec::new();

    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };

    for entry in read_dir {
        let entry = entry?;
        let meta = entry.metadata()?;

        // Only descend into subdirectories — ignore any loose files at the top level.
        if !meta.is_dir() {
            continue;
        }

        let folder_name = entry.file_name().to_string_lossy().to_string();

        // Look for the script file inside: <folder>/<folder>.sh or <folder>/<folder>.js
        let folder_path = entry.path();
        let script_filename = find_script_in_folder(&folder_path, &folder_name);

        // Skip directories that don't contain a recognisable script file.
        let filename = match script_filename {
            Some(f) => f,
            None => continue,
        };

        // Read meta.json from inside the subfolder for the description.
        let meta_path = folder_path.join("meta.json");
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

/// Looks inside `folder_path` for a script file whose stem matches `folder_name`.
/// Returns the bare filename (e.g. `"my_script.sh"`) if found.
fn find_script_in_folder(folder_path: &Path, folder_name: &str) -> Option<String> {
    for ext in &["sh", "js"] {
        let candidate = format!("{}.{}", folder_name, ext);
        if folder_path.join(&candidate).exists() {
            return Some(candidate);
        }
    }
    // Fallback: any file in the folder whose stem matches the folder name.
    if let Ok(rd) = std::fs::read_dir(folder_path) {
        for item in rd.flatten() {
            let name = item.file_name().to_string_lossy().to_string();
            let stem = std::path::Path::new(&name)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            if stem == folder_name && item.metadata().map(|m| m.is_file()).unwrap_or(false) {
                return Some(name);
            }
        }
    }
    None
}

/// Returns the `PathBuf` for the script file for `name` inside `dir`, or an
/// error if no matching script folder/file is found.
///
/// `name` may be either the folder name (stem) or the full filename including
/// extension (e.g. `"my_script"` or `"my_script.sh"`).
pub fn resolve_script(dir: &Path, name: &str) -> Result<PathBuf> {
    // Derive the stem: strip the extension if one was given.
    let stem = std::path::Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(name)
        .to_string();

    let folder_path = dir.join(&stem);

    if folder_path.is_dir() {
        // Try to locate the script file inside the folder.
        if let Some(filename) = find_script_in_folder(&folder_path, &stem) {
            let script_path = folder_path.join(filename);
            if script_path.exists() {
                return Ok(script_path);
            }
        }
    }

    Err(anyhow!(
        "Script '{}' not found in {}",
        name,
        dir.display()
    ))
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Main entry point for `dt run`.
///
/// When `verbose` is `true`, the environment variable `DT_VERBOSE=1` is set
/// for the child script process so that scripts can gate verbose output on it.
pub fn execute(script_name: Option<String>, verbose: bool) -> Result<()> {
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

        // Refuse to launch the interactive selector when stdin is not a TTY
        // (e.g. in tests or when piped). FuzzySelect would hang indefinitely.
        if !std::io::stdin().is_terminal() {
            return Err(anyhow!(
                "No script name given and stdin is not a TTY. \
                 Run `dt run <script_name>` to specify a script directly."
            ));
        }

        let labels: Vec<String> = entries.iter().map(|e| e.display_label()).collect();

        let selection = FuzzySelect::new()
            .with_prompt("Select a script to run")
            .items(&labels)
            .interact()?;

        // The filename is e.g. "my_script.sh"; the script lives at
        // scripts_dir/my_script/my_script.sh — use resolve_script to find it.
        resolve_script(&scripts_dir, &entries[selection].filename)?
    };

    // Execute the script, inheriting stdio so it feels like a native command.
    // When --verbose is passed, inject DT_VERBOSE=1 so the script can gate
    // verbose output on that variable.
    let mut cmd = std::process::Command::new(&script_path);
    if verbose {
        cmd.env("DT_VERBOSE", "1");
    }
    let status = cmd.status()?;

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

    // ── Helper: create the per-script folder layout ───────────────────────────

    /// Creates `dir/<stem>/<stem>.<ext>` and optionally `dir/<stem>/meta.json`,
    /// then returns the path to the script file.
    fn make_script_folder(dir: &std::path::Path, stem: &str, ext: &str, description: Option<&str>) -> std::path::PathBuf {
        let folder = dir.join(stem);
        std::fs::create_dir_all(&folder).unwrap();
        let script_path = folder.join(format!("{}.{}", stem, ext));
        std::fs::write(&script_path, format!("#!/bin/bash\necho {}", stem)).unwrap();
        if let Some(desc) = description {
            std::fs::write(
                folder.join("meta.json"),
                format!(r#"{{"description":"{}"}}"#, desc),
            )
            .unwrap();
        }
        script_path
    }

    // ── discover_scripts ──────────────────────────────────────────────────────

    #[test]
    fn test_discover_scripts_empty_dir() {
        let dir = tempfile::tempdir().unwrap();
        let entries = discover_scripts(dir.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_discover_scripts_finds_sh_files() {
        let dir = tempfile::tempdir().unwrap();
        make_script_folder(dir.path(), "foo", "sh", None);
        make_script_folder(dir.path(), "bar", "sh", None);
        // A loose file at the top level — must be ignored.
        std::fs::write(dir.path().join("stray.sh"), "#!/bin/bash").unwrap();

        let entries = discover_scripts(dir.path()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.filename.as_str()).collect();
        assert!(names.contains(&"foo.sh"), "foo.sh should be discovered");
        assert!(names.contains(&"bar.sh"), "bar.sh should be discovered");
        assert_eq!(entries.len(), 2, "only per-folder scripts must appear");
    }

    #[test]
    fn test_discover_scripts_reads_description() {
        let dir = tempfile::tempdir().unwrap();
        make_script_folder(dir.path(), "foo", "sh", Some("Does something useful"));

        let entries = discover_scripts(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].description.as_deref(), Some("Does something useful"));
    }

    #[test]
    fn test_discover_scripts_missing_meta_gives_none() {
        let dir = tempfile::tempdir().unwrap();
        make_script_folder(dir.path(), "foo", "sh", None);

        let entries = discover_scripts(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].description.is_none());
    }

    #[test]
    fn test_discover_scripts_skips_folder_without_script() {
        let dir = tempfile::tempdir().unwrap();
        // A folder that has no matching script file inside.
        std::fs::create_dir_all(dir.path().join("empty_folder")).unwrap();
        make_script_folder(dir.path(), "valid", "sh", None);

        let entries = discover_scripts(dir.path()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.filename.as_str()).collect();
        assert_eq!(entries.len(), 1, "empty_folder must be skipped");
        assert!(names.contains(&"valid.sh"));
    }

    #[test]
    fn test_discover_scripts_js_extension() {
        let dir = tempfile::tempdir().unwrap();
        make_script_folder(dir.path(), "myscript", "js", Some("A JS script"));

        let entries = discover_scripts(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].filename, "myscript.js");
        assert_eq!(entries[0].description.as_deref(), Some("A JS script"));
    }

    // ── resolve_script ────────────────────────────────────────────────────────

    #[test]
    fn test_resolve_script_by_stem() {
        let dir = tempfile::tempdir().unwrap();
        make_script_folder(dir.path(), "foo", "sh", None);

        // Resolve using just the stem.
        let path = resolve_script(dir.path(), "foo").unwrap();
        assert_eq!(path.file_name().unwrap(), "foo.sh");
    }

    #[test]
    fn test_resolve_script_by_full_filename() {
        let dir = tempfile::tempdir().unwrap();
        make_script_folder(dir.path(), "foo", "sh", None);

        // Resolve using the full filename (stem + extension).
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
