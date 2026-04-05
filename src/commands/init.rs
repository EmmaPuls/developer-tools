use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use dirs::home_dir;
use serde::{Deserialize, Serialize};

use crate::utils::ui;

// ── Embedded default meta-prompt files ───────────────────────────────────────
// These are compiled into the binary so `dt` always has sensible defaults even
// before `dt init` has been run.  They are seeded to disk on `dt init` (and
// lazily on first `dt add`) so users can edit them freely.

pub const META_PROMPT_PREAMBLE:    &str = include_str!("../resources/meta_prompts/preamble.txt");
pub const META_PROMPT_LOGGING:     &str = include_str!("../resources/meta_prompts/logging.txt");
pub const META_PROMPT_NO_COLOUR:   &str = include_str!("../resources/meta_prompts/no_colour.txt");
pub const META_PROMPT_ASYNC_FIRST: &str = include_str!("../resources/meta_prompts/async_first.txt");
pub const META_PROMPT_FOOTER:      &str = include_str!("../resources/meta_prompts/footer.txt");

// ── DtConfig ──────────────────────────────────────────────────────────────────

/// Top-level `dt` application configuration.
///
/// Stored as JSON at `~/.dt/config` (or wherever the user's OS home dir is).
/// The `dt_home` field records the canonical root for all `dt` data — scripts,
/// resources, per-command config, etc.  All other commands resolve their paths
/// relative to `dt_home`.
///
/// `dt init` creates or updates this file interactively.
/// Individual command sub-configs (e.g. `add`) are stored as nested objects
/// inside the same file so users only need to manage one config location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DtConfig {
    /// Root directory for all `dt` data.  Defaults to `$HOME/.dt`.
    pub dt_home: PathBuf,
}

impl Default for DtConfig {
    fn default() -> Self {
        let home = home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            dt_home: home.join(".dt"),
        }
    }
}

impl DtConfig {
    /// Path to the global config file: `~/.dt/config`.
    ///
    /// We always look here regardless of what `dt_home` is set to, because
    /// the config file itself tells us where `dt_home` is.  This is the
    /// bootstrap path.
    pub fn config_file_path() -> Result<PathBuf> {
        let home = home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        Ok(home.join(".dt").join("config"))
    }

    /// Load from `~/.dt/config`.  Returns `DtConfig::default()` if the file
    /// does not exist or cannot be parsed.
    pub fn load() -> Self {
        let Ok(path) = Self::config_file_path() else {
            return Self::default();
        };
        let Ok(contents) = fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str::<Self>(&contents).unwrap_or_default()
    }

    /// Persist the config to disk, creating parent directories as needed.
    pub fn save(&self) -> Result<()> {
        let path = Self::config_file_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialise DtConfig")?;
        fs::write(&path, json)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        Ok(())
    }
}

// ── Meta-prompt seeding ───────────────────────────────────────────────────────

/// Mapping of (filename, embedded_content) for each meta-prompt part.
const META_PROMPT_FILES: &[(&str, &str)] = &[
    ("preamble.txt",    META_PROMPT_PREAMBLE),
    ("logging.txt",     META_PROMPT_LOGGING),
    ("no_colour.txt",   META_PROMPT_NO_COLOUR),
    ("async_first.txt", META_PROMPT_ASYNC_FIRST),
    ("footer.txt",      META_PROMPT_FOOTER),
];

/// Seeds `<dt_home>/meta_prompts/` with the embedded default `.txt` files.
///
/// Files that already exist on disk are **not** overwritten — if the user has
/// edited a file their changes are preserved.  Only missing files are created.
///
/// Returns the path to the `meta_prompts` directory.
pub fn seed_meta_prompts(dt_home: &PathBuf) -> Result<PathBuf> {
    let dir = dt_home.join("meta_prompts");
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create meta_prompts dir: {}", dir.display()))?;

    for (filename, content) in META_PROMPT_FILES {
        let dest = dir.join(filename);
        if !dest.exists() {
            fs::write(&dest, content)
                .with_context(|| format!("Failed to seed {}", dest.display()))?;
        }
    }

    Ok(dir)
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Main entry point for `dt init`.
///
/// Guides the user through setting (or confirming) the `dt` home directory and
/// writes the result to `~/.dt/config`.  On subsequent runs it shows the
/// current value and lets the user change it.
pub fn execute() -> Result<()> {
    let existing = DtConfig::load();

    println!();
    println!("  dt init — configure the dt application directory");
    println!();
    println!("  The dt home directory is where scripts, resources, and");
    println!("  configuration are stored.  You can change it at any time");
    println!("  by running `dt init` again.");
    println!();
    println!("  Current dt home: {}", existing.dt_home.display());
    println!();

    let new_home = prompt_dt_home(&existing.dt_home)?;

    let config = DtConfig { dt_home: new_home.clone() };
    config.save()?;

    // Ensure the dt_home directory itself exists.
    fs::create_dir_all(&new_home)
        .with_context(|| format!("Failed to create dt home: {}", new_home.display()))?;

    // Seed meta-prompt files (skips any the user has already edited).
    let meta_dir = seed_meta_prompts(&new_home)?;

    ui::success(&format!(
        "dt home set to: {}  (config saved to ~/.dt/config)",
        new_home.display()
    ));
    println!();
    println!("  Meta-prompt rules are in: {}", meta_dir.display());
    println!("  Edit the .txt files there to customise AI generation behaviour.");

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn prompt_dt_home(current: &PathBuf) -> Result<PathBuf> {
    use std::io::{self, Write};

    let default_str = current.display().to_string();
    print!("  dt home directory [{}]: ", default_str);
    io::stdout().flush().context("Failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(current.clone());
    }

    // Expand a leading `~` to the actual home directory.
    let expanded = if trimmed.starts_with('~') {
        let home = home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        let without_tilde = trimmed.trim_start_matches('~').trim_start_matches('/');
        if without_tilde.is_empty() {
            home
        } else {
            home.join(without_tilde)
        }
    } else {
        PathBuf::from(trimmed)
    };

    Ok(expanded)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dt_config_default_contains_dt() {
        let cfg = DtConfig::default();
        assert!(
            cfg.dt_home.to_string_lossy().contains(".dt"),
            "default dt_home should contain '.dt'"
        );
    }

    #[test]
    fn test_dt_config_roundtrip_json() {
        let cfg = DtConfig {
            dt_home: PathBuf::from("/tmp/my_dt_home"),
        };
        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let parsed: DtConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.dt_home, cfg.dt_home);
    }

    #[test]
    fn test_config_file_path_ends_with_config() {
        let path = DtConfig::config_file_path().unwrap();
        assert_eq!(path.file_name().unwrap(), "config");
        assert!(path.to_string_lossy().contains(".dt"));
    }
}
