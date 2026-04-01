# Goal 2: `dt add` — AI Script Generator — Detailed Implementation Plan

## Overview

Implement `src/commands/add.rs` as a new `dt add` subcommand that accepts a user prompt (inline via `-m` or via an editor session), sends it through the `AiProvider` abstraction from Goal 1, and saves the generated script + sidecar metadata to `~/.dt/scripts/`.

---

## TDD Sequence — Red → Green

The plan follows strict TDD: **write tests first, confirm they fail, then implement**.

> **⚠️ Cargo command convention:** The implementor (AI) **does not run `cargo` commands directly**. Instead, at each step where a build or test run is needed, the implementor provides the exact command to run and asks the user to run it and share the output. The user is the one who executes all `cargo` invocations.

---

## Phase 00 — Regression-net: establish baseline before adding `dt add`

> **⚠️ Cargo convention:** The AI does **not** run `cargo` commands. Provide the exact command and ask the user to run it and share output.

**Goal:** Confirm the full existing test suite is green before any new files are touched. This gives a clean baseline to diff against after each phase.

---

### Concrete steps for Phase 00

1. **Ask the user to run the existing test suite:**
   > Please run `cargo test` and share the output.

   All existing tests must be green before we proceed.

2. **Record the existing subcommand list** to confirm `add` is not yet present:
   > Please run `dt --help` and share the output.

---

## Phase 0 — Write tests FIRST, confirm RED

> **⚠️ Cargo convention:** The AI does **not** run `cargo` commands. Provide the exact command and ask the user to run it and share output.

Write all tests before writing any implementation code. After writing the tests, ask the user to confirm they fail:

> Please run `cargo test` and share the output — the new `add` tests should be RED (compile error or test failure) at this point.

---

### Integration tests to add to `tests/integration_tests.rs`

#### 1. `test_add_help`

```rust
#[test]
fn test_add_help() {
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["add", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Generate a script using AI"));
}
```

**Why:** Verifies `add` is wired into clap and its help text is present. Cheapest possible smoke test — will catch accidental removal or mis-wiring during future refactors.

---

#### 2. `test_add_requires_prompt_or_message`

```rust
#[test]
fn test_add_requires_prompt_or_message() {
    // When stdin is not a TTY and no -m flag is given, the editor path will
    // fail to open $VISUAL/$EDITOR. The command must fail cleanly (non-zero
    // exit) without panicking or hanging.
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["add"])
        .write_stdin("")   // non-TTY stdin, no editor available
        .env_remove("VISUAL")
        .env_remove("EDITOR")
        .assert()
        .failure();
}
```

**Why:** Guards the editor-open path: when no prompt is provided and no editor env var is set, `dt add` must fail with a clean error message, not hang or panic.

---

#### 3. `test_add_inline_message_flag_accepted`

```rust
#[test]
fn test_add_inline_message_flag_accepted() {
    // The -m flag must be accepted by clap (it may still fail later when
    // trying to spawn `acli`, but it must not fail at argument parsing).
    // We verify the failure mode is NOT "unrecognised argument".
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["add", "-m", "copy all .md files from ~/notes to ~/backup"])
        .assert()
        .stderr(predicate::str::contains("unrecognised").not())
        .stderr(predicate::str::contains("unexpected argument").not());
}
```

**Why:** Confirms the `-m`/`--message` flag is registered with clap and the prompt is captured correctly before the AI call is attempted.

---

#### 4. `test_add_lang_flag_accepted`

```rust
#[test]
fn test_add_lang_flag_accepted() {
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["add", "-m", "list all files", "--lang", "js"])
        .assert()
        .stderr(predicate::str::contains("unrecognised").not())
        .stderr(predicate::str::contains("unexpected argument").not());
}
```

**Why:** Confirms the `--lang` flag (default: `bash`, alternative: `js`) is registered and parsed without error.

---

### Unit tests to add in `src/commands/add.rs` (inside `#[cfg(test)]`)

| Test | What it asserts |
|------|----------------|
| `test_strip_comment_lines` | `strip_comment_lines("# foo\nbar\n# baz\nqux")` → `"bar\nqux"` |
| `test_strip_comment_lines_empty` | All-comment input → empty string |
| `test_strip_comment_lines_preserves_blank_lines` | Blank lines between content lines are preserved |
| `test_scripts_dir_constant` | `DEFAULT_SCRIPTS_DIR` resolves to a path containing `.dt/scripts` |
| `test_meta_prompt_contains_json_instruction` | `ADD_META_PROMPT` contains the string `"filename"` and `"description"` and `"script"` (JSON field names the AI must emit) |
| `test_meta_prompt_contains_async_instruction` | `ADD_META_PROMPT` contains the word `"background"` or `"async"` (enforcing Goal 3 pattern) |
| `test_build_add_prompt_inline` | `build_add_prompt("do x", Lang::Bash)` returns a string that includes `"do x"` and `"bash"` |
| `test_build_add_prompt_js` | `build_add_prompt("do x", Lang::Js)` returns a string that includes `"javascript"` or `"js"` |

