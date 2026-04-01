# Workspace Memory

## Dependency Placement

- `tempfile` is required as a **`[dependencies]`** entry because it is used in production code (`src/commands/add.rs` → `open_editor_for_prompt()`). Keep it in `[dependencies]`.

## Cargo Convention

- If you need to run a `cargo` command ask the user to run it. Provide the exact command and ask the user to run it. If you get an SSL error you are probably trying to run a cargo command that you should have asked the user to run for you.
