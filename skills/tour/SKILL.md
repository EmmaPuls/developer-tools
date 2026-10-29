---
name: tour
description: "Create a CodeTour (.tour) walkthrough of a codebase topic for the VS Code CodeTour extension. Use for '/tour <topic>', 'make a code tour of X', or 'walk me through how X works' as a guided tour."
disable-model-invocation: true
---

Create a CodeTour (`vsls-contrib.codetour`) walkthrough of the codebase for the topic the user gives in the prompt (e.g. `/tour how livewall loads cameras`).

If the topic is missing or too vague to pick a starting file, ask one short question.

**1. Explore first.** Find the real code for the topic (LSP, then Grep). Read enough to explain the flow end to end. Never write a step for a line you haven't read.

**2. Write `.tours/<kebab-case-topic>.tour`** (JSON). 

```json
{
  "$schema": "https://aka.ms/codetour-schema",
  "title": "Livewall camera loading",
  "description": "One sentence on what the reader will understand by the end.",
  "steps": [
    {
      "title": "Entry point",
      "file": "frontend/src/features/livewall/LiveWall.tsx",
      "line": 42,
      "description": "What this does and why it matters. Name what the next step picks up."
    }
  ]
}
```

Step rules:

- `file` is a repo-relative path with forward slashes. Use `line` (1-based), or `pattern` (regex) when line numbers are likely to drift. Add `selection` (`{"start": {"line": 1, "character": 1}, "end": {...}}`) to highlight a range.
- Use `directory` instead of `file` to point at a folder. Omit both for an intro step with no code (`title` + `description` only).
- `description` is Markdown, 1-4 sentences: explain the _why_, not what the line literally says. Link other steps with `[#2]` and files with `[name](./path/to/file)`.
- Cross-project hops (frontend → backend, event-agent → algorithm) are fine; say so in the description.
- Optional top-level `ref` pins the tour to a branch or tag. Leave it out unless asked.

**3. Verify.** Open each `file` and confirm the `line`/`pattern` lands on the code the step describes. Check the JSON parses (`python3 -m json.tool .tours/<name>.tour`).

**4. Report** the tour path and a one-line list of step titles. Tell the user to open it via the CodeTour panel in VS Code, or the command palette: `CodeTour: Start Tour`.

Don't commit the tour unless asked.
