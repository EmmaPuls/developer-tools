# Goal 1: AI Provider Abstraction — Detailed Implementation Plan

## Overview

Create `src/utils/ai.rs` as an **interface + concrete implementation** layer, then refactor `fixup.rs` to use it. The three inline functions in `fixup.rs` (`setup_fixup_resources`, `request_commit_message`, `extract_commit_message`) are replaced by the new abstraction.

---

## TDD Sequence — Red → Green

The plan follows strict TDD: **write tests first, confirm they fail, then implement**.

> **⚠️ Cargo command convention:** The implementor (AI) **does not run `cargo` commands directly**. Instead, at each step where a build or test run is needed, the implementor provides the exact command to run and asks the user to run it and share the output. The user is the one who executes all `cargo` invocations.

---

## Phase 00 — Regression-net: write tests for `dt fixup`'s current AI behaviour

**Goal:** Capture the observable behaviour of `dt fixup`'s AI path *before* the refactor so that the exact same tests can be re-run at the end of Phase 4 to confirm nothing regressed.

---

### Why this phase exists

The three private functions we're about to delete (`setup_fixup_resources`, `request_commit_message`, `extract_commit_message`) are not currently covered by any test. If we refactor and break them we won't know until a human tries the real workflow. Phase 00 puts a safety net under those functions *first*.

---

### What is already tested (don't duplicate)

The existing integration suite in `tests/integration_tests.rs` already covers:

| Existing test | What it guards |
|---|---|
| `test_help_commands` | `dt --help`, `dt checkout --help`, etc. |
| `test_version_and_help` | `--version` flag |
| `test_rovo_basic` / `test_rovo_no_args` | `dt rovo` path |
| `test_checkout_*` | checkout command |
| `test_squash_*` | squash command |

There is **no test** for `dt fixup` at all. We add three targeted ones below.

---

### Tests to add to `tests/integration_tests.rs`

#### 1. `test_fixup_help`

```rust
#[test]
fn test_fixup_help() {
    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["fixup", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Manage the stack fixup workflow"));
}
```

**Why:** Verifies the `fixup` subcommand is wired into clap and its help text is present. This is the cheapest possible smoke test and will catch any accidental removal of the subcommand during refactor.

---

#### 2. `test_fixup_init_not_in_git_repo`

```rust
#[test]
fn test_fixup_init_not_in_git_repo() {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["fixup", "init"])
        .current_dir(&temp_dir)
        // Provide piped stdin so dialoguer doesn't hang waiting for a TTY
        .write_stdin("\n\n\n\n\n\n\n\n\n\n")
        .assert()
        // It may succeed (writes config) or fail — but must never panic/SIGABRT.
        // The key invariant is: exit code is 0 or 1, not a signal.
        .stderr(predicate::str::is_empty().not());
}
```

**Why:** `run_init` calls `prompt_and_write_config` → `prompt_for_config` (10 `dialoguer` prompts) and then `setup_fixup_resources`. In a non-TTY CI environment `dialoguer` may return an error instead of blocking; the test confirms we get a clean error exit rather than a panic or signal termination. This matches the pattern used by `test_checkout_not_in_git_repo` and `test_squash_not_in_git_repo`.

> **Note on TTY behaviour:** `assert_cmd` sets up a non-interactive pipe for stdin. `dialoguer` will return `Err` when there is no TTY. The command should propagate that as a non-zero exit with an error message on stderr — not a panic. Confirm this is the case before moving on.

---

#### 3. `test_fixup_no_args_not_in_git_repo`

```rust
#[test]
fn test_fixup_no_args_not_in_git_repo() {
    let temp_dir = tempfile::tempdir().unwrap();

    let mut cmd = Command::cargo_bin("dt").unwrap();
    cmd.args(["fixup"])
        .current_dir(&temp_dir)
        .write_stdin("\n\n\n\n\n\n\n\n\n\n")
        .assert()
        .failure();
}
```

**Why:** `run_fixup` → `read_config` (reads `~/.dt-fixup-config`, which may or may not exist on the test machine) → if no config, calls `prompt_and_write_config`. Either path eventually calls `execute` → `git::ensure_git_repo()` which should fail cleanly outside a git repo. This test ensures the entry-point survives the refactor intact.

---

### Unit tests to add in `src/commands/fixup.rs` (inside `#[cfg(test)]`)

These test the *current* private logic that is about to be deleted. They go green against the existing code, then we delete the functions in Phase 3 and expect these tests to be *removed* (replaced by the equivalent `ai.rs` unit tests from Phase 0).

> **Alternative approach:** Skip inlining unit tests for the soon-to-be-deleted functions and rely only on the integration tests above. This avoids writing tests that are immediately thrown away. **Recommendation: use this approach** — the integration tests above plus the unit tests in Phase 0's `ai.rs` provide sufficient coverage.

---

### Concrete steps for Phase 00

1. **Ask the user to run the existing test suite** to establish a clean baseline:
   > Please run `cargo test` and share the output.

   All tests must be green before we write any new ones.

