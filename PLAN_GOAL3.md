# Goal 3: Async-First Pattern in Generated Scripts — Detailed Implementation Plan

## Overview

Goal 3 has no new Rust source files. Its deliverable is a **refined `ADD_META_PROMPT` constant** in `src/commands/add.rs` that reliably instructs the AI to emit every AI-dependent value as a backgrounded `acli rovodev` call at the very top of generated scripts, before any deterministic work. Unit tests assert the structural properties of the prompt; a golden-file integration test validates the async-first shape of a sample generated script.

---

## TDD Sequence — Red → Green

The plan follows strict TDD: **write tests first, confirm they fail, then implement**.

> **⚠️ Cargo command convention:** The implementor (AI) **does not run `cargo` commands directly**. Instead, at each step where a build or test run is needed, the implementor provides the exact command to run and asks the user to run it and share the output. The user is the one who executes all `cargo` invocations.

---

## Phase 00 — Regression-net: establish baseline before touching `ADD_META_PROMPT`

> **⚠️ Cargo convention:** The AI does **not** run `cargo` commands. Provide the exact command and ask the user to run it and share output.

**Goal:** Confirm the full existing test suite (from Goals 1 and 2) is green before any changes are made. This gives a clean baseline to diff against after each phase.

---

### Concrete steps for Phase 00

1. **Ask the user to run the existing test suite:**
   > Please run `cargo test` and share the output.

   All existing tests must be green before we proceed.

2. **Record the current meta-prompt** to confirm it does not yet satisfy all Goal 3 assertions:
   > Please run `cargo test test_meta_prompt` and share the output — these tests should be RED at this point.

---

## Phase 0 — Write tests FIRST, confirm RED

> **⚠️ Cargo convention:** The AI does **not** run `cargo` commands. Provide the exact command and ask the user to run it and share output.

Write all tests before modifying `ADD_META_PROMPT`. After writing the tests, ask the user to confirm they fail:

> Please run `cargo test` and share the output — the new Goal 3 tests should be RED (compile error or test failure) at this point.

---

### Unit tests to add in `src/commands/add.rs` (inside `#[cfg(test)]`)

#### 1. `test_meta_prompt_contains_json_fields`

```rust
#[test]
fn test_meta_prompt_contains_json_fields() {
    // The AI must return structured JSON with exactly these three fields.
    assert!(ADD_META_PROMPT.contains("\"filename\"") || ADD_META_PROMPT.contains("filename"));
    assert!(ADD_META_PROMPT.contains("\"description\"") || ADD_META_PROMPT.contains("description"));
    assert!(ADD_META_PROMPT.contains("\"script\"") || ADD_META_PROMPT.contains("script"));
}
```

**Why:** Guards the JSON contract between `ADD_META_PROMPT` and the `AddOutput` struct. If the prompt drifts from the struct field names, the AI will return unparseable output. This test makes that class of error immediately visible.

---

#### 2. `test_meta_prompt_enforces_background_calls`

```rust
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
```

**Why:** The central requirement of Goal 3. Ensures the meta-prompt explicitly tells the AI to fire AI sub-calls as background processes, not blocking sequential calls.

---

#### 3. `test_meta_prompt_enforces_pid_capture`

```rust
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
```

**Why:** Backgrounding without PID capture leads to race conditions. The generated script must `wait $PID` before reading each AI output file. This test pins that requirement in the prompt.

---

#### 4. `test_meta_prompt_enforces_wait_before_read`

```rust
#[test]
fn test_meta_prompt_enforces_wait_before_read() {
    // The prompt must explicitly instruct reading AI output only AFTER waiting.
    let prompt_lower = ADD_META_PROMPT.to_lowercase();
    let has_wait = prompt_lower.contains("wait")
        && (prompt_lower.contains("read") || prompt_lower.contains("consume") || prompt_lower.contains("output file"));
    assert!(has_wait, "ADD_META_PROMPT must instruct the AI to wait before reading AI output");
}
```

**Why:** Enforces the "join" step of the async-first pattern — reads happen only after the corresponding AI call has completed.

---

#### 5. `test_meta_prompt_enforces_cleanup`

```rust
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
```

**Why:** Without cleanup instructions the generated script will leak temp JSON files in `/tmp`. This is the equivalent of `AiSession`'s RAII cleanup, but inside the generated script.

---

#### 6. `test_meta_prompt_lang_placeholder`

```rust
#[test]
fn test_meta_prompt_lang_placeholder() {
    // {LANG} must be present so build_add_prompt() can substitute the target language.
    assert!(
        ADD_META_PROMPT.contains("{LANG}"),
        "ADD_META_PROMPT must contain the {{LANG}} placeholder"
    );
}
```

