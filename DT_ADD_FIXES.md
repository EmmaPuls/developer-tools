# DT_ADD_FIXES — Plan to Fix `dt add`

## Context

When `dt add` generates a script (e.g. `smart_commit`), the generated script's
calls to `acli rovodev` use incorrect paths and an incorrect schema.  
This plan documents every issue and the concrete steps to fix them.

---

## Problem Summary

When `smart_commit.sh` was generated, lines 54–56 read:

```bash
  --output-file=/tmp/dt_ai_commit_msg.json \
  --output-schema="$HOME/.dt/resources/acli_add_schema.json" \
  --config-file="$HOME/.dt/resources/config.ag-stack.yml" &
```

They should read:

```bash
  --output-file=$HOME/.dt/scripts/smart_commit/output/tmp_dt_ai_commit_msg.json \
  --output-schema="$HOME/.dt/scripts/smart_commit/resources/acli_add_schema.json" \
  --config-file="$HOME/.dt/scripts/smart_commit/resources/config.ag-stack.yml" &
```

---

## Root Causes & Issues

### Issue 1 — AI is left to invent `acli rovodev` call paths

**Where:** `ADD_META_PROMPT` in `src/commands/add.rs` (the example in the prompt).  
**Problem:** The prompt shows `/tmp/dt_ai_<name>.json` and
`$(dirname "$0")/resources/...` as example paths. The AI substitutes these
literally (or worse, uses `$HOME/.dt/resources/...` which is the global shared
location). The AI should never decide these paths — `dt add` must inject them
as concrete, pre-computed strings into the prompt.  
**Fix:** Before sending the prompt to the AI, `dt add` must know the final
`script_stem` (either ask the AI for it in a separate first call, or derive it
from the prompt). Then it replaces the path placeholders in the prompt with the
actual resolved paths:
- `--output-file=$HOME/.dt/scripts/<stem>/output/tmp_dt_ai_<name>.json`
- `--output-schema="$HOME/.dt/scripts/<stem>/resources/acli_add_schema.json"`
- `--config-file="$HOME/.dt/scripts/<stem>/resources/config.ag-stack.yml"`

### Issue 2 — `output/` subdirectory never created

**Where:** `execute()` in `src/commands/add.rs`.  
**Problem:** The per-script folder structure is:
```
~/.dt/scripts/<stem>/
  <script>         ← the script itself
  meta.json
  resources/       ← created ✓
  output/          ← NEVER created ✗
```
Any `acli rovodev` call that writes to `output/` will fail at runtime because
the directory does not exist.  
**Fix:** Add `fs::create_dir_all(script_folder.join("output"))` alongside the
existing `resources/` creation.

### Issue 3 — AI uses the `dt add` schema instead of a task-specific schema

**Where:** `ADD_META_PROMPT` and `execute()`.  
**Problem:** The resources folder gets `acli_add_schema.json` copied into it —
which is the schema for `dt add`'s own output (`filename`, `description`,
`script`). A generated script like `smart_commit` whose AI calls return a
`commit_message` field needs a **different, task-specific schema**.  
**Fix:** `dt add` must ask the AI to produce (as part of its JSON output) an
`output_schema` field containing the JSON Schema that the generated script's
own `acli rovodev` calls should use. `dt add` then:
1. Create a template schema to reuse for this purpose
2. Copy the template schema to `~/.dt/scripts/<stem>/resources/acli_add_schema.json` and ask the AI to modify it for this specific task
3. Update the template schema with the results from the AI

### Issue 4 — `--output-schema` points to the wrong schema name

**Where:** `ADD_META_PROMPT`.  
**Problem:** Even if paths were correct, the prompt example references
`acli_add_schema.json` for the generated script's own AI calls. The generated
script should reference a schema that matches the data it expects back — not
the `dt add` schema.  
**Fix:** The schema written to `resources/` for the generated script should be
named after the task (e.g. `acli_add_schema.json` is fine as a filename
convention) but its **contents** must be the task-specific schema from Issue 3.

