# `dt` — Remaining Issues & Known Gaps

A living document of known problems, design gaps, and areas needing improvement in the `dt` CLI.

---

## 1. Script Folder Structure

**Issue:** Scripts are stored flat inside `~/.dt/scripts/` — a single `.sh` / `.js` file alongside a `<name>.meta.json` sidecar. There is no isolation between scripts.

**Problem in practice:**
- Scripts that generate their own temporary files or resources have nowhere clean to put them.
- Config files, helper assets, and the sidecar metadata get mixed together in one flat directory.
- Renaming or deleting a script requires knowing to also remove the sidecar manually.

**Desired design:**
Each script should live in its own subdirectory:
```
~/.dt/scripts/
  my_commit_script/
    my_commit_script.sh      ← the script itself
    meta.json                ← description / metadata
    resources/               ← any script-specific config, schemas, etc.
```

**Files affected:** `src/commands/add.rs` (`resolve_scripts_dir`, save logic), `src/commands/run.rs` (`discover_scripts`, `resolve_script`).

---

## 2. Missing `list`, `delete`, and `edit` Subcommands

**Issue:** There is no way to manage scripts after they have been generated. `dt run` can execute a script interactively, but users cannot:

- **List** all saved scripts with their descriptions without launching the run UI.
- **Delete** a script (and its associated folder/sidecar) cleanly.
- **Edit** the source of a generated script in the user's preferred editor.

**Desired subcommands:**
```
dt list              ← tabular view of all scripts + descriptions
dt delete <name>     ← removes the script folder (with confirmation)
dt edit <name>       ← opens the script in $VISUAL / $EDITOR
```

**Notes:**
- `PLAN_LIST.md` contains a draft implementation plan for `dt list` — this plan still needs to be implemented.
- `delete` and `edit` have no plan yet.
- With the per-script folder structure (issue 1), `delete` becomes straightforward: `rm -rf ~/.dt/scripts/<name>/`.

---

## 3. `VERBOSE_DT` / `DT_VERBOSE` Variable — Inconsistency & Enforcement

**Issue:** The meta-prompt (`ADD_META_PROMPT` in `src/commands/add.rs`) instructs the AI to gate verbose logging on `DT_VERBOSE`, and `dt run --verbose` injects `DT_VERBOSE=1` into the child environment. However:

- The variable name in the meta-prompt (`DT_VERBOSE`) and the constant `DT_VERBOSE_ENV` in `run.rs` may drift if either is changed independently.
- There is no validation that a generated script actually contains the `dt_log` function — the AI can silently omit it.
- The user-facing flag is `--verbose` on `dt run`, but there is no equivalent way to pass it when running a script directly (outside of `dt run`).

**Desired improvements:**
- Define a single shared constant (e.g. `pub const DT_VERBOSE_ENV: &str = "DT_VERBOSE"`) in a central location and reference it from both `add.rs` (meta-prompt interpolation) and `run.rs`.
- Add a post-generation check: after the AI returns the script, scan the script body for `dt_log` (or `DT_VERBOSE`) and warn the user if it is absent.
- Document the variable in the README so users know they can also run scripts manually with `DT_VERBOSE=1 ~/.dt/scripts/my_script/my_script.sh`.

---

## 4. Template Script for Common Features

**Issue:** Every generated script currently reimplements the same boilerplate from scratch based solely on the meta-prompt instructions:
- The `dt_log` verbose logging utility
- The async-first `acli rovodev` call pattern (background call → PID capture → wait → read → cleanup)
- Common error handling (`set -euo pipefail` for Bash)

If the AI deviates from the meta-prompt (which it occasionally does), the generated script may be missing these foundations entirely.

**Desired improvement:**
Maintain a template script for each supported language (e.g. `templates/userscript-template.sh`, `templates/userscript-template.js`) that the `dt add` flow either:
1. Prepends to the generated script as a header, **or**
2. References explicitly in the meta-prompt as "start from this skeleton".

A `templates/userscript-template.js` already exists in the repo — it should be wired into the generation flow.

---

## 5. `ADD_META_PROMPT` Should Be an Editable File, Not a Rust Constant

**Issue:** The meta-prompt is defined as `pub const ADD_META_PROMPT: &str = r#"..."#` in `src/commands/add.rs`. This means:
- Users cannot adjust generation behaviour without recompiling the binary.
- The prompt is not visible or discoverable outside of reading source code.
- Changing the prompt requires a code change + PR + release.