**Why:** If `{LANG}` is accidentally removed from the prompt, `build_add_prompt()` silently produces a prompt without a language instruction. The test makes this a hard failure.

---

#### 7. `test_meta_prompt_prompt_placeholder`

```rust
#[test]
fn test_meta_prompt_prompt_placeholder() {
    // {PROMPT} must be present so build_add_prompt() can inject user requirements.
    assert!(
        ADD_META_PROMPT.contains("{PROMPT}"),
        "ADD_META_PROMPT must contain the {{PROMPT}} placeholder"
    );
}
```

**Why:** Same rationale as `{LANG}` — the user's requirements must reach the AI, and this test prevents silent placeholder removal during future edits.

---

#### 8. `test_build_add_prompt_substitutes_lang`

```rust
#[test]
fn test_build_add_prompt_substitutes_lang() {
    let result = build_add_prompt("list all files", &Lang::Bash);
    assert!(!result.contains("{LANG}"), "{{LANG}} placeholder must be replaced");
    assert!(result.to_lowercase().contains("bash"));
}
```

**Why:** Confirms the `build_add_prompt()` helper correctly substitutes the language. Companion to the placeholder presence tests above.

---

#### 9. `test_build_add_prompt_substitutes_prompt`

```rust
#[test]
fn test_build_add_prompt_substitutes_prompt() {
    let result = build_add_prompt("do the thing", &Lang::Bash);
    assert!(!result.contains("{PROMPT}"), "{{PROMPT}} placeholder must be replaced");
    assert!(result.contains("do the thing"));
}
```

**Why:** Confirms the user's requirements are actually embedded in the final prompt sent to the AI.

---

#### 10. `test_build_add_prompt_js_language`

```rust
#[test]
fn test_build_add_prompt_js_language() {
    let result = build_add_prompt("list all files", &Lang::Js);
    assert!(!result.contains("{LANG}"), "{{LANG}} placeholder must be replaced");
    let lower = result.to_lowercase();
    assert!(lower.contains("javascript") || lower.contains("js"));
}
```

**Why:** Ensures the `Lang::Js` variant is represented correctly in the final prompt — `Lang::Js` must map to `"JavaScript"` or similar, not the bare enum name.

---

### Integration test to add to `tests/integration_tests.rs`

#### `test_add_meta_prompt_async_pattern_example`

```rust
#[test]
fn test_add_meta_prompt_async_pattern_example() {
    // Verify that the example script shape from PLAN.md is consistent with what
    // the prompt would instruct — specifically that the prompt mentions temp
    // output file, background execution, and PID waiting.
    // This is a structural / documentation-level test: it reads ADD_META_PROMPT
    // via the public constant and asserts the async-first invariants hold.
    use dt::commands::add::ADD_META_PROMPT;

    let lower = ADD_META_PROMPT.to_lowercase();
    assert!(lower.contains("background") || lower.contains("backgrounded") || lower.contains(" &"),
        "Prompt must mention backgrounded calls");
    assert!(lower.contains("pid") || lower.contains("$!"),
        "Prompt must mention PID capture");
    assert!(lower.contains("wait"),
        "Prompt must mention waiting on background calls");
    assert!(lower.contains("filename") && lower.contains("description") && lower.contains("script"),
        "Prompt must specify the three required JSON fields");
}
```

**Why:** This is the single cross-boundary test that verifies the public `ADD_META_PROMPT` constant (visible outside the crate via `pub`) satisfies all async-first properties. It acts as the integration-level contract test for Goal 3.

> **Note:** For this test to compile, `ADD_META_PROMPT` must be changed from `const` to `pub const` in `add.rs`.

---

## Phase 1 — Update `ADD_META_PROMPT` in `src/commands/add.rs`

> **⚠️ Cargo convention:** The AI does **not** run `cargo` commands. Provide the exact command and ask the user to run it and share output.

Replace the existing `ADD_META_PROMPT` constant with the full async-first version. The key additions over the Goal 2 draft are:

- Explicit rule to fire **all** AI sub-calls as backgrounded processes (`&`) at the **very top** of the script, before any deterministic work
- Explicit rule to capture each PID immediately (`AI_FOO_PID=$!`)
- Explicit rule to `wait $AI_FOO_PID` **only at the step that actually consumes the value**, so deterministic work can proceed concurrently
- Explicit rule to `rm -f <output_file>` after reading each AI result
- Clear ordering constraint: background AI calls → deterministic work (in parallel) → join + read → cleanup