### Issue 5 — Correct paths must be injected into the prompt, not left to the AI

**Where:** `ADD_META_PROMPT` and `build_add_prompt()` in `src/commands/add.rs`.  
**Problem:** `build_add_prompt()` only substitutes `{LANG}` and `{PROMPT}`. The
path placeholders in the example `acli rovodev` call are plain prose that the
AI may or may not follow.  
**Fix:** Add additional template placeholders to `ADD_META_PROMPT`:
- `{SCRIPT_STEM}` — the snake_case name `dt add` derives for this script
- `{SCRIPTS_DIR}` — the absolute path to `~/.dt/scripts`

Then `build_add_prompt()` substitutes these so the prompt contains the **exact
literal paths** the generated script must use. Because `dt add` constructs
these strings in Rust and injects them, the AI cannot deviate from them.

### Issue 6 — `meta.json` missing `original_prompt` field

**Where:** `ScriptMeta` serialization in `execute()`.  
**Problem:** `ScriptMeta` has both `description` and `original_prompt` fields,
but the `meta.json` written to disk for `smart_commit` only contains
`description`. The `original_prompt` key is absent.  
**Fix:** Verify the `ScriptMeta` struct derives `Serialize` correctly and that
both fields are populated before `serde_json::to_string_pretty`. (Likely a
regression where `original_prompt` was added to the struct but never wired up
in `execute()`.)

---

## Proposed Folder Structure (per generated script)

```
~/.dt/scripts/<stem>/
  <stem>.sh (or .js)        ← executable script
  meta.json                 ← { description, original_prompt }
  resources/
    acli_add_schema.json    ← TASK-SPECIFIC schema (AI-generated, not dt-add's schema)
    config.ag-stack.yml     ← copied from global or embedded template
  output/                   ← empty dir; acli rovodev writes tmp files here
```

---

## Implementation Plan

### Step 1 — Two-phase AI call: name first, then script

To know the `script_stem` before generating the script (so we can inject real
paths into the prompt), split the AI interaction into two phases:

**Phase A — Name + schema generation** (new, small call):  
Send a lightweight prompt: *"Given these requirements, what should the
snake_case filename stem be, and what JSON Schema should the script's own AI
calls use?"*  
Receive: `{ "stem": "smart_commit", "output_schema": { ... } }`

**Phase B — Script generation** (existing call, enhanced):  
Now that we know `stem`, inject the correct literal paths into the meta-prompt
before sending it. The AI produces `{ "filename", "description", "script" }`.

> Alternative (simpler): derive `stem` heuristically from the user's prompt
> (e.g. first noun phrase, snake_cased), and ask the AI for only the
> `output_schema` in Phase A. This avoids a round-trip for just the name.

### Step 2 — Update `ADD_META_PROMPT`

Replace the hard-coded example paths with injected values:

```
acli rovodev "<prompt>" \
  --output-file=$HOME/.dt/scripts/{SCRIPT_STEM}/output/tmp_dt_ai_<name>.json \
  --output-schema="$HOME/.dt/scripts/{SCRIPT_STEM}/resources/acli_add_schema.json" \
  --config-file="$HOME/.dt/scripts/{SCRIPT_STEM}/resources/config.ag-stack.yml" &
```

Add `{SCRIPT_STEM}` substitution to `build_add_prompt()`.

### Step 3 — Update `build_add_prompt()` signature

```rust
pub fn build_add_prompt(user_requirements: &str, lang: &Lang, script_stem: &str) -> String {
    ADD_META_PROMPT
        .replace("{LANG}", lang.display_name())
        .replace("{PROMPT}", user_requirements)
        .replace("{SCRIPT_STEM}", script_stem)
}
```

### Step 4 — Update `execute()` to create `output/` directory

```rust
let output_folder = script_folder.join("output");
fs::create_dir_all(&output_folder)?;
```

