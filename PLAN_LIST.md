# `dt list` — Implementation Plan

## Overview

Add a `dt list` subcommand that displays all scripts saved in `~/.dt/scripts/` with a
fuzzy-searchable interactive selector (the same `dialoguer::FuzzySelect` pattern used by
`dt run`). Once a script is selected the user is offered three actions: **edit**, **remove**,
or **rename**. The command has zero new dependencies — everything required is already present
in `Cargo.toml`.

---

## User Experience

```
$ dt list

  Search: git_
  ──────────────────────────────────────────────────
  git_clone_repos.sh       —  Clone all org repos to ~/work
▶ git_daily_summary.sh     —  Summarise commits from the last 24 h
  git_stash_all.sh         —  Stash changes across every dirty repo
  ──────────────────────────────────────────────────
  [enter to select]

# After selection:
What would you like to do with 'git_daily_summary.sh'?
  ❯ Edit
    Rename
    Remove
    ← Cancel
```

---

## Scope

| Feature | In scope |
|---------|----------|
| Fuzzy search over script names + descriptions | ✅ |
| Three post-selection actions: edit, rename, remove | ✅ |
| Edit opens script in `$VISUAL` / `$EDITOR` / `git config core.editor` | ✅ |
| Rename updates filename + sidecar `.meta.json` atomically | ✅ |
| Remove deletes script file + sidecar `.meta.json` with confirmation | ✅ |
| Works when no scripts exist (helpful error message) | ✅ |
| Re-opens list after cancel / after successful action (loop UX) | ❌ out of scope v1 |
| Filter by language / tag | ❌ out of scope v1 |

---

## Architecture

### New file: `src/commands/list.rs`

This is the only new file. It owns the full feature:

```
list.rs
 ├─ execute()                   ← public entry point wired into main.rs
 ├─ pick_script()               ← fuzzy-select step, returns ScriptEntry
 ├─ prompt_action()             ← second dialoguer::Select for edit/rename/remove/cancel
 ├─ action_edit()               ← open script in editor
 ├─ action_rename()             ← prompt new name, fs::rename + meta rename
 └─ action_remove()             ← confirm + fs::remove_file × 2
```

Shared helpers already in the codebase that `list.rs` will **reuse**:

| Symbol | Source | Used for |
|--------|--------|----------|
| `add::resolve_scripts_dir()` | `src/commands/add.rs` | Locate `~/.dt/scripts/` |
| `run::discover_scripts()` | `src/commands/run.rs` | Build the list of `ScriptEntry` values |
| `run::ScriptEntry` | `src/commands/run.rs` | Shared display label & metadata |
| `DT_VERBOSE_ENV` | `src/utils/ui.rs` | (not directly needed, but neighbours) |
| `ui::success / ui::error / ui::info` | `src/utils/ui.rs` | Consistent terminal output |

No duplication: discovery, path resolution, and display formatting are not re-implemented.

---

## Detailed Implementation

### 1. `src/commands/list.rs`

```rust
// High-level sketch — full implementation follows below

use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use dialoguer::{Confirm, FuzzySelect, Input, Select};

use crate::commands::add;
use crate::commands::run::{discover_scripts, ScriptEntry};
use crate::utils::ui;
```

#### 1a. `pick_script(scripts_dir) -> Result<ScriptEntry>`

```rust
fn pick_script(scripts_dir: &std::path::Path) -> Result<ScriptEntry> {
    let entries = discover_scripts(scripts_dir)?;

    if entries.is_empty() {
        return Err(anyhow!(
            "No scripts found in {}. Run `dt add` to generate one.",
            scripts_dir.display()
        ));
    }

    let labels: Vec<String> = entries.iter().map(|e| e.display_label()).collect();

    let selection = FuzzySelect::new()
        .with_prompt("Select a script")
        .items(&labels)
        .interact()?;

    // Consume the entry at that index.
    // discover_scripts returns an owned Vec so we can move out.
    Ok(entries.into_iter().nth(selection).unwrap())
}
```

Key design points:
- Uses the same `FuzzySelect` + `display_label()` approach as `dt run` — consistent UX.
- Returns the full `ScriptEntry` (filename + optional description) so the subsequent action
  functions have everything they need without re-reading the directory.

#### 1b. `prompt_action(entry) -> Result<Action>`

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
enum Action {
    Edit,
    Rename,
    Remove,
    Cancel,
}

