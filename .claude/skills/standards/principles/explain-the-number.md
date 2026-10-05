# Explain the number

A measured number is a claim about a system. Before you trust it, report it or
act on it, find what limits it and rule out that it measured something else.

**Why:** A run that went wrong still prints a plausible number. Failed
requests, a cache that skipped the work, code that never ran, a side left on
default settings, and run-to-run noise all produce results that look fine. If
you cannot say why the number is not twice as good, you do not know what you
measured.

**Pattern:**
- **Ask "why not double?"** Name what bounds the result: a core, a lock, the
  disk, the network, the load generator itself. Get it from a profile or from
  counters taken during the run, not from reading the code.
- **List what else the number could be measuring,** and rule each out with
  evidence: errors, skipped or cached work, an untuned side, noise, a piece too
  small to matter end to end.
- **Keep the evidence with the number:** the run count, the spread and the
  limiter, in the notes or a linked file, so a reader can check the claim.

You skipped this when a number is reported with no run count, no spread or no
named limiter.

**Enforced by:** nothing yet; a judgement call.

Source: adapted from pstack's *explain-the-number* (Lauren Tan, MIT; LICENSE-pstack).
