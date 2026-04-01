# `dt` — Implementation Plan

## Goal 1: AI Provider Abstraction (`src/utils/ai.rs`)

Think of this as an **interface + concrete implementation** layer. Today the only provider is
`acli rovodev`, but the design anticipates alternatives (e.g. raw OpenAI, Claude CLI, etc.).

### Core Types

```
AiProvider (trait)
  └─ AcliRovodev  ← concrete impl today

AiCallSpec { prompt, output_schema_fields, output_path }
AiPaths    { output_file, schema_file, config_file }
AiSession  ← RAII guard returned by provider; drops/cleans up temp files on close
```

### Responsibilities

- **`AiProvider::setup_resources(dir: &Path) -> Result<AiPaths>`**
  Writes the schema + config files the provider needs to disk (idempotent). Returns the resolved
  paths.

- **`AiProvider::build_command(spec: &AiCallSpec, paths: &AiPaths) -> Command`**
  Constructs the `std::process::Command` ready to spawn (but not yet started). Callers can choose
  to `.spawn()` (async/background) or `.status()` (blocking) depending on context.

- **`AiSession` (RAII cleanup guard)**
  Returned alongside the command. Wraps the output file path. When dropped (or when
  `.cleanup()` is called explicitly), it deletes the output file and any other temp artifacts
  created for that call. This ensures scripts and the CLI never leak temp files even on early
  exit or error.

- **`AiProvider::read_output<T: DeserializeOwned>(session: &AiSession) -> Result<Option<T>>`**
  Reads + parses the JSON output file. Consuming the session (or calling cleanup after) removes
  the file.

### Refactor

`fixup.rs` is updated to use this abstraction instead of its inline `setup_fixup_resources`,
`request_commit_message`, and `extract_commit_message` functions.

> **TODO (future):** Add a config key (e.g. `AI_PROVIDER=acli_rovodev`) to `.dt-fixup-config`
> and `.dt-add-config` so users can swap providers without recompiling.

---

## Goal 2: `dt add` — AI Script Generator

### Invocation Modes

- **Inline:** `dt add -m "copy all .md files from ~/notes to ~/backup and summarize"`
- **Editor:** `dt add` (no args) — opens a temporary `.md` file in the user's `$VISUAL` /
  `$EDITOR`, falling back to `core.editor` from `git config`. The file is pre-populated with
  comment-style instructions explaining what to write, plus a blank template section for the
  user's requirements. After the user saves and closes, `dt` reads the file, strips comment
  lines, and uses the remaining body as the prompt.

### Workflow After Prompt is Captured

1. Set up AI resources via `AiProvider::setup_resources()`
2. Build + run the `acli rovodev` command with the **meta-prompt** (user requirements wrapped in
   generation instructions) — blocking call since we need the script before we can save it
3. Parse structured output: `{ "filename": "...", "description": "...", "script": "..." }`
4. Save script to `~/.dt/scripts/<filename>`, `chmod +x`
5. Write a sidecar `<filename>.meta.json` alongside it containing `{ "description": "..." }` for
   use by `dt run`'s listing UI
6. `AiSession` cleanup removes the temp output file
7. Print success with the saved path

### Script Output Location

`~/.dt/scripts/` stored as a `DEFAULT_SCRIPTS_DIR` constant.

> **TODO (future):** Add a `scripts_dir` key to `~/.dt/config` (shared dt config file) so users
> can override the save location.

> **TODO (future):** Add an `editor` key to `~/.dt/config` so users can override the editor
> without relying on `$VISUAL` / `$EDITOR` / git config.

### Meta-Prompt

Defined as a `const ADD_META_PROMPT` in `add.rs`. It instructs the AI to:

- Produce a script in the target language (default: Bash; flag: `--lang bash|js`)
- Follow the **async-first AI call pattern** for any AI-needed values within the script (see
  Goal 3)
- Return structured JSON: `{ "filename": "...", "description": "...", "script": "..." }`