2. **Add the three integration tests** above to `tests/integration_tests.rs`.

3. **Ask the user to run `cargo test` again** — all three new tests should go **green** immediately (they test existing behaviour, not future behaviour). If any fail, fix `dt fixup` *before* touching anything else.
   > Please run `cargo test` and share the output.

4. **Record the baseline.** Ask the user to share the output of `dt fixup --help` so we can assert on the exact text again in Phase 4.
   > Please run `dt fixup --help` and share the output.

---

### Key facts about `fixup.rs` gathered for implementation clarity

| Detail | Value |
|---|---|
| Config file location | `~/.dt-fixup-config` (key=value, `#` comments) |
| Current `FixupOptions` AI fields | `acli_output_file`, `acli_output_schema`, `acli_config_file` (all `String`) |
| Config keys for those fields | `ACLI_OUTPUT_FILE`, `ACLI_OUTPUT_SCHEMA`, `ACLI_CONFIG_FILE` |
| Default `acli_output_file` | `"../fixup/resources/acli_rovodev_output.json"` (relative path — this is a smell; Phase 3 replaces with `~/.dt/fixup-resources/`) |
| Embedded bytes | `ACLI_SCHEMA_TEMPLATE` (`include_bytes!("../resources/acli_schema.json")`), `ACLI_CONFIG_TEMPLATE` (`include_bytes!("../resources/config.ag-stack.yml")`) |
| `setup_fixup_resources` | Writes schema + config from embedded bytes (skip if exists), creates parent dirs for output file |
| `request_commit_message` | Runs `acli rovodev <message> --output-file=… --output-schema=… --config-file=…` blocking |
| `extract_commit_message` | Reads `acli_output_file`, parses `{ "commit_message": "…" }`, deletes file, returns `Option<String>` |
| `verify_acli_inputs` | Guards that none of the three path strings are empty — will be removed in Phase 3 |
| `AcliOutput` struct | `#[derive(Deserialize)] struct AcliOutput { commit_message: Option<String> }` — moves to `ai.rs` |
| `prompt_for_config` | Prompts for ALL 10 fields including the three AI paths — Phase 3 reduces this to one `acli_resources_dir` prompt |
| `write_config_file` | Writes all 10 keys — Phase 3 removes `ACLI_OUTPUT_FILE`, `ACLI_OUTPUT_SCHEMA`, `ACLI_CONFIG_FILE`, adds `ACLI_RESOURCES_DIR` |
| `apply_config_value` | Handles all 10 keys with a `match` — Phase 3 updates this accordingly |



## Phase 0 — Write tests FIRST, confirm RED

After writing the tests, ask the user to confirm they fail:
> Please run `cargo test` and share the output — the new `ai.rs` tests should be RED at this point.

### Unit tests to write in `src/utils/ai.rs` (stub module only)

| Test | What it asserts |
|------|----------------|
| `test_ai_paths_construction` | `AiPaths` holds the three paths correctly |
| `test_ai_call_spec_construction` | `AiCallSpec` stores `prompt` and `output_path` |
| `test_acli_rovodev_build_command_args` | `AcliRovodev::build_command()` produces a `Command` with correct args: `acli rovodev <prompt> --output-file=… --output-schema=… --config-file=…` |
| `test_ai_session_cleanup_removes_file` | Create a temp file, wrap in `AiSession`, call `.cleanup()` → file is deleted |
| `test_ai_session_drop_removes_file` | Same but letting `AiSession` fall out of scope via `Drop` |
| `test_setup_resources_creates_files` | `AcliRovodev::setup_resources(dir)` writes schema + config files to `dir`, returns correct `AiPaths` |
| `test_setup_resources_idempotent` | Calling `setup_resources` twice doesn't error and doesn't overwrite existing files |
| `test_read_output_parses_json` | Write `{ "commit_message": "hello" }` to a file, call `read_output::<AcliOutput>()`, get `Some("hello")` |
| `test_read_output_missing_file_returns_none` | `read_output` on non-existent path returns `Ok(None)` |
| `test_read_output_bad_json_returns_none` | `read_output` on malformed JSON returns `Ok(None)` |

### Integration tests to add to `tests/integration_tests.rs`

| Test | What it asserts |
|------|----------------|
| `test_fixup_help` | `dt fixup --help` still works and shows expected text after refactor |
| `test_fixup_init_not_in_git_repo` | After refactor, `dt fixup init` outside a git repo errors cleanly (no crash/panic) |

---

## Phase 1 — Create `src/utils/ai.rs`

### Public types

