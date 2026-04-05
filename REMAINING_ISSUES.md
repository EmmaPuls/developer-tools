# `dt` — Remaining Issues & Known Gaps

A living document of known problems, design gaps, and areas needing improvement in the `dt` CLI.

---

## 1. Missing `list`, `delete`, and `edit` Subcommands

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
- With a per-script folder structure, `delete` becomes straightforward: `rm -rf ~/.dt/scripts/<name>/`.

---

## 2. Insufficient Confirmation / Progress Feedback Steps

**Issue:** The `dt add` flow goes from "capturing the prompt" straight to "running the AI" with no intermediate feedback to the user. The user does not know:
- What the name of the script will be
- What permissions the AI will have
- What was actually generated before it is saved.

**Desired improvements:**
- After capturing the user's prompt but before calling the AI for the stem (Phase A), display a summary of what is about to happen — the enabled generation rules (from `AddConfig`) and the target language — and ask for confirmation before proceeding.
- After Phase A returns the script stem, display the derived script name and the save path so the user can confirm before the expensive Phase B AI call runs.
- Add a progress spinner or step counter during both AI calls (Phase A and Phase B) so the user knows the process is running and roughly where it is.
- After Phase B returns the generated script, show the script content in a pager (e.g. `less`) or a plain stdout preview, then prompt the user to **accept**, **edit** (open in `$VISUAL`/`$EDITOR`), or **abort** before the script is saved to disk.
- Upon successful save, print the script name, full save path, and description as a clear completion summary.

---

## Summary Table

| # | Issue | Severity | Area |
|---|-------|----------|------|
| 1 | No `list`, `delete`, or `edit` subcommands | High | New commands needed |
| 2 | Too few confirmation steps before and after generation | Medium | `add.rs` |
