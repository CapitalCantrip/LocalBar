# Minimise reader load

Maintainability is the work a reader must do to understand the code. Track two
things:
1. **Layers to trace:** how many indirections sit between the question and the
   answer.
2. **State to hold:** how much hidden or changeable context the reader must
   keep in their head.

**Why:** Code is read far more than it is written, and agents read it on every
task. The two are independent: a flat file with fifty globals is as hard to
follow as a six-layer adapter stack.

**Pattern:**
- **Collapse layers** that cost more than they save: wrappers with one caller,
  adapters with one implementation, indirection added for a future that never
  came.
- **Make each layer change the abstraction.** A layer that repeats the same
  methods and arguments as the one below adds load and hides nothing.
- **Shrink state:** return values over mutation, locals over fields, fields over
  module state, module state over globals. Derive a value instead of keeping two
  copies in sync.
- **Name an invariant once, at the boundary,** not in every caller.
- Before adding a layer or a piece of state, ask whether it removes at least as
  much load somewhere else.

**The test:** can a new reader answer "where does X come from?" and "what can
change X?" in under thirty seconds?

**Enforced by:** `cc-check` at 8 and `crap` at 8, in part: they cap the paths
through one function. Layers and state are a judgement call.

Source: adapted from pstack's *minimize-reader-load* (Lauren Tan, MIT; LICENSE-pstack).