fn prompt_action(entry: &ScriptEntry) -> Result<Action> {
    let items = ["Edit", "Rename", "Remove", "← Cancel"];

    let selection = Select::new()
        .with_prompt(format!("What would you like to do with '{}'?", entry.filename))
        .items(&items)
        .default(0)
        .interact()?;

    Ok(match selection {
        0 => Action::Edit,
        1 => Action::Rename,
        2 => Action::Remove,
        _ => Action::Cancel,
    })
}
```

Uses `dialoguer::Select` (non-fuzzy, fixed menu — 4 options). This is already available in
`Cargo.toml` via the `dialoguer` crate.

#### 1c. `action_edit(script_path)`

```rust
fn action_edit(script_path: &std::path::Path) -> Result<()> {
    // Resolve editor: $VISUAL > $EDITOR > git config core.editor
    // (same resolution order as add.rs::open_editor_for_prompt)
    let editor = resolve_editor()?;

    let mut parts = editor.split_whitespace();
    let bin = parts.next().ok_or_else(|| anyhow!("Editor command is empty"))?;

    std::process::Command::new(bin)
        .args(parts)
        .arg(script_path)
        .status()
        .with_context(|| format!("Failed to launch editor '{}'", editor))?;

    ui::success(&format!("Finished editing '{}'", script_path.display()));
    Ok(())
}
```

The editor resolution logic (`$VISUAL` → `$EDITOR` → `$GIT_EDITOR` → `git config core.editor`)
is identical to what `add.rs` already does in `open_editor_for_prompt()`. To avoid duplication
this logic should be **extracted into a shared private helper** called `resolve_editor()` in
`add.rs` (already `pub(crate)` or moved to `utils/ui.rs`). See the Refactor section below.

#### 1d. `action_rename(scripts_dir, entry)`

```rust
fn action_rename(scripts_dir: &std::path::Path, entry: &ScriptEntry) -> Result<()> {
    // 1. Prompt for the new filename
    let new_name: String = Input::new()
        .with_prompt("New filename")
        .with_initial_text(&entry.filename)
        .interact_text()?;

    let new_name = new_name.trim().to_string();

    if new_name.is_empty() {
        return Err(anyhow!("Filename cannot be empty."));
    }

    if new_name == entry.filename {
        ui::info("Name unchanged.");
        return Ok(());
    }

    // 2. Check for collision
    let new_script_path = scripts_dir.join(&new_name);
    if new_script_path.exists() {
        return Err(anyhow!(
            "A script named '{}' already exists. Choose a different name.",
            new_name
        ));
    }

    // 3. Rename script file
    let old_script_path = scripts_dir.join(&entry.filename);
    fs::rename(&old_script_path, &new_script_path)
        .with_context(|| format!("Failed to rename script to '{}'", new_name))?;

    // 4. Rename sidecar .meta.json (if it exists)
    let old_meta = scripts_dir.join(format!("{}.meta.json", entry.filename));
    let new_meta = scripts_dir.join(format!("{}.meta.json", new_name));
    if old_meta.exists() {
        fs::rename(&old_meta, &new_meta)
            .with_context(|| format!("Failed to rename metadata for '{}'", new_name))?;
    }

    ui::success(&format!("Renamed '{}' → '{}'", entry.filename, new_name));
    Ok(())
}
```

Key design points:
- Pre-fills the `Input` prompt with the current filename so the user can edit it in-place.
- Collision check prevents accidental overwrites.
- Sidecar rename is best-effort: if no `.meta.json` exists the rename is silently skipped.
- Both renames use `fs::rename` which is atomic on the same filesystem.

#### 1e. `action_remove(scripts_dir, entry)`

```rust
fn action_remove(scripts_dir: &std::path::Path, entry: &ScriptEntry) -> Result<()> {
    // 1. Confirm
    let confirmed = Confirm::new()
        .with_prompt(format!(
            "Permanently delete '{}'? This cannot be undone.",
            entry.filename
        ))
        .default(false)
        .interact()?;

    if !confirmed {
        ui::info("Deletion cancelled.");
        return Ok(());
    }

    // 2. Delete script
    let script_path = scripts_dir.join(&entry.filename);
    fs::remove_file(&script_path)
        .with_context(|| format!("Failed to delete '{}'", entry.filename))?;

    // 3. Delete sidecar (best-effort)
    let meta_path = scripts_dir.join(format!("{}.meta.json", entry.filename));
    if meta_path.exists() {
        let _ = fs::remove_file(&meta_path); // non-fatal if it fails
    }

    ui::success(&format!("Deleted '{}'.", entry.filename));
    Ok(())
}
```

Key design points:
- `Confirm` defaults to `false` (safe default — requires explicit `y`).
- Sidecar deletion is non-fatal (`let _ = ...`) since the script file is the source of truth.

#### 1f. `execute()` — public entry point

```rust
pub fn execute() -> Result<()> {
    let scripts_dir = add::resolve_scripts_dir()?;
    let entry = pick_script(&scripts_dir)?;

    match prompt_action(&entry)? {
        Action::Edit => {
            let script_path = scripts_dir.join(&entry.filename);
            action_edit(&script_path)?;
        }
        Action::Rename => {
            action_rename(&scripts_dir, &entry)?;
        }
        Action::Remove => {
            action_remove(&scripts_dir, &entry)?;
        }
        Action::Cancel => {
            ui::info("No action taken.");
        }
    }

    Ok(())
}
```

---

### 2. Refactor: extract `resolve_editor()` from `add.rs`

`add.rs` currently has editor resolution logic inline inside `open_editor_for_prompt()`.
`list.rs`'s `action_edit()` needs the same logic. To avoid duplication:

**Option A (preferred):** Extract into a `pub(crate) fn resolve_editor() -> Result<String>`
in `add.rs` and call it from both `open_editor_for_prompt()` and `action_edit()`.

**Option B:** Move it to `src/utils/ui.rs` as `pub fn resolve_editor() -> Result<String>`.

Option A keeps the change minimal (no new public surface). Option B is cleaner if more
commands need an editor in the future. Either works — pick A for now, move to B when a third
caller appears.

```rust
// In add.rs
pub(crate) fn resolve_editor() -> Result<String> {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .or_else(|_| std::env::var("GIT_EDITOR"))
        .or_else(|_| {
            let output = std::process::Command::new("git")
                .args(["config", "core.editor"])
                .output();
            match output {
                Ok(o) if o.status.success() => {
                    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    if s.is_empty() { Err(std::env::VarError::NotPresent) } else { Ok(s) }
                }
                _ => Err(std::env::VarError::NotPresent),
            }
        })
        .map_err(|_| anyhow!(
            "No editor found. Set $VISUAL or $EDITOR, or configure git config core.editor."
        ))
}
```

`open_editor_for_prompt()` then becomes:
```rust
let editor = resolve_editor()?;
```

---

### 3. Wire into `src/commands/mod.rs`

```rust
// Add one line:
pub mod list;
```

---

### 4. Wire into `src/main.rs`

#### 4a. Add import

```rust
use commands::{add, checkout, fixup, list, rovo, run, squash};
```

#### 4b. Add variant to `Commands` enum

```rust
/// List all saved scripts with fuzzy search; edit, rename, or remove a selection
List,
```

#### 4c. Add arm to `match cli.command`

```rust
Commands::List => {
    list::execute()?;
}
```

---

## Files Changed

| File | Change |
|------|--------|
| `src/commands/list.rs` | **New file** — full `list` feature |
| `src/commands/mod.rs` | Add `pub mod list;` |
| `src/main.rs` | Add `List` variant to `Commands`, import, and match arm |
| `src/commands/add.rs` | Extract `resolve_editor()` as `pub(crate)`, call it from `open_editor_for_prompt()` |

---

## No New Dependencies

All required crates are already present in `Cargo.toml`:

| Crate | Feature used |
|-------|-------------|
| `dialoguer` | `FuzzySelect`, `Select`, `Input`, `Confirm` |
| `anyhow` | `Result`, `anyhow!()`, `.with_context()` |
| `console` | (via `ui.rs` helpers) |
| `dirs` | (via `add::resolve_scripts_dir`) |
| `serde_json` | (via `run::discover_scripts` for `.meta.json` parsing) |

---

## Unit Tests

Add a `#[cfg(test)]` module inside `src/commands/list.rs`. Tests should use a
`tempfile::TempDir` (already in `[dependencies]`) as a fake scripts directory.

