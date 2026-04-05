use std::{
    env,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::commands::init::{
    DtConfig,
    META_PROMPT_ASYNC_FIRST,
    META_PROMPT_FOOTER,
    META_PROMPT_LOGGING,
    META_PROMPT_NO_COLOUR,
    META_PROMPT_PREAMBLE,
};
use crate::utils::ai::{AcliRovodev, AiCallSpec, AiProvider, AiSession};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Default location where generated scripts are stored (relative to $HOME).
pub const DEFAULT_SCRIPTS_DIR: &str = ".dt/scripts";

// ── Prompt rule constants ─────────────────────────────────────────────────────
//
// These are aliases for the embedded file contents defined in `init.rs`.
// The canonical text lives in `src/resources/meta_prompts/*.txt` (compiled in
// via `include_str!`).  At runtime `assemble_prompt_rules` reads the user's
// on-disk copies from `<dt_home>/meta_prompts/`, falling back to these
// constants only when a file is absent.
//
// Tests that validate rule *content* should reference these constants directly.

/// Fixed preamble — always included, never toggled.
const RULE_PREAMBLE: &str    = META_PROMPT_PREAMBLE;
/// Logging helpers rule. Controlled by `AddConfig::rule_logging`.
pub const RULE_LOGGING: &str     = META_PROMPT_LOGGING;
/// No-colour accessibility rule. Controlled by `AddConfig::rule_no_colour`.
pub const RULE_NO_COLOUR: &str   = META_PROMPT_NO_COLOUR;
/// Async-first AI call rule. Controlled by `AddConfig::rule_async_first`.
pub const RULE_ASYNC_FIRST: &str = META_PROMPT_ASYNC_FIRST;
/// Fixed footer — always included, never toggled.
const RULE_FOOTER: &str      = META_PROMPT_FOOTER;

// ── AddConfig ─────────────────────────────────────────────────────────────────

/// Per-rule toggles that control which sections are assembled into the meta-prompt.
///
/// Loaded from `~/.dt/config` (JSON).  Any key absent from the file defaults to
/// `true` (the rule is enabled).  Users can disable a rule by setting it to
/// `false` without recompiling `dt`.
///
/// Example `~/.dt/config`:
/// ```json
/// {
///   "rule_logging":    true,
///   "rule_no_colour":  false,
///   "rule_async_first": true
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddConfig {
    /// Include the standard `log()` / `verbose_log()` helpers and usage rules.
    #[serde(default = "default_true")]
    pub rule_logging: bool,

    /// Forbid ANSI colour codes; require bold or plain text for emphasis.
    #[serde(default = "default_true")]
    pub rule_no_colour: bool,

    /// Enforce the async-first AI call pattern in generated scripts.
    #[serde(default = "default_true")]
    pub rule_async_first: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AddConfig {
    fn default() -> Self {
        Self {
            rule_logging: true,
            rule_no_colour: true,
            rule_async_first: true,
        }
    }
}

impl AddConfig {
    /// Load config from `<dt_home>/add_config.json`.
    ///
    /// The `dt_home` root is resolved via `DtConfig::load()` so it respects
    /// whatever directory the user set with `dt init` (default `~/.dt`).
    /// If the file does not exist or cannot be parsed, returns
    /// `AddConfig::default()` (all rules enabled).
    pub fn load() -> Self {
        let dt_home = DtConfig::load().dt_home;
        let config_path = dt_home.join("add_config.json");
        let Ok(contents) = fs::read_to_string(&config_path) else {
            return Self::default();
        };
        serde_json::from_str::<Self>(&contents).unwrap_or_default()
    }

    /// Persist the config to `<dt_home>/add_config.json`.
    pub fn save(&self) -> anyhow::Result<()> {
        let dt_home = DtConfig::load().dt_home;
        fs::create_dir_all(&dt_home)
            .with_context(|| format!("Failed to create dt home: {}", dt_home.display()))?;
        let config_path = dt_home.join("add_config.json");
        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialise AddConfig")?;
        fs::write(&config_path, json)
            .with_context(|| format!("Failed to write add config to {}", config_path.display()))?;
        Ok(())
    }

    /// Returns a list of all rules as `(key, description, current_value)` triples.
    /// Used by `dt add config` to display and edit rules.
    pub fn rules(&self) -> Vec<(&'static str, &'static str, bool)> {
        vec![
            (
                "rule_logging",
                "Include log()/verbose_log() helpers in generated scripts",
                self.rule_logging,
            ),
            (
                "rule_no_colour",
                "Forbid ANSI colour codes; use bold/plain text for emphasis (accessibility)",
                self.rule_no_colour,
            ),
            (
                "rule_async_first",
                "Enforce async-first AI call pattern in generated scripts",
                self.rule_async_first,
            ),
        ]
    }

    /// Apply a `key=value` pair to this config in-place.
    /// Returns an error if the key is unknown.
    pub fn set_rule(&mut self, key: &str, value: bool) -> anyhow::Result<()> {
        match key {
            "rule_logging" => self.rule_logging = value,
            "rule_no_colour" => self.rule_no_colour = value,
            "rule_async_first" => self.rule_async_first = value,
            other => anyhow::bail!("Unknown add config rule: '{}'", other),
        }
        Ok(())
    }
}

// ── dt add config subcommand ──────────────────────────────────────────────────

/// Entry point for `dt add config`.
///
/// Lists all configurable rules with their current on/off state and lets the
/// user toggle them interactively.  Changes are written back to
/// `<dt_home>/add_config.json`.
pub fn run_add_config() -> anyhow::Result<()> {
    let mut cfg = AddConfig::load();
    let dt_home = DtConfig::load().dt_home;
    let config_path = dt_home.join("add_config.json");
    let meta_prompts_dir = dt_home.join("meta_prompts");

    // Ensure prompt files exist so the user can find them immediately.
    crate::commands::init::seed_meta_prompts(&dt_home)
        .context("Failed to seed meta-prompt files")?;

    println!();
    println!("  dt add config — manage script generation rules");
    println!("  Config file:   {}", config_path.display());
    println!("  Prompt files:  {}", meta_prompts_dir.display());
    println!();
    println!("  Rules control which .txt prompt files are included when generating");
    println!("  scripts.  Edit the .txt files directly to change rule wording.");
    println!("  Toggle a rule on/off by entering its number.");
    println!();

    loop {
        // Print the current state of all rules.
        let rules = cfg.rules();
        for (i, (key, description, enabled)) in rules.iter().enumerate() {
            let status = if *enabled { "✓ ON " } else { "✗ OFF" };
            println!("  [{}]  {}  {}  ({})", i + 1, status, description, key);
        }
        println!();
        println!("  Enter a rule number to toggle it, or press Enter to save and quit:");
        print!("  > ");

        use std::io::{self, Write};
        io::stdout().flush().context("Failed to flush stdout")?;

        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .context("Failed to read input")?;

        let trimmed = input.trim();
        if trimmed.is_empty() {
            break;
        }

        match trimmed.parse::<usize>() {
            Ok(n) if n >= 1 && n <= rules.len() => {
                let (key, _, current) = rules[n - 1];
                cfg.set_rule(key, !current)?;
                println!();
            }
            _ => {
                println!("  Please enter a number between 1 and {}.", rules.len());
                println!();
            }
        }
    }

    cfg.save()?;
    crate::utils::ui::success(&format!(
        "add config saved to {}",
        config_path.display()
    ));
    Ok(())
}

/// Reads a meta-prompt rule file from `<dt_home>/meta_prompts/<filename>`.
/// Falls back to the provided `fallback` string (the embedded Rust constant)
/// if the file does not exist or cannot be read.
fn read_rule_file(dt_home: &std::path::Path, filename: &str, fallback: &str) -> String {
    let path = dt_home.join("meta_prompts").join(filename);
    fs::read_to_string(&path).unwrap_or_else(|_| fallback.to_string())
}

/// Assembles the raw (un-substituted) meta-prompt from the enabled rules in `config`.
///
/// Each rule section is read from `<dt_home>/meta_prompts/<name>.txt` so users
/// can edit the files directly to customise generation behaviour without
/// recompiling `dt`.  If a file is missing the embedded default is used as a
/// fallback.
///
/// Placeholders `{LANG}`, `{PROMPT}`, and `{SCRIPT_STEM}` are left in place for
/// `build_add_prompt` to fill in.
pub fn assemble_prompt_rules(config: &AddConfig) -> String {
    let dt_home = DtConfig::load().dt_home;

    let preamble   = read_rule_file(&dt_home, "preamble.txt",    RULE_PREAMBLE);
    let logging    = read_rule_file(&dt_home, "logging.txt",     RULE_LOGGING);
    let no_colour  = read_rule_file(&dt_home, "no_colour.txt",   RULE_NO_COLOUR);
    let async_first = read_rule_file(&dt_home, "async_first.txt", RULE_ASYNC_FIRST);
    let footer     = read_rule_file(&dt_home, "footer.txt",      RULE_FOOTER);

    let mut parts: Vec<String> = vec![preamble];

    if config.rule_logging {
        parts.push(logging);
    }
    if config.rule_no_colour {
        parts.push(no_colour);
    }
    if config.rule_async_first {
        parts.push(async_first);
    }

    parts.push(footer);
    parts.join("\n")
}

/// The default (all-rules-enabled) meta-prompt template, assembled from the
/// embedded `.txt` files via `concat!` + `include_str!`.
///
/// This is a static string compiled into the binary — it reflects the default
/// file content and does **not** pick up on-disk user edits.  At runtime,
/// `assemble_prompt_rules(&AddConfig::default())` is used instead, which reads
/// `<dt_home>/meta_prompts/*.txt` and falls back to this constant only when a
/// file is absent.
///
/// Kept as a `pub const` so tests can validate default rule content without
/// touching the filesystem.
pub const ADD_META_PROMPT: &str = concat!(
    include_str!("../resources/meta_prompts/preamble.txt"),
    "\n",
    include_str!("../resources/meta_prompts/logging.txt"),
    "\n",
    include_str!("../resources/meta_prompts/no_colour.txt"),
    "\n",
    include_str!("../resources/meta_prompts/async_first.txt"),
    "\n",
    include_str!("../resources/meta_prompts/footer.txt"),
);

/// Editor template pre-populated into the temp file when `dt add` is run without `-m`.
const EDITOR_TEMPLATE: &str = r#"# dt add — describe the script you want to generate
#
# Write your requirements below this comment block.
# Lines starting with '#' will be ignored.
# Save and close the editor when done.
#
# Example:
#   Copy all .md files from ~/notes to ~/backup and summarise their contents.
#

"#;

// ── Public types ──────────────────────────────────────────────────────────────

/// Target scripting language for generated scripts.
#[derive(Debug, Clone, PartialEq)]
pub enum Lang {
    Bash,
    Js,
}

impl Lang {
    /// Parse from a CLI string (case-insensitive).
    pub fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "bash" | "sh" => Ok(Lang::Bash),
            "js" | "javascript" => Ok(Lang::Js),
            other => Err(anyhow!("Unknown language '{}'. Supported: bash, js", other)),
        }
    }

    /// Return the display name used in the meta-prompt.
    pub fn display_name(&self) -> &'static str {
        match self {
            Lang::Bash => "Bash",
            Lang::Js => "JavaScript",
        }
    }
}

