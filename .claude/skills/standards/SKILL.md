---
name: standards
description: >
  Engineering principles and the size rule for agent-written code. Use when
  choosing where to start a piece of work, before declaring work done, before
  putting a question to the builder, when a correction recurs, or when a
  principle is named (prove it works, subtract before you add, ...).
---

# Standards

Each principle is one rule with a name. The name is the shorthand: when the
builder or a review says *prove it works*, open that file and apply it. Open a
file when its trigger matches; read only the files that apply.

## Where to start

- **Size rule** — at the start of any piece of work, to pick the step it begins
  at. [workflow.md](workflow.md)

## Principles

- **Ask the builder** — before a decision that is irreversible, outward-facing
  or large in scope; whenever you write a question to the builder.
  [principles/ask-the-builder.md](principles/ask-the-builder.md)
- **Prove it works** — before saying a task is done.
  [principles/prove-it-works.md](principles/prove-it-works.md)
- **Explain the number** — before reporting or acting on a measured number.
  [principles/explain-the-number.md](principles/explain-the-number.md)
- **Build the lever** — when work is more than a couple of obvious edits.
  [principles/build-the-lever.md](principles/build-the-lever.md)
- **Test behaviour, not implementation** — when writing, changing or keeping a
  test. [principles/test-behaviour-not-implementation.md](principles/test-behaviour-not-implementation.md)
- **Subtract before you add** — when planning an addition, refactor or rewrite.
  [principles/subtract-before-you-add.md](principles/subtract-before-you-add.md)
- **Boundary discipline** — when placing validation or error handling.
  [principles/boundary-discipline.md](principles/boundary-discipline.md)
- **Make operations idempotent** — when writing a command, migration or setup
  step that may run twice or be interrupted.
  [principles/make-operations-idempotent.md](principles/make-operations-idempotent.md)
- **Minimise reader load** — when code is hard to trace, or before adding a
  layer or a piece of state.
  [principles/minimise-reader-load.md](principles/minimise-reader-load.md)
- **Attack the premise** — when a second fix resting on the same assumption has
  failed. [principles/attack-the-premise.md](principles/attack-the-premise.md)
- **Encode lessons in structure** — when the same correction arrives a second
  time. [principles/encode-lessons-in-structure.md](principles/encode-lessons-in-structure.md)
- **Date every "this can't be done"** — when recording or relying on a finding
  that something is impossible or unsupported.
  [principles/date-every-cant-be-done.md](principles/date-every-cant-be-done.md)
- **Stop and report when a loop runs on** — when an hour or three review rounds
  have passed without a commit.
  [principles/stop-and-report.md](principles/stop-and-report.md)

Why the standards take this shape: `docs/adr/ADR-002-engineering-standards.md`
in the quality-gates repo. Principles adapted from Lauren Tan's pstack keep its
MIT notice in [LICENSE-pstack](LICENSE-pstack).
