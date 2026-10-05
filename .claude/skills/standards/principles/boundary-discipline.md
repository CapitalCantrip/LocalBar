# Boundary discipline

Place validation, type narrowing and error handling at the system's
boundaries. Trust internal code. Keep business logic in pure functions, and the
shell around them thin.

**Why:** Validation scattered through the code is noisy, redundant and gives a
false sense of safety. Logic kept out of framework wiring can be tested without
the framework.

**Pattern:**
- **At boundaries** (command-line arguments, config files, external APIs,
  network input): validate, return errors, handle defensively.
- **Inside:** typed data, errors propagated, nothing re-validated.
- **Across the boundary:** expose the project's own concepts, not the
  transport's or the storage's private representation.
- Parse raw data into the project's types at the boundary.
- Put business logic in pure functions with no framework dependency.

**The tests:**
- "Is this data crossing a boundary right now?" If not, validation here is
  redundant.
- "Can this be a pure function the shell just calls?" If yes, extract it.

**Enforced by:** nothing yet; a judgement call.

Source: adapted from pstack's *boundary-discipline* (Lauren Tan, MIT; LICENSE-pstack).
