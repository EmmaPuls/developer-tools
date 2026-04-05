use std::{
    env,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{anyhow, Context, Result};
use dirs::home_dir;
use serde::{Deserialize, Serialize};

use crate::utils::ai::{AcliRovodev, AiCallSpec, AiProvider, AiSession};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Default location where generated scripts are stored (relative to $HOME).
pub const DEFAULT_SCRIPTS_DIR: &str = ".dt/scripts";

/// The meta-prompt template sent to the AI for script generation.
/// `{LANG}`, `{PROMPT}`, and `{SCRIPT_STEM}` are replaced at runtime via simple string substitution.
pub const ADD_META_PROMPT: &str = r#"
You are a script generator. Given the user's requirements below, produce a
shell script (or JavaScript script if instructed) that fulfils them.

The script will be saved at:
  $HOME/.dt/scripts/{SCRIPT_STEM}/{SCRIPT_STEM}.sh (or .js)

Async-first rules (MANDATORY — do not omit):
1. Identify EVERY value in the script that requires an AI response (any call
   to `acli rovodev`).
2. Emit ALL of those as backgrounded `acli rovodev` calls at the very top of
   the script — before any other work. Capture each PID immediately:
       acli rovodev "<prompt>" \
         --output-file=$HOME/.dt/scripts/{SCRIPT_STEM}/output/tmp_dt_ai_<name>.json \
         --output-schema="$HOME/.dt/scripts/{SCRIPT_STEM}/resources/acli_add_schema.json" \
         --config-file="$HOME/.dt/scripts/{SCRIPT_STEM}/resources/config.ag-stack.yml" &
       AI_<NAME>_PID=$!
3. Proceed with ALL deterministic steps concurrently while the AI calls run
   in the background.
4. Read each AI output file ONLY at the step that actually needs the value —
   and not before. Always wait on the PID first:
       wait $AI_<NAME>_PID
       RESULT=$(jq -r '.value' $HOME/.dt/scripts/{SCRIPT_STEM}/output/tmp_dt_ai_<name>.json)
       rm -f $HOME/.dt/scripts/{SCRIPT_STEM}/output/tmp_dt_ai_<name>.json
5. Choose a short, descriptive snake_case filename with the correct extension
   (.sh for Bash, .js for JavaScript). The filename stem MUST be {SCRIPT_STEM}.
6. Determine what structured data each `acli rovodev` call in the script will
   return and define an `output_schema` JSON Schema for it. Include that schema
   verbatim in your JSON response under the key "output_schema". This schema
   will be written to the script's resources/ folder and passed via
   --output-schema to every acli rovodev call in the generated script.
7. Return ONLY valid JSON in this exact shape — no markdown, no prose:
   {
     "filename":      "<snake_case_name.ext>",
     "description":   "<one sentence describing what the script does>",
     "script":        "<full script content as a single string>",
     "output_schema": { <JSON Schema object for the script's acli rovodev calls> }
   }

Target language: {LANG}

User requirements:
{PROMPT}
"#;

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

/// Substitutes `{LANG}`, `{PROMPT}`, and `{SCRIPT_STEM}` into `ADD_META_PROMPT`.
pub fn build_add_prompt(user_requirements: &str, lang: &Lang, script_stem: &str) -> String {
    ADD_META_PROMPT
        .replace("{LANG}", lang.display_name())
        .replace("{PROMPT}", user_requirements)
        .replace("{SCRIPT_STEM}", script_stem)
}

/// Resolves `~/.dt/scripts`, creating the directory if needed.
pub fn resolve_scripts_dir() -> Result<PathBuf> {
    let dir = home_dir()
        .ok_or_else(|| anyhow!("Could not determine home directory"))?
        .join(DEFAULT_SCRIPTS_DIR);
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
    let home = home_dir().ok_or_else(|| anyhow!("Could not determine home directory"))?;
    let resources_dir = home.join(".dt").join("resources");

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
    let prompt = build_add_prompt(&user_requirements, &lang, &script_stem);

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
    // We source it from ~/.dt/resources/ so any user edits there are propagated;
    // fall back to the embedded template if it doesn't exist yet.
    let global_config = home_dir()
        .ok_or_else(|| anyhow!("Could not determine home directory"))?
        .join(".dt")
        .join("resources")
        .join("config.ag-stack.yml");
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

    #[test]
    fn test_build_add_prompt_substitutes_lang() {
        let result = build_add_prompt("list all files", &Lang::Bash, "list_files");
        assert!(!result.contains("{LANG}"), "{{LANG}} placeholder must be replaced");
        assert!(result.to_lowercase().contains("bash"));
    }

    #[test]
    fn test_build_add_prompt_substitutes_prompt() {
        let result = build_add_prompt("do the thing", &Lang::Bash, "do_thing");
        assert!(!result.contains("{PROMPT}"), "{{PROMPT}} placeholder must be replaced");
        assert!(result.contains("do the thing"));
    }

    #[test]
    fn test_build_add_prompt_js_language() {
        let result = build_add_prompt("list all files", &Lang::Js, "list_files");
        assert!(!result.contains("{LANG}"), "{{LANG}} placeholder must be replaced");
        let lower = result.to_lowercase();
        assert!(lower.contains("javascript") || lower.contains("js"));
    }

    #[test]
    fn test_build_add_prompt_substitutes_script_stem() {
        let result = build_add_prompt("do the thing", &Lang::Bash, "my_script");
        assert!(!result.contains("{SCRIPT_STEM}"), "{{SCRIPT_STEM}} placeholder must be replaced");
        assert!(result.contains("my_script"), "injected stem must appear in prompt");
    }

    #[test]
    fn test_build_add_prompt_injects_literal_paths() {
        let result = build_add_prompt("summarise logs", &Lang::Bash, "log_summary");
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

    // ── build_add_prompt ──────────────────────────────────────────────────────

    #[test]
    fn test_build_add_prompt_inline() {
        let result = build_add_prompt("do x", &Lang::Bash, "do_x");
        assert!(result.contains("do x"), "prompt must include user requirements");
        assert!(
            result.to_lowercase().contains("bash"),
            "prompt must mention bash"
        );
    }

    #[test]
    fn test_build_add_prompt_js() {
        let result = build_add_prompt("do x", &Lang::Js, "do_x");
        let lower = result.to_lowercase();
        assert!(
            lower.contains("javascript") || lower.contains("js"),
            "prompt must mention javascript or js"
        );
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
