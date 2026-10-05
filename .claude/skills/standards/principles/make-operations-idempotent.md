# Make operations idempotent

Design every operation that changes state so it converges on the same end
state however many times it runs and wherever a previous run stopped.

**Why:** Commands, setup steps and processing loops run where crashes,
restarts and retries are normal. If a partial run changes what the next run
does, every restart becomes a debugging session.

**Pattern:**
- On start, look at what already exists: adopt what is correct, repair or
  replace what is stale.
- Compare by content, not by creation order or timestamps.
- Detect stale locks by checking that their owner is still alive.
- Let failed work restart cleanly from fresh input.

**The test:**
1. What happens if this runs twice in a row?
2. What happens if the previous run crashed at each possible point?
3. Does running it again reach the same end state?

If any answer is "it depends on what was left behind", the operation needs a
step that reconciles the current state first.

**Enforced by:** nothing yet; a judgement call.

Source: adapted from pstack's *make-operations-idempotent* (Lauren Tan, MIT;
LICENSE-pstack).