### Updated `ADD_META_PROMPT`

```rust
pub const ADD_META_PROMPT: &str = r#"
You are a script generator. Given the user's requirements below, produce a
shell script (or JavaScript script if instructed) that fulfils them.

Async-first rules (MANDATORY — do not omit):
1. Identify EVERY value in the script that requires an AI response (any call
   to `acli rovodev`).
2. Emit ALL of those as backgrounded `acli rovodev` calls at the very top of
   the script — before any other work. Capture each PID immediately:
       acli rovodev "<prompt>" \
         --output-file=/tmp/dt_ai_<name>.json \
         --output-schema=~/.dt/resources/schema.json \
         --config-file=~/.dt/resources/config.ag-stack.yml &
       AI_<NAME>_PID=$!
3. Proceed with ALL deterministic steps concurrently while the AI calls run
   in the background.
4. Read each AI output file ONLY at the step that actually needs the value —
   and not before. Always wait on the PID first:
       wait $AI_<NAME>_PID
       RESULT=$(jq -r '.value' /tmp/dt_ai_<name>.json)
       rm -f /tmp/dt_ai_<name>.json
5. Choose a short, descriptive snake_case filename with the correct extension
   (.sh for Bash, .js for JavaScript).
6. Return ONLY valid JSON in this exact shape — no markdown, no prose:
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

### Visibility change

Change `const ADD_META_PROMPT` → `pub const ADD_META_PROMPT` so the integration test in `tests/integration_tests.rs` can reference it directly without re-exporting through a wrapper function.

---

## Phase 2 — Confirm tests are GREEN

> **⚠️ Cargo convention:** The AI does **not** run `cargo` commands. Provide the exact command and ask the user to run it and share output.

Ask the user to run the full test suite:

> Please run `cargo test` and share the output.

All existing tests (Goals 1 and 2) plus all new Goal 3 unit and integration tests must pass green.

Then verify the help text and flags are unchanged:

> Please run `dt add --help` and confirm `-m`/`--message` and `--lang` are still present.

---

## Key Design Decisions

| Decision | Rationale |
|---|---|
| Goal 3 is prompt-only — no new Rust files | The async-first pattern is a generation-time constraint enforced by the meta-prompt, not a runtime Rust mechanism. Keeping it in the prompt keeps the generated scripts self-contained. |
| `pub const ADD_META_PROMPT` | Makes the constant testable from `tests/integration_tests.rs` without an extra accessor. The prompt is not sensitive — exposing it publicly has no downside. |
| Explicit numbered rules in the prompt | LLMs follow numbered rule lists more reliably than prose paragraphs. Each rule maps to one invariant in the tests, making the traceability obvious. |
| PID variable naming convention (`AI_<NAME>_PID`) | Gives generated scripts a consistent, grep-able pattern. Reduces chance of variable name collisions when multiple AI calls are backgrounded. |
| `rm -f` cleanup instruction in the prompt | Mirrors `AiSession`'s RAII cleanup at the Rust level, but inside the generated script. Without it, `/tmp` accumulates stale JSON files on every script run. |
| Temp file path pattern `/tmp/dt_ai_<name>.json` | Namespaced under `dt_ai_` to avoid collisions with other tools. Short enough to be readable in the generated script. |
| No new crate dependencies | Goal 3 adds only a refined string constant and unit tests — zero new `Cargo.toml` entries required. |

---

## TODOs (future, not in scope for this goal)

> **TODO (future):** Add a `meta_prompt` key to `.dt-add-config` (or `~/.dt/config`) so power users can override the generation instructions entirely without recompiling.

> **TODO (future):** Support JavaScript-specific async pattern (e.g. `Promise.all([ ... ])` instead of `&` + `wait`) when `--lang js` is used. Today the rules are written for Bash.

> **TODO (future):** Add a golden-file test that captures a real `acli rovodev` response and asserts the generated script contains at least one `&` backgrounded call and a corresponding `wait`.

> **TODO (future):** Lint generated scripts for the async-first pattern automatically post-generation (e.g. parse for `&` + `wait` pairs) and warn if none are found — indicating the AI ignored the instructions.

---

## File Change Summary

| File | Change |
|------|--------|
| `src/commands/add.rs` | Update `ADD_META_PROMPT`: add explicit async-first numbered rules, PID capture/wait/cleanup instructions; change `const` → `pub const` |
| `tests/integration_tests.rs` | Add `test_add_meta_prompt_async_pattern_example` integration test |
