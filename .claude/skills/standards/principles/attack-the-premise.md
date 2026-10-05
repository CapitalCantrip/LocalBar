# Attack the premise

When two or more fixes that share one premise have failed the same check,
suspect the premise, not the fixes.

**Why:** Each failure under a shared premise is evidence against the premise.
A third fix that assumes it will fail the same way.

**Pattern:**
- **Write the premise down:** the one sentence every failed fix assumed.
- **Gather evidence before the next fix.** Measure where the problem actually
  sits, with a script you can rerun (*build the lever*), rather than how large
  it is.
- **Read the skew.** If the problem sits in the same few places on every run,
  something puts it there. Find that cause and fix it, instead of compensating
  for it with more machinery on every run.

**Stop:** start no further fix until the premise is written down and the
evidence exists. If the evidence is even, the premise is not the cause; look
elsewhere and keep the evidence.

**Enforced by:** nothing yet; a judgement call.

Source: adapted from pstack's *attack-the-premise* (Lauren Tan, MIT; LICENSE-pstack).