| Test | What it covers |
|------|---------------|
| `test_list_empty_dir` | `pick_script()` returns an error when the dir is empty |
| `test_list_no_dir` | `pick_script()` returns an error when the dir does not exist |
| `test_rename_updates_meta` | Rename moves both the script file and the `.meta.json` |
| `test_rename_collision` | Rename fails if the target name already exists |
| `test_rename_same_name` | Rename is a no-op when the new name equals the old name |
| `test_remove_deletes_both` | Remove deletes both the script and its `.meta.json` |
| `test_remove_no_meta` | Remove succeeds even when no `.meta.json` exists |

Interactive tests (`FuzzySelect`, `Select`, `Confirm`) cannot be unit-tested without a TTY,
so they are left to manual / integration testing.

---

## Integration Test Hook

Add a new test in `tests/integration_tests.rs`:

```rust
#[test]
fn test_list_help() {
    // Smoke-test: verify `dt list --help` exits 0 and mentions the command.
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_dt"))
        .arg("list")
        .arg("--help")
        .output()
        .expect("failed to run dt list --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("list") || stdout.contains("script"));
}
```

---

## Implementation Sequence

| Step | What | Key files |
|------|------|-----------|
| 1 | Extract `resolve_editor()` from `open_editor_for_prompt()` in `add.rs` | `src/commands/add.rs` |
| 2 | Create `src/commands/list.rs` with all helpers and `execute()` | `src/commands/list.rs` |
| 3 | Register module in `mod.rs` | `src/commands/mod.rs` |
| 4 | Wire `List` variant + match arm into `main.rs` | `src/main.rs` |
| 5 | Add unit tests inside `list.rs` | `src/commands/list.rs` |
| 6 | Add integration smoke-test | `tests/integration_tests.rs` |
| 7 | Ask user to run `cargo build && cargo test` and share output | — |

---

## Open Questions / Future Ideas

- **Loop UX:** After an action completes, re-show the list instead of exiting. This makes
  batch management (delete several scripts in a row) much smoother. Deferred to v2.
- **View / preview:** A fourth action — "View source" — that `cat`s or syntax-highlights the
  script content inline before deciding to edit. Deferred to v2.
- **Pass-through args for Edit:** `dt list` could accept `--editor` to override `$VISUAL`
  for that invocation only. Deferred.
- **Multi-select remove:** Select multiple scripts to delete in one pass using
  `dialoguer::MultiSelect`. Deferred.
