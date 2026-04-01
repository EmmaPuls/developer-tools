# Workspace Memory

## Dependency Placement

- `tempfile` is required as a **`[dependencies]`** entry because it is used in production code (`src/commands/add.rs` → `open_editor_for_prompt()`). Keep it in `[dependencies]`.

## Cargo Convention

- The user runs all `cargo` commands. At each step needing a build or test run, provide the exact command and ask the user to run it.