> **TODO (future):** Expose `meta_prompt` as a configurable key in `.dt-add-config` so power
> users can override the generation instructions entirely.

---

## Goal 3: Async-First Pattern in Generated Scripts

Enforced via the meta-prompt rather than Rust code. The constraint embedded in `ADD_META_PROMPT`:

> "Identify every value in the script that requires an AI response. Emit all of those as
> backgrounded `acli rovodev` calls at the very top of the script — before any other work.
> Proceed with deterministic steps concurrently. Read from each AI output file only at the step
> that actually consumes the value."

### Example Generated Bash Script

```bash
# [AI calls — fired immediately in background]
acli rovodev "Summarize these files: $(ls ~/notes/*.md)" \
  --output-file=/tmp/dt_ai_summary.json \
  --output-schema=~/.dt/resources/schema.json \
  --config-file=~/.dt/resources/config.ag-stack.yml &
AI_SUMMARY_PID=$!

# [Deterministic work — runs in parallel]
mkdir -p ~/backup
cp ~/notes/*.md ~/backup/

# [Join — read AI result only when needed]
wait $AI_SUMMARY_PID
SUMMARY=$(jq -r '.value' /tmp/dt_ai_summary.json)
rm -f /tmp/dt_ai_summary.json   # cleanup temp file
echo "$SUMMARY"
```

> **TODO (future):** Expose `meta_prompt` as a configurable key in `.dt-add-config` so power
> users can tune the async-first generation constraint.

---

## Additional Goal: `dt run <script-name>`

### Invocation

```
dt run <script-name>
dt run              ← no args: shows interactive fuzzy list
```

### Tab Completion + Fuzzy List

- Add `clap_complete` crate; generate completions that enumerate script names from
  `~/.dt/scripts/` at shell completion time
- When called with no args (or no exact match), show an interactive fuzzy-searchable list via
  `dialoguer::FuzzySelect` — same pattern already used in `fixup.rs` for commit selection
- List displays `<filename>  —  <description>` where description is read from the sidecar
  `<filename>.meta.json` written by `dt add`

### Execution

Runs the script via `std::process::Command`, inheriting stdio so it feels like a native command.

> **TODO (future):** Support passing arguments through to the script:
> `dt run script-name -- arg1 arg2`

---

## Cargo Command Convention

> **⚠️ The implementor (AI) does not run `cargo` commands directly.** At each step where a build or test run is needed, the implementor provides the exact command and asks the user to run it and share the output. The user is the one who executes all `cargo` invocations.

---

## Implementation Sequence

| Step | What | Key Files |
|------|------|-----------|
| 1 | Create `src/utils/ai.rs`: `AiProvider` trait, `AcliRovodev` impl, `AiCallSpec`, `AiPaths`, `AiSession` RAII guard | `src/utils/ai.rs`, `src/utils/mod.rs` |
| 2 | Refactor `fixup.rs` to use `ai` utility | `src/commands/fixup.rs` |
| 3 | Create `src/commands/add.rs`: config, editor-open flow, meta-prompt const, script save, sidecar `.meta.json` | `src/commands/add.rs` |
| 4 | Wire `Add` into `main.rs` and `commands/mod.rs` | `src/main.rs`, `src/commands/mod.rs` |
| 5 | Create `src/commands/run.rs`: script discovery, fuzzy list via `FuzzySelect`, exec | `src/commands/run.rs` |
| 6 | Wire `Run` into `main.rs` and `commands/mod.rs` | `src/main.rs`, `src/commands/mod.rs` |
| 7 | Add `clap_complete` to `Cargo.toml` for shell tab completion | `Cargo.toml` |

---

## New Dependencies

| Crate | Purpose |
|-------|---------|
| `clap_complete` | Shell completion generation for `dt run <tab>` script name enumeration |

No other new deps needed — `dialoguer::FuzzySelect` is already available via the existing
`dialoguer` dependency, `dirs` already handles `~` expansion, `serde_json` handles script
metadata.