This must happen before the script is written (or at minimum before it runs).

### Step 5 — Update `AddOutput` and `acli_add_schema.json` for `output_schema`

Add `output_schema` field to `AddOutput`:

```rust
#[derive(Deserialize)]
pub struct AddOutput {
    pub filename:      Option<String>,
    pub description:   Option<String>,
    pub script:        Option<String>,
    pub output_schema: Option<serde_json::Value>,  // NEW
}
```

Update `src/resources/acli_add_schema.json` to include `output_schema` as a
required field:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "type": "object",
  "properties": {
    "filename":      { "type": "string" },
    "description":   { "type": "string" },
    "script":        { "type": "string" },
    "output_schema": {
      "type": "object",
      "description": "JSON Schema for the output of acli rovodev calls made within the generated script."
    }
  },
  "required": ["filename", "description", "script", "output_schema"]
}
```

### Step 6 — Write task-specific schema to `resources/`

In `execute()`, after parsing the AI output, replace:

```rust
fs::write(&schema_dest, AcliRovodev::add_schema_bytes())?;
```

With:

```rust
let task_schema_bytes = if let Some(ref schema_val) = output.output_schema {
    serde_json::to_vec_pretty(schema_val)?
} else {
    // Fallback: generic single-value schema
    include_bytes!("../resources/acli_schema.json").to_vec()
};
fs::write(&schema_dest, &task_schema_bytes)?;
```

### Step 7 — Fix `meta.json` to include `original_prompt`

Ensure `execute()` passes `original_prompt` when constructing `ScriptMeta`:

```rust
let meta = ScriptMeta {
    description: description.clone(),
    original_prompt: user_requirements.clone(),  // must be wired up
};
```

Verify `ScriptMeta` has `#[derive(Serialize)]` and both fields present.

### Step 8 — Update the prompt to instruct the AI about `output_schema`

In `ADD_META_PROMPT`, add a rule explaining that the AI must:
- Identify what structured data its own `acli rovodev` calls will return.
- Emit a `output_schema` field in its JSON response containing a valid JSON
  Schema describing that data.

Example addition to the prompt:
```
7. Determine what structured data each `acli rovodev` call in the script will
   return and define a `output_schema` JSON Schema for it.
   Include that schema verbatim in your JSON response under the key
   "output_schema". This schema will be written to the script's resources/
   folder and passed via --output-schema to every acli rovodev call.
```

### Step 9 — Update tests

- Update `test_build_add_prompt_*` tests to pass the new `script_stem`
  argument.
- Add a test asserting the injected paths appear literally in the built prompt.
- Add a test for the `output/` directory creation.
- Add a test that `meta.json` contains `original_prompt`.
- Add a test that `acli_add_schema.json` includes `output_schema`.

---

## Files to Change

| File | Change |
|------|--------|
| `src/commands/add.rs` | `ADD_META_PROMPT` — inject `{SCRIPT_STEM}` placeholder; `build_add_prompt()` — add `script_stem` param; `AddOutput` — add `output_schema` field; `execute()` — two-phase AI call, create `output/` dir, write task-specific schema, fix `meta.json` |
| `src/resources/acli_add_schema.json` | Add `output_schema` property and required field |
| `src/utils/ai.rs` | No changes required (paths come from caller) |
| Tests in `add.rs` | Update `build_add_prompt` call sites; add new assertions |

---

## What `dt add` Controls vs. What the AI Controls

| Concern | Controlled by |
|---------|--------------|
| `--output-file` path | **`dt add`** (injected into prompt as literal string) |
| `--output-schema` path | **`dt add`** (injected into prompt as literal string) |
| `--config-file` path | **`dt add`** (injected into prompt as literal string) |
| Schema *contents* for task output | **AI** (returned as `output_schema` in JSON) |
| Script logic / filename / description | **AI** |
| Folder + file creation on disk | **`dt add`** |
