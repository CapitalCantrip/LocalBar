# ADR D13 — No code comments

**Status:** Accepted
**Date:** 2026-09-27
**Deciders:** Jay

---

## Context

Agents extend whatever patterns they see in the surrounding code. A comment that justifies a workaround ("bypassed on purpose for now", "leak intentionally", "use process::exit instead") reads as permission to add more workarounds, and a comment that states a precondition is a signal that the type system isn't enforcing it. Lauren Tan's *agent-friendly codebases* write-up documents the Dune framework banning inline comments for the same reason: agents used them to justify patching over root causes rather than fixing them. A review of this codebase found ~360 comment lines carrying this kind of rationale (https://claude.ai/artifact/1dwdWCXL8J4b7Y9oUFW5Da).

---

## Decision

No `//`, `///`, `//!` or `/* */` comments in `.rs`, `.ts` or `.tsx` files.

**One exception:** `// SAFETY:` directly above an `unsafe` block, required by `clippy::undocumented_unsafe_blocks`.

### Where each kind of comment goes instead

| Comment kind | Replacement |
|---|---|
| Precondition / caller contract | Change the signature or have the function do the step itself, so the precondition can't be violated |
| Invariant or ordering rule | A test named after the invariant |
| Magic number | A named `const` |
| Design decision / workaround rationale | An ADR in `docs/adr/` |
| Known risk or deferred fix | A GitHub issue |
| Section divider | Delete it. If a file needs dividers to be navigable, split it into modules |
| Restates the name | Delete it |

The legacy Swift tree (`LocalBar/`, `LocalBar.xcodeproj`) is out of scope.

---

## Consequences

- The ADR set and GitHub issues become the home for rationale that used to live in comments; a reader who wants the "why" behind a piece of code looks in `docs/adr/` or the issue tracker, not inline.
- Preconditions and invariants must be pushed into signatures, types, and named tests, which is more upfront work than writing a comment but removes an entire class of stale or unenforced documentation.
- CI gates this policy (#43); until that lands, reviewers are the enforcement mechanism.