---

## Phase 1 — Create `src/commands/add.rs`

> **⚠️ Cargo convention:** The AI does **not** run `cargo` commands. Provide the exact command and ask the user to run it and share output.

### Public types and constants

```rust
/// Default location where generated scripts are stored.
pub const DEFAULT_SCRIPTS_DIR: &str = ".dt/scripts";

/// Target scripting language for generated scripts.
#[derive(Debug, Clone, PartialEq)]
pub enum Lang {
    Bash,
    Js,
}

impl Lang {
    /// Parse from a CLI string (case-insensitive).
    pub fn from_str(s: &str) -> Result<Self> { ... }

    /// Return the display name used in the meta-prompt.
    pub fn display_name(&self) -> &'static str {
        match self {
            Lang::Bash => "Bash",
            Lang::Js   => "JavaScript",
        }
    }
}

/// Structured JSON output expected from the AI.
#[derive(Deserialize)]
pub struct AddOutput {
    pub filename:    Option<String>,
    pub description: Option<String>,
    pub script:      Option<String>,
}

/// Sidecar metadata written alongside the saved script.
#[derive(Serialize)]
pub struct ScriptMeta {
    pub description: String,
}
```

---

### The meta-prompt constant

```rust
const ADD_META_PROMPT: &str = r#"
You are a script generator. Given the user's requirements below, produce a
shell script (or JavaScript script if instructed) that fulfils them.

Rules:
1. Identify every value in the script that requires an AI response. Emit ALL
   of those as backgrounded `acli rovodev` calls at the very top of the
   script — before any other work — capturing the PID of each.
2. Proceed with all deterministic steps concurrently while the AI calls run.
3. Read each AI output file only at the step that actually needs the value
   (`wait $PID`, then `jq -r '.value' <file>`). Delete the temp file after.
4. Choose a short, descriptive snake_case filename with the correct extension
   (.sh for Bash, .js for JavaScript).
5. Return ONLY valid JSON in this exact shape — no markdown, no prose:
   {
     "filename":    "<snake_case_name.ext>",
     "description": "<one sentence describing what the script does>",
     "script":      "<full script content as a single string>"
   }

Target language: {LANG}

User requirements:
{PROMPT}
"#;
```

> **Note:** `{LANG}` and `{PROMPT}` are placeholder markers replaced at runtime via simple string substitution — no external template engine needed.

---

### Helper functions

#### `strip_comment_lines(text: &str) -> String`

Removes lines that start with `#` (after trimming leading whitespace). Used to clean editor-session content before sending to the AI.

```rust
fn strip_comment_lines(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}
```

#### `build_add_prompt(user_requirements: &str, lang: &Lang) -> String`

Substitutes `{LANG}` and `{PROMPT}` into `ADD_META_PROMPT`.

```rust
fn build_add_prompt(user_requirements: &str, lang: &Lang) -> String {
    ADD_META_PROMPT
        .replace("{LANG}", lang.display_name())
        .replace("{PROMPT}", user_requirements)
}
```

#### `open_editor_for_prompt() -> Result<String>`

1. Creates a `NamedTempFile` with a `.md` extension.
2. Writes the editor template (comment-style instructions + blank body section) into it.
3. Resolves the editor binary: `$VISUAL` → `$EDITOR` → `git config core.editor` → error.
4. Spawns the editor with `.status()` (blocking).
5. Reads the file back, calls `strip_comment_lines()`, trims whitespace.
6. If the result is empty, returns `Err("no prompt provided — editor was empty")`.
7. Returns the cleaned prompt string.

**Editor template pre-populated into the temp file:**

```markdown
# dt add — describe the script you want to generate
#
# Write your requirements below this comment block.
# Lines starting with '#' will be ignored.
# Save and close the editor when done.
#
# Example:
#   Copy all .md files from ~/notes to ~/backup and summarise their contents.
#

```

#### `resolve_scripts_dir() -> Result<PathBuf>`

Expands `~/.dt/scripts` using `dirs::home_dir()`. Creates the directory if it doesn't exist. Returns the resolved `PathBuf`.

---

### `execute()` — main entry point

