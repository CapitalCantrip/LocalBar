# Size rule

The size of the work decides which step it starts at. Judge the size, then
start there.

| The work... | Starts at | Then |
|---|---|---|
| will not fit in one agent session | `/wayfinder` | `/grill-with-docs`, `/to-spec`, `/to-tickets`, `/implement-spec` |
| fits in one session but changes more than one module | `/grill-with-docs` | `/to-spec`, `/implement` |
| is smaller than that | `/tdd`, or `/diagnosing-bugs` for a defect | |

Every route ends with `/review-against-spec`, then the project's gates. Run
`/improve-codebase-architecture` when a module starts resisting change.

When the size is unclear, say which row you picked and why in one line, then
start; the builder can redirect you.

These steps are Matt Pocock's skills (github.com/mattpocock/skills), which
`qg-skills` copies into the project beside this one, so local and cloud sessions
both have them. His `code-review` ships as `/review-against-spec`, so it does
not hide Claude Code's own `/code-review`. Why they are copied: [vendored
skills](references/vendored-skills.md).