```rust
pub struct AiCallSpec {
    pub prompt: String,
    pub output_path: PathBuf,   // full path for --output-file
}

pub struct AiPaths {
    pub output_file: PathBuf,
    pub schema_file: PathBuf,
    pub config_file: PathBuf,
}

// RAII guard — deletes output_file on drop
pub struct AiSession {
    pub output_file: PathBuf,
    cleaned_up: bool,
}

impl AiSession {
    pub fn new(output_file: PathBuf) -> Self { ... }
    pub fn cleanup(&mut self) { /* delete file, set flag */ }
}

impl Drop for AiSession {
    fn drop(&mut self) { self.cleanup(); }
}

pub trait AiProvider {
    fn setup_resources(&self, dir: &Path) -> Result<AiPaths>;
    fn build_command(&self, spec: &AiCallSpec, paths: &AiPaths) -> Command;
    fn read_output<T: DeserializeOwned>(&self, session: &mut AiSession) -> Result<Option<T>>;
}

pub struct AcliRovodev;
```

### Embedded resource bytes (moved from `fixup.rs`)

```rust
const ACLI_SCHEMA_TEMPLATE: &[u8] = include_bytes!("../resources/acli_schema.json");
const ACLI_CONFIG_TEMPLATE: &[u8] = include_bytes!("../resources/config.ag-stack.yml");
```

### `setup_resources` behaviour

1. Creates `dir` if it doesn't exist
2. Writes `acli_schema.json` → `dir/acli_schema.json` (skip if already exists — idempotent)
3. Writes `config.ag-stack.yml` → `dir/config.ag-stack.yml` (skip if already exists)
4. Returns `AiPaths { output_file: dir/acli_rovodev_output.json, schema_file: dir/acli_schema.json, config_file: dir/config.ag-stack.yml }`

### `build_command` behaviour

```rust
fn build_command(&self, spec: &AiCallSpec, paths: &AiPaths) -> Command {
    let mut cmd = Command::new("acli");
    cmd.arg("rovodev")
       .arg(&spec.prompt)
       .arg(format!("--output-file={}", paths.output_file.display()))
       .arg(format!("--output-schema={}", paths.schema_file.display()))
       .arg(format!("--config-file={}", paths.config_file.display()));
    cmd
}
```

### `read_output` behaviour

```rust
fn read_output<T: DeserializeOwned>(&self, session: &mut AiSession) -> Result<Option<T>> {
    let path = &session.output_file;
    if !path.exists() { return Ok(None); }
    let data = fs::read_to_string(path)?;
    let parsed = serde_json::from_str::<T>(&data).ok();
    session.cleanup();
    Ok(parsed)
}
```

---

## Phase 2 — Wire into `src/utils/mod.rs`

```rust
pub mod ai;   // add this line
pub mod git;
pub mod ui;
```

---

## Phase 3 — Refactor `fixup.rs`

### What gets removed

| Removed from `fixup.rs` | Replaced by |
|-------------------------|-------------|
| `const ACLI_SCHEMA_TEMPLATE` | lives in `ai.rs` |
| `const ACLI_CONFIG_TEMPLATE` | lives in `ai.rs` |
| `fn setup_fixup_resources(options)` | `AcliRovodev.setup_resources(dir)` |
| `fn request_commit_message(options, message)` | `AcliRovodev.build_command(spec, paths).status()` |
| `fn extract_commit_message(options)` | `AcliRovodev.read_output::<AcliOutput>(session)` |
| `fn verify_acli_inputs(options)` | no longer needed — paths always come from `AiPaths` |

### `FixupOptions` field changes

Remove the three path fields (`acli_output_file`, `acli_output_schema`, `acli_config_file`) from `FixupOptions`. Replace with a single `acli_resources_dir: PathBuf` defaulting to `~/.dt/fixup-resources/`. All three concrete paths are derived by `setup_resources()`. The config file key changes from three separate keys to `ACLI_RESOURCES_DIR`; unknown old keys are silently skipped (existing behaviour for unknown keys).

### Updated `execute()` flow

```rust
let provider = AcliRovodev;
let paths = provider.setup_resources(&options.acli_resources_dir)?;
let spec = AiCallSpec {
    prompt: acli_message,
    output_path: paths.output_file.clone(),
};
let mut session = AiSession::new(paths.output_file.clone());
provider.build_command(&spec, &paths)
    .status()
    .context("Failed to execute acli rovodev")?;
let suggested_message = provider
    .read_output::<AcliOutput>(&mut session)?
    .and_then(|o| o.commit_message)
    .unwrap_or_else(|| options.default_message.clone());
```

---

## Phase 4 — Run full test suite, confirm GREEN

Ask the user to run the full test suite and share the output:
> Please run `cargo test` and share the output.

All existing tests + new unit/integration tests must pass, including the Phase 00 regression tests.

---

## File Change Summary

| File | Change |
|------|--------|
| `src/utils/ai.rs` | **New** — trait, impl, types, unit tests |
| `src/utils/mod.rs` | Add `pub mod ai;` |
| `src/commands/fixup.rs` | Remove 3 inline AI fns + 2 `include_bytes!` consts; use `AcliRovodev`; simplify `FixupOptions` |
| `tests/integration_tests.rs` | Add 2 integration tests verifying the `fixup` command still works after refactor |
