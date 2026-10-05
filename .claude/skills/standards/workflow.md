# Size rule

The size of the work decides which step it starts at. Judge the size, then
start there.

| The work... | Starts at | Then |
|---|---|---|
| will not fit in one agent session | `/wayfinder` | `/grill-with-docs`, `/to-spec`, `/to-tickets`, `/implement` per ticket |
| fits in one session but changes more than one module | `/grill-with-docs` | `/to-spec`, `/implement` |
| is smaller than that | `/tdd`, or `/diagnosing-bugs` for a defect | |

Every route ends with `/code-review`, then the project's gates. Run
`/improve-codebase-architecture` when a module starts resisting change.

When the size is unclear, say which row you picked and why in one line, then
start; the builder can redirect you.

These steps are Matt Pocock's skills (github.com/mattpocock/skills). Where his
plugin is not installed, as in cloud sessions, tell the builder which step the
size rule picks and that it is unavailable, then do that step's job yourself:
settle the open decisions with the builder before writing code, and write the
test before the change.