**Desired design:**
- Ship a default prompt file at `~/.dt/prompts/add_meta_prompt.md` (written on first run if absent, similar to how `config.ag-stack.yml` is handled).
- At runtime, read the file and use its contents as the prompt; fall back to the embedded constant if the file is missing.
- This allows power users to tune generation without recompiling, and makes the prompt visible and self-documenting.

**Related:** The same pattern should apply to any other large string constants used as AI instructions.

---

## 6. AI-Assisted Permission / Config Generation

**Issue:** Generated scripts that use `acli rovodev` need two resource files:
- `~/.dt/resources/acli_add_schema.json` — the output JSON schema
- `~/.dt/resources/config.ag-stack.yml` — the Rovo agent-stack config (defines which tools/permissions the agent has)

Currently these are shared across all scripts. A script that only needs to read Jira tickets gets the same permission set as one that needs to write to Confluence. There is also no mechanism to ask Rovo what permissions are available and which ones the new script is likely to need.

**Desired improvement:**
- During `dt add`, after capturing the user's requirements but before generating the script, make a lightweight AI call to analyse the requirements and determine:
  - Which `acli` tool categories the script will likely use.
  - What permission scopes those require.
- Use that analysis to generate a per-script `config.ag-stack.yml` (stored in the script's own `resources/` folder per issue 1) with only the necessary permissions.
- This also acts as a confirmation step (see issue 7): "This script will need access to Jira (read) and Confluence (write). Continue?"

---

## 7. Insufficient Confirmation / Progress Feedback Steps

**Issue:** The `dt add` flow goes from "capturing the prompt" straight to "running the AI" with no intermediate feedback to the user. The user does not know:
- What the tool is about to do.
- What permissions / network calls will be made.
- How long to expect to wait.
- What was actually generated before it is saved.

**Desired confirmation steps:**
1. **Before the AI call:** Print a summary of what is being sent and ask for confirmation (or at least print a "Generating script…" spinner).
2. **After generation, before saving:** Show the generated script filename + description and ask the user to confirm before writing to disk.
3. **Permission scope review** (ties into issue 6): Show the inferred permission set and ask the user to approve.
4. **After save:** Print the full path of the saved script and remind the user how to run / edit / delete it.

---

## 8. Unreliable Data Passing Into Generated Scripts

**Issue:** Scripts generated by `dt add` that call `acli rovodev` and attempt to extract a value from the JSON output (e.g. a commit message) intermittently return `null` or an empty string. This has been observed with the commit-message generation script.

**Root causes (suspected):**
- The AI output JSON may not match the expected schema shape (`.value` vs a different key).
- `jq -r '.value'` silently returns `null` when the key is absent rather than failing.
- The schema (`acli_add_schema.json`) may not be constraining the output format tightly enough for the inner AI calls made inside the generated script.
- Race conditions: the script may be reading the output file before `acli rovodev` has fully flushed it, despite the `wait $PID` guard.

**Desired improvements:**
- Update the meta-prompt to instruct the AI to validate the output file is non-empty and contains the expected key before proceeding, and to exit with an error if not.
- Add a `jq` existence check in the generated script template:
  ```bash
  RESULT=$(echo "$AI_RAW" | jq -r '.value // empty')
  [ -z "$RESULT" ] && { echo "Error: AI returned no value" >&2; exit 1; }
  ```
- Investigate whether the schema file needs a stricter definition (e.g. `required: ["value"]`) to force the AI to always include the field.
- Consider adding a retry loop (up to N attempts) around the AI call inside generated scripts for transient failures.

---

## Summary Table

| # | Issue | Severity | Area |
|---|-------|----------|------|
| 1 | Flat script folder structure — no per-script isolation | Medium | `add.rs`, `run.rs` |
| 2 | No `list`, `delete`, or `edit` subcommands | High | New commands needed |
| 3 | `DT_VERBOSE` inconsistency & no enforcement in generated scripts | Medium | `add.rs`, `run.rs` |
| 4 | No template script for common boilerplate | Medium | `templates/`, `add.rs` |
| 5 | `ADD_META_PROMPT` is a hard-coded constant, not user-editable | Low | `add.rs` |
| 6 | No AI-assisted permission / config generation per script | Medium | `add.rs`, `ai.rs` |
| 7 | Too few confirmation steps before and after generation | Medium | `add.rs` |
| 8 | Generated scripts return `null` when reading AI output | High | Meta-prompt, schema |