/// Structured JSON output expected from the AI.
#[derive(Deserialize)]
pub struct AddOutput {
    pub filename: Option<String>,
    pub description: Option<String>,
    pub script: Option<String>,
    pub output_schema: Option<serde_json::Value>,
}

/// Sidecar metadata written alongside the saved script.
#[derive(Serialize)]
pub struct ScriptMeta {
    pub description: String,
    pub original_prompt: String,
}

// ── Helper functions ──────────────────────────────────────────────────────────

/// Removes lines that start with `#` (after trimming leading whitespace).
/// Used to clean editor-session content before sending to the AI.
fn strip_comment_lines(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Substitutes `{LANG}`, `{PROMPT}`, and `{SCRIPT_STEM}` into the assembled meta-prompt.
///
/// The prompt is assembled from the rules enabled in `config` via
/// `assemble_prompt_rules`.  Pass `&AddConfig::default()` to get the
/// all-rules-enabled prompt (equivalent to the old `ADD_META_PROMPT` constant).
pub fn build_add_prompt(
    user_requirements: &str,
    lang: &Lang,
    script_stem: &str,
    config: &AddConfig,
) -> String {
    assemble_prompt_rules(config)
        .replace("{LANG}", lang.display_name())
        .replace("{PROMPT}", user_requirements)
        .replace("{SCRIPT_STEM}", script_stem)
}

/// Resolves `<dt_home>/scripts`, creating the directory if needed.
///
/// The `dt_home` root is resolved via `DtConfig::load()`, so it respects
/// whatever the user configured with `dt init` (default `~/.dt`).
pub fn resolve_scripts_dir() -> Result<PathBuf> {
    let dir = DtConfig::load().dt_home.join("scripts");
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create scripts directory: {}", dir.display()))?;
    Ok(dir)
}

/// Opens a temporary `.md` file in `$VISUAL` / `$EDITOR` / `git config core.editor`,
/// waits for the user to save and close, then returns the cleaned prompt text.
fn open_editor_for_prompt() -> Result<String> {
    // Create a named temp file with .md extension
    let mut tmp = tempfile::Builder::new()
        .prefix("dt-add-")
        .suffix(".md")
        .tempfile()
        .context("Failed to create temporary editor file")?;

    // Pre-populate with the editor template
    tmp.write_all(EDITOR_TEMPLATE.as_bytes())
        .context("Failed to write editor template")?;
    tmp.flush().context("Failed to flush editor template")?;

    let tmp_path = tmp.path().to_path_buf();

    // Resolve editor binary.
    // Priority: $VISUAL > $EDITOR > $GIT_EDITOR > git config core.editor
    let editor = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .or_else(|_| env::var("GIT_EDITOR"))
        .or_else(|_| {
            // Try git config core.editor
            let output = Command::new("git")
                .args(["config", "core.editor"])
                .output();
            match output {
                Ok(o) if o.status.success() => {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() {
                        Err(env::VarError::NotPresent)
                    } else {
                        Ok(s)
                    }
                }
                _ => Err(env::VarError::NotPresent),
            }
        })
        .map_err(|_| {
            anyhow!(
                "No editor found. Set $VISUAL or $EDITOR, or configure git config core.editor."
            )
        })?;

    // Spawn editor blocking.
    // The editor string may contain arguments (e.g. "code --wait"), so split on
    // whitespace and pass the tail as extra arguments before the file path.
    let mut parts = editor.split_whitespace();
    let bin = parts.next().ok_or_else(|| anyhow!("Editor command is empty"))?;
    Command::new(bin)
        .args(parts)
        .arg(&tmp_path)
        .status()
        .with_context(|| format!("Failed to launch editor '{}'", editor))?;

    // Read back, strip comments, trim
    let raw = fs::read_to_string(&tmp_path).context("Failed to read editor file")?;
    let cleaned = strip_comment_lines(&raw).trim().to_string();

    if cleaned.is_empty() {
        return Err(anyhow!(
            "No prompt provided — editor was empty or contained only comments."
        ));
    }

    Ok(cleaned)
}

/// Sets the file at `path` to be executable (Unix only; no-op elsewhere).
#[cfg(unix)]
fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Main entry point for `dt add`.
pub fn execute(message: Option<String>, lang: Lang) -> Result<()> {
    // 1. Capture prompt
    let user_requirements = match message {
        Some(m) => m,
        None => open_editor_for_prompt()?,
    };

    // 2. Phase A — ask the AI for just the script stem so we can inject
    //    concrete paths into the main prompt before generating the script.
    //    We send a lightweight prompt; the AI returns { "stem": "..." }.
    #[derive(serde::Deserialize)]
    struct StemOutput {
        stem: Option<String>,
    }

    let stem_prompt = format!(
        "Given these script requirements, respond with ONLY valid JSON containing a single \
         field \"stem\" whose value is a short snake_case identifier (no extension) that \
         would make a good filename stem for this script. No markdown, no prose.\n\n\
         Requirements:\n{}",
        user_requirements
    );

    let provider = AcliRovodev;
    let dt_home = DtConfig::load().dt_home;

    // Lazily seed meta-prompt files so users can edit them even if they never
    // ran `dt init`.  Existing user-edited files are not overwritten.
    crate::commands::init::seed_meta_prompts(&dt_home)
        .context("Failed to seed meta-prompt files")?;

    let resources_dir = dt_home.join("resources");

    // Use the generic acli_schema.json (just { "stem": string }) for Phase A.
    // We reuse setup_resources_with_schema with a minimal inline schema.
    let stem_schema = br#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "stem": { "type": "string" }
  },
  "required": ["stem"]
}"#;
    let stem_paths = provider.setup_resources_with_schema(&resources_dir, stem_schema)?;
    let stem_spec = AiCallSpec { prompt: stem_prompt };
    let mut stem_session = AiSession::new(stem_paths.output_file.clone());

    provider
        .build_command(&stem_spec, &stem_paths)
        .status()
        .context("Failed to execute acli rovodev (phase A — stem)")?;

    let stem_output = provider
        .read_output::<StemOutput>(&mut stem_session)?
        .ok_or_else(|| anyhow!("AI returned no output for stem query"))?;

    let script_stem = stem_output
        .stem
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow!("AI response missing 'stem'"))?;

    drop(stem_session);

    // 3. Phase B — build the full meta-prompt with concrete paths injected,
    //    then ask the AI to generate the complete script + output_schema.
    let add_config = AddConfig::load();
    let prompt = build_add_prompt(&user_requirements, &lang, &script_stem, &add_config);

    let paths = provider
        .setup_resources_with_schema(&resources_dir, AcliRovodev::add_schema_bytes())?;

    let spec = AiCallSpec { prompt };
    let mut session = AiSession::new(paths.output_file.clone());

    provider
        .build_command(&spec, &paths)
        .status()
        .context("Failed to execute acli rovodev (phase B — script)")?;

    // 4. Parse output
    let output = provider
        .read_output::<AddOutput>(&mut session)?
        .ok_or_else(|| anyhow!("AI returned no output or unparseable JSON"))?;

    let filename = output
        .filename
        .ok_or_else(|| anyhow!("AI response missing 'filename'"))?;
    let description = output
        .description
        .unwrap_or_else(|| "No description provided.".to_string());
    let script = output
        .script
        .ok_or_else(|| anyhow!("AI response missing 'script'"))?;

    // 5. Save script into its own subdirectory
    //    ~/.dt/scripts/<stem>/
    //      <filename>        ← the script itself
    //      meta.json         ← description / metadata
    //      resources/        ← script-specific config / schemas
    //      output/           ← empty dir for acli rovodev tmp output files
    let scripts_dir = resolve_scripts_dir()?;

    // Use the stem we got from Phase A (consistent with what was injected into the prompt).
    // Fall back to deriving from the filename in case the AI changed it.
    let stem_from_filename = Path::new(&filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename.as_str())
        .to_string();
    let final_stem = if !script_stem.is_empty() {
        script_stem.clone()
    } else {
        stem_from_filename
    };

    let script_folder = scripts_dir.join(&final_stem);
    fs::create_dir_all(&script_folder)
        .with_context(|| format!("Failed to create script folder: {}", script_folder.display()))?;

    // Create resources/ subdirectory.
    let resources_folder = script_folder.join("resources");
    fs::create_dir_all(&resources_folder)
        .with_context(|| format!("Failed to create resources folder: {}", resources_folder.display()))?;

    // Create output/ subdirectory (acli rovodev writes tmp files here at runtime).
    let output_folder = script_folder.join("output");
    fs::create_dir_all(&output_folder)
        .with_context(|| format!("Failed to create output folder: {}", output_folder.display()))?;

    // Write the task-specific schema to resources/acli_add_schema.json.
    // If the AI returned an output_schema, use it; otherwise fall back to the
    // generic single-value schema.
    let schema_dest = resources_folder.join("acli_add_schema.json");
    let task_schema_bytes = if let Some(ref schema_val) = output.output_schema {
        serde_json::to_vec_pretty(schema_val)
            .context("Failed to serialize AI-provided output_schema")?
    } else {
        include_bytes!("../resources/acli_schema.json").to_vec()
    };
    fs::write(&schema_dest, &task_schema_bytes)
        .with_context(|| format!("Failed to write schema to {}", schema_dest.display()))?;

    // Copy (or seed) config.ag-stack.yml into the per-script resources/ folder.
    // We source it from <dt_home>/resources/ so any user edits there are propagated;
    // fall back to the embedded template if it doesn't exist yet.
    let global_config = dt_home.join("resources").join("config.ag-stack.yml");
    let config_bytes: Vec<u8> = if global_config.exists() {
        fs::read(&global_config)
            .with_context(|| format!("Failed to read {}", global_config.display()))?
    } else {
        include_bytes!("../resources/config.ag-stack.yml").to_vec()
    };
    let config_dest = resources_folder.join("config.ag-stack.yml");
    fs::write(&config_dest, &config_bytes)
        .with_context(|| format!("Failed to write config to {}", config_dest.display()))?;

    let script_path = script_folder.join(&filename);
    fs::write(&script_path, &script)
        .with_context(|| format!("Failed to write script to {}", script_path.display()))?;
    set_executable(&script_path)?;

    // 6. Write meta.json inside the script folder.
    let meta_path = script_folder.join("meta.json");
    let meta = ScriptMeta {
        description: description.clone(),
        original_prompt: user_requirements.clone(),
    };
    fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)
        .with_context(|| format!("Failed to write metadata to {}", meta_path.display()))?;

    // 7. AiSession cleanup (output file already deleted by read_output; this is a no-op)
    drop(session);

    // 8. Print success
    println!("✓ Script saved to: {}", script_path.display());
    println!("  {}", description);

    Ok(())
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── strip_comment_lines ───────────────────────────────────────────────────

    #[test]
    fn test_strip_comment_lines() {
        let input = "# foo\nbar\n# baz\nqux";
        assert_eq!(strip_comment_lines(input), "bar\nqux");
    }

    #[test]
    fn test_strip_comment_lines_empty() {
        let input = "# comment one\n# comment two\n# comment three";
        assert_eq!(strip_comment_lines(input), "");
    }

    #[test]
    fn test_strip_comment_lines_preserves_blank_lines() {
        let input = "first\n\nsecond\n\nthird";
        assert_eq!(strip_comment_lines(input), "first\n\nsecond\n\nthird");
    }

    // ── DEFAULT_SCRIPTS_DIR constant ──────────────────────────────────────────

    #[test]
    fn test_scripts_dir_constant() {
        // DEFAULT_SCRIPTS_DIR must resolve to something containing ".dt/scripts"
        assert!(DEFAULT_SCRIPTS_DIR.contains(".dt/scripts"));
    }

    // ── ADD_META_PROMPT constant ──────────────────────────────────────────────

    #[test]
    fn test_meta_prompt_contains_json_instruction() {
        assert!(
            ADD_META_PROMPT.contains("filename"),
            "ADD_META_PROMPT must mention 'filename'"
        );
        assert!(
            ADD_META_PROMPT.contains("description"),
            "ADD_META_PROMPT must mention 'description'"
        );
        assert!(
            ADD_META_PROMPT.contains("script"),
            "ADD_META_PROMPT must mention 'script'"
        );
    }

    #[test]
    fn test_meta_prompt_contains_async_instruction() {
        let has_async_keyword = ADD_META_PROMPT.contains("background")
            || ADD_META_PROMPT.contains("async")
            || ADD_META_PROMPT.contains("backgrounded");
        assert!(
            has_async_keyword,
            "ADD_META_PROMPT must contain 'background', 'backgrounded', or 'async'"
        );
    }

    // ── Goal 3 unit tests ────────────────────────────────────────────────────

    #[test]
    fn test_meta_prompt_contains_json_fields() {
        // The AI must return structured JSON with exactly these three fields.
        assert!(ADD_META_PROMPT.contains("\"filename\"") || ADD_META_PROMPT.contains("filename"));
        assert!(ADD_META_PROMPT.contains("\"description\"") || ADD_META_PROMPT.contains("description"));
        assert!(ADD_META_PROMPT.contains("\"script\"") || ADD_META_PROMPT.contains("script"));
    }

    #[test]
    fn test_meta_prompt_enforces_background_calls() {
        // The prompt must instruct the AI to background AI calls (async-first pattern).
        let prompt_lower = ADD_META_PROMPT.to_lowercase();
        let has_background = prompt_lower.contains("background")
            || prompt_lower.contains("async")
            || prompt_lower.contains(" &")
            || prompt_lower.contains("backgrounded");
        assert!(has_background, "ADD_META_PROMPT must instruct the AI to emit backgrounded AI calls");
    }

    #[test]
    fn test_meta_prompt_enforces_pid_capture() {
        // The prompt must instruct the AI to capture the PID of each background call
        // so it can wait for it before consuming the result.
        let prompt_lower = ADD_META_PROMPT.to_lowercase();
        let has_pid = prompt_lower.contains("pid")
            || prompt_lower.contains("wait $!")
            || prompt_lower.contains("$!");
        assert!(has_pid, "ADD_META_PROMPT must instruct the AI to capture and wait on PIDs");
    }

    #[test]
    fn test_meta_prompt_enforces_wait_before_read() {
        // The prompt must explicitly instruct reading AI output only AFTER waiting.
        let prompt_lower = ADD_META_PROMPT.to_lowercase();
        let has_wait = prompt_lower.contains("wait")
            && (prompt_lower.contains("read") || prompt_lower.contains("consume") || prompt_lower.contains("output file"));
        assert!(has_wait, "ADD_META_PROMPT must instruct the AI to wait before reading AI output");
    }

    #[test]
    fn test_meta_prompt_enforces_cleanup() {
        // The prompt must instruct the AI to delete temp output files after reading.
        let prompt_lower = ADD_META_PROMPT.to_lowercase();
        let has_cleanup = prompt_lower.contains("delete")
            || prompt_lower.contains("rm ")
            || prompt_lower.contains("cleanup")
            || prompt_lower.contains("remove");
        assert!(has_cleanup, "ADD_META_PROMPT must instruct the AI to clean up temp AI output files");
    }

    #[test]
    fn test_meta_prompt_lang_placeholder() {
        // {LANG} must be present so build_add_prompt() can substitute the target language.
        assert!(
            ADD_META_PROMPT.contains("{LANG}"),
            "ADD_META_PROMPT must contain the {{LANG}} placeholder"
        );
    }

    #[test]
    fn test_meta_prompt_prompt_placeholder() {
        // {PROMPT} must be present so build_add_prompt() can inject user requirements.
        assert!(
            ADD_META_PROMPT.contains("{PROMPT}"),
            "ADD_META_PROMPT must contain the {{PROMPT}} placeholder"
        );
    }

    // ── ADD_META_PROMPT: new rule content checks ──────────────────────────────

    #[test]
    fn test_meta_prompt_verbose_log_has_verbose_prefix() {
        // verbose_log() must emit [VERBOSE] so users can identify diagnostic output.
        assert!(
            ADD_META_PROMPT.contains("[VERBOSE]"),
            "ADD_META_PROMPT must include [VERBOSE] prefix in verbose_log output"
        );
    }

    #[test]
    fn test_meta_prompt_no_colour_rule_present() {
        // The no-colour accessibility rule must be part of the default prompt.
        let lower = ADD_META_PROMPT.to_lowercase();
        assert!(
            lower.contains("colour") || lower.contains("color"),
            "ADD_META_PROMPT must mention the no-colour rule"
        );
        assert!(
            lower.contains("bold"),
            "ADD_META_PROMPT must instruct using bold instead of colour"
        );
    }

    // ── RULE_* constants ──────────────────────────────────────────────────────

    #[test]
    fn test_rule_logging_contains_verbose_prefix() {
        assert!(
            RULE_LOGGING.contains("[VERBOSE]"),
            "RULE_LOGGING must include [VERBOSE] prefix in the verbose_log definition"
        );
    }

    #[test]
    fn test_rule_no_colour_mentions_bold() {
        let lower = RULE_NO_COLOUR.to_lowercase();
        assert!(lower.contains("bold"), "RULE_NO_COLOUR must instruct using bold");
        assert!(
            lower.contains("colour") || lower.contains("color"),
            "RULE_NO_COLOUR must mention colour/color"
        );
    }

    #[test]
    fn test_rule_async_first_contains_background() {
        let lower = RULE_ASYNC_FIRST.to_lowercase();
        assert!(
            lower.contains("background") || lower.contains("backgrounded"),
            "RULE_ASYNC_FIRST must mention backgrounded calls"
        );
    }

    // ── AddConfig ─────────────────────────────────────────────────────────────

    #[test]
    fn test_add_config_default_all_enabled() {
        let cfg = AddConfig::default();
        assert!(cfg.rule_logging, "rule_logging should default to true");
        assert!(cfg.rule_no_colour, "rule_no_colour should default to true");
        assert!(cfg.rule_async_first, "rule_async_first should default to true");
    }

    #[test]
    fn test_add_config_deserialize_partial() {
        // Keys absent from JSON should default to true.
        let json = r#"{ "rule_logging": false }"#;
        let cfg: AddConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.rule_logging, "rule_logging should be false");
        assert!(cfg.rule_no_colour, "rule_no_colour should default to true");
        assert!(cfg.rule_async_first, "rule_async_first should default to true");
    }

    #[test]
    fn test_add_config_deserialize_all_false() {
        let json = r#"{ "rule_logging": false, "rule_no_colour": false, "rule_async_first": false }"#;
        let cfg: AddConfig = serde_json::from_str(json).unwrap();
        assert!(!cfg.rule_logging);
        assert!(!cfg.rule_no_colour);
        assert!(!cfg.rule_async_first);
    }

    // ── assemble_prompt_rules ─────────────────────────────────────────────────

    #[test]
    fn test_assemble_prompt_all_rules_default() {
        let prompt = assemble_prompt_rules(&AddConfig::default());
        assert!(prompt.contains("{LANG}"), "assembled prompt must have {{LANG}} placeholder");
        assert!(prompt.contains("{PROMPT}"), "assembled prompt must have {{PROMPT}} placeholder");
        assert!(prompt.contains("{SCRIPT_STEM}"), "assembled prompt must have {{SCRIPT_STEM}} placeholder");
        assert!(prompt.contains("[VERBOSE]"), "assembled prompt must include logging rule with [VERBOSE]");
        assert!(prompt.to_lowercase().contains("bold"), "assembled prompt must include no-colour rule");
        assert!(
            prompt.to_lowercase().contains("background") || prompt.to_lowercase().contains("backgrounded"),
            "assembled prompt must include async-first rule"
        );
    }

    #[test]
    fn test_assemble_prompt_logging_disabled() {
        let cfg = AddConfig { rule_logging: false, ..AddConfig::default() };
        let prompt = assemble_prompt_rules(&cfg);
        assert!(
            !prompt.contains("verbose_log"),
            "logging rule should be absent when rule_logging=false"
        );
        // Other rules should still be present.
        assert!(prompt.to_lowercase().contains("bold"));
        assert!(
            prompt.to_lowercase().contains("background") || prompt.to_lowercase().contains("backgrounded")
        );
    }

    #[test]
    fn test_assemble_prompt_no_colour_disabled() {
        let cfg = AddConfig { rule_no_colour: false, ..AddConfig::default() };
        let prompt = assemble_prompt_rules(&cfg);
        assert!(
            !prompt.to_lowercase().contains("accessibility"),
            "no-colour rule should be absent when rule_no_colour=false"
        );
        // Logging and async rules should still be present.
        assert!(prompt.contains("verbose_log"));
        assert!(
            prompt.to_lowercase().contains("background") || prompt.to_lowercase().contains("backgrounded")
        );
    }

    #[test]
    fn test_assemble_prompt_async_first_disabled() {
        let cfg = AddConfig { rule_async_first: false, ..AddConfig::default() };
        let prompt = assemble_prompt_rules(&cfg);
        assert!(
            !prompt.to_lowercase().contains("async-first"),
            "async-first rule should be absent when rule_async_first=false"
        );
        // Logging and no-colour rules should still be present.
        assert!(prompt.contains("verbose_log"));
        assert!(prompt.to_lowercase().contains("bold"));
    }

    #[test]
    fn test_assemble_prompt_all_rules_disabled_still_has_preamble_and_footer() {
        let cfg = AddConfig {
            rule_logging: false,
            rule_no_colour: false,
            rule_async_first: false,
        };
        let prompt = assemble_prompt_rules(&cfg);
        // Preamble and footer are always present.
        assert!(prompt.contains("script generator"), "preamble must always be present");
        assert!(prompt.contains("{LANG}"), "footer must always be present");
        assert!(prompt.contains("{PROMPT}"), "footer must always be present");
    }

    // ── build_add_prompt ──────────────────────────────────────────────────────

    #[test]
    fn test_build_add_prompt_substitutes_lang() {
        let result = build_add_prompt("list all files", &Lang::Bash, "list_files", &AddConfig::default());
        assert!(!result.contains("{LANG}"), "{{LANG}} placeholder must be replaced");
        assert!(result.to_lowercase().contains("bash"));
    }

    #[test]
    fn test_build_add_prompt_substitutes_prompt() {
        let result = build_add_prompt("do the thing", &Lang::Bash, "do_thing", &AddConfig::default());
        assert!(!result.contains("{PROMPT}"), "{{PROMPT}} placeholder must be replaced");
        assert!(result.contains("do the thing"));
    }

    #[test]
    fn test_build_add_prompt_js_language() {
        let result = build_add_prompt("list all files", &Lang::Js, "list_files", &AddConfig::default());
        assert!(!result.contains("{LANG}"), "{{LANG}} placeholder must be replaced");
        let lower = result.to_lowercase();
        assert!(lower.contains("javascript") || lower.contains("js"));
    }

    #[test]
    fn test_build_add_prompt_substitutes_script_stem() {
        let result = build_add_prompt("do the thing", &Lang::Bash, "my_script", &AddConfig::default());
        assert!(!result.contains("{SCRIPT_STEM}"), "{{SCRIPT_STEM}} placeholder must be replaced");
        assert!(result.contains("my_script"), "injected stem must appear in prompt");
    }

    #[test]
    fn test_build_add_prompt_injects_literal_paths() {
        let result = build_add_prompt("summarise logs", &Lang::Bash, "log_summary", &AddConfig::default());
        // The exact paths must appear literally so the AI cannot deviate.
        assert!(
            result.contains(".dt/scripts/log_summary/output/"),
            "output path must contain stem"
        );
        assert!(
            result.contains(".dt/scripts/log_summary/resources/acli_add_schema.json"),
            "schema path must contain stem"
        );
        assert!(
            result.contains(".dt/scripts/log_summary/resources/config.ag-stack.yml"),
            "config path must contain stem"
        );
    }

    #[test]
    fn test_build_add_prompt_inline() {
        let result = build_add_prompt("do x", &Lang::Bash, "do_x", &AddConfig::default());
        assert!(result.contains("do x"), "prompt must include user requirements");
        assert!(
            result.to_lowercase().contains("bash"),
            "prompt must mention bash"
        );
    }

    #[test]
    fn test_build_add_prompt_js() {
        let result = build_add_prompt("do x", &Lang::Js, "do_x", &AddConfig::default());
        let lower = result.to_lowercase();
        assert!(
            lower.contains("javascript") || lower.contains("js"),
            "prompt must mention javascript or js"
        );
    }

    #[test]
    fn test_build_add_prompt_respects_config_toggles() {
        // With logging disabled, verbose_log should not appear in the output.
        let cfg = AddConfig { rule_logging: false, ..AddConfig::default() };
        let result = build_add_prompt("do x", &Lang::Bash, "do_x", &cfg);
        assert!(!result.contains("verbose_log"), "verbose_log must not appear when rule_logging=false");
        // Preamble and user requirements must still be present.
        assert!(result.contains("do x"));
        assert!(result.to_lowercase().contains("bash"));
    }

    // ── Lang ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_lang_from_str_bash() {
        assert_eq!(Lang::from_str("bash").unwrap(), Lang::Bash);
        assert_eq!(Lang::from_str("BASH").unwrap(), Lang::Bash);
        assert_eq!(Lang::from_str("sh").unwrap(), Lang::Bash);
    }

    #[test]
    fn test_lang_from_str_js() {
        assert_eq!(Lang::from_str("js").unwrap(), Lang::Js);
        assert_eq!(Lang::from_str("javascript").unwrap(), Lang::Js);
        assert_eq!(Lang::from_str("JS").unwrap(), Lang::Js);
    }

    #[test]
    fn test_lang_from_str_unknown() {
        assert!(Lang::from_str("python").is_err());
    }

    #[test]
    fn test_lang_display_name() {
        assert_eq!(Lang::Bash.display_name(), "Bash");
        assert_eq!(Lang::Js.display_name(), "JavaScript");
    }
}