```rust
pub fn execute(message: Option<String>, lang: Lang) -> Result<()> {
    // 1. Capture prompt
    let user_requirements = match message {
        Some(m) => m,
        None    => open_editor_for_prompt()?,
    };

    // 2. Build full meta-prompt
    let prompt = build_add_prompt(&user_requirements, &lang);

    // 3. Set up AI resources
    let provider = AcliRovodev;
    let resources_dir = resolve_scripts_dir()?.parent().unwrap().join("resources");
    let paths = provider.setup_resources(&resources_dir)?;

    // 4. Build spec + session
    let spec = AiCallSpec {
        prompt,
        output_path: paths.output_file.clone(),
    };
    let mut session = AiSession::new(paths.output_file.clone());

    // 5. Run AI call — blocking (we need the script before we can save it)
    provider.build_command(&spec, &paths)
        .status()
        .context("Failed to execute acli rovodev")?;

    // 6. Parse output
    let output = provider
        .read_output::<AddOutput>(&mut session)?
        .ok_or_else(|| anyhow::anyhow!("AI returned no output or unparseable JSON"))?;

    let filename    = output.filename.ok_or_else(|| anyhow::anyhow!("AI response missing 'filename'"))?;
    let description = output.description.unwrap_or_else(|| "No description provided.".to_string());
    let script      = output.script.ok_or_else(|| anyhow::anyhow!("AI response missing 'script'"))?;

    // 7. Save script
    let scripts_dir  = resolve_scripts_dir()?;
    let script_path  = scripts_dir.join(&filename);
    fs::write(&script_path, &script)?;
    set_executable(&script_path)?;

    // 8. Write sidecar .meta.json
    let meta_path = scripts_dir.join(format!("{}.meta.json", filename));
    let meta      = ScriptMeta { description: description.clone() };
    fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)?;

    // 9. AiSession cleanup (output file already deleted by read_output, this is a no-op)
    drop(session);

    // 10. Print success
    println!("✓ Script saved to: {}", script_path.display());
    println!("  {}", description);

    Ok(())
}
```

#### `set_executable(path: &Path) -> Result<()>`

Uses `std::fs::Permissions` + `std::os::unix::fs::PermissionsExt` to set mode `0o755`.

```rust
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
    Ok(()) // no-op on non-Unix platforms
}
```

---

## Phase 2 — Wire `add` into `src/commands/mod.rs`

> **⚠️ Cargo convention:** The AI does **not** run `cargo` commands. Provide the exact command and ask the user to run it and share output.

```rust
pub mod add;       // add this line
pub mod checkout;
pub mod fixup;
pub mod rovo;
pub mod squash;
```

---

## Phase 3 — Wire `Add` into `src/main.rs`

> **⚠️ Cargo convention:** The AI does **not** run `cargo` commands. Provide the exact command and ask the user to run it and share output.

### Import

```rust
use commands::{add, checkout, fixup, rovo, squash};
```

### New `Commands` variant

```rust
/// Generate a script using AI from a prompt or interactive editor session
Add {
    /// Describe the script to generate (skips editor if provided)
    #[arg(short, long)]
    message: Option<String>,
    /// Target scripting language
    #[arg(long, default_value = "bash", value_parser = ["bash", "js"])]
    lang: String,
},
```

### New `match` arm in `main()`

```rust
Commands::Add { message, lang } => {
    let lang = add::Lang::from_str(&lang)?;
    add::execute(message, lang)?;
}
```

---

## Phase 4 — Run full test suite, confirm GREEN

> **⚠️ Cargo convention:** The AI does **not** run `cargo` commands. Provide the exact command and ask the user to run it and share output.

Ask the user to run the full test suite and share the output:

> Please run `cargo test` and share the output.

All existing tests + new unit/integration tests must pass green.

Then confirm the new subcommand is visible:

> Please run `dt --help` and share the output — you should see `add` listed alongside the other subcommands.

> Please run `dt add --help` and share the output — you should see `-m`/`--message` and `--lang` flags.

---

## Key Design Decisions

| Decision | Rationale |
|---|---|
| `AiProvider` from `ai.rs` reused as-is | No changes to the abstraction — this validates that Goal 1's design is general enough |
| Resources dir is `~/.dt/resources/` (shared) | Both `fixup` and `add` write the same schema + config files; `setup_resources` is idempotent so sharing is safe |
| Blocking AI call | Script content must be available before it can be written to disk; no benefit to backgrounding here |
| `tempfile::NamedTempFile` for editor session | Auto-deleted if the process exits early; avoids leaking `.md` files |
| `strip_comment_lines` | Keeps the editor template instructions visible to the user without polluting the AI prompt |
| Sidecar `.meta.json` | Decoupled from the script itself; consumed by `dt run`'s fuzzy list (Goal: `dt run`) |
| `set_executable` gated on `#[cfg(unix)]` | Keeps Windows compilation valid even though `chmod` is a Unix concept |

---

## TODOs (future, not in scope for this goal)

> **TODO (future):** Add a `scripts_dir` key to `~/.dt/config` (shared dt config file) so users can override the save location without recompiling.

> **TODO (future):** Add an `editor` key to `~/.dt/config` so users can override the editor without relying on `$VISUAL` / `$EDITOR` / git config.

> **TODO (future):** Expose `meta_prompt` as a configurable key in `.dt-add-config` so power users can override the generation instructions entirely.

> **TODO (future):** Support `--lang` values beyond `bash` and `js` (e.g. `python`, `ruby`).

---

## File Change Summary

| File | Change |
|------|--------|
| `src/commands/add.rs` | **New** — `Lang`, `AddOutput`, `ScriptMeta`, `ADD_META_PROMPT` const, helper fns, `execute()`, unit tests |
| `src/commands/mod.rs` | Add `pub mod add;` |
| `src/main.rs` | Add `Add` variant to `Commands`; add `add` to import list; add match arm |
| `tests/integration_tests.rs` | Add 4 integration tests for the `add` subcommand |
