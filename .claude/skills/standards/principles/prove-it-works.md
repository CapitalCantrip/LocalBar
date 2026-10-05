# Prove it works

Before saying a task is done, check the real thing directly: run the feature,
read the actual value, inspect the diff. A proxy, a self-report or "it
compiles" proves nothing.

**Why:** Unverified work has unknown correctness. Indirect checks (file
timestamps, output that looks fresh, another agent's summary, a cached
screenshot) feel cheaper than looking, and acting on a wrong inference costs far
more than the look.

**Pattern:**
- Check the process is alive by asking it, not by reading state it left behind.
- Read the value itself, not a cached or derived copy of it.
- When the check fails, suspect the way you observed before the system.
- Script the check when you can: a script that reruns the same comparison is a
  proof a reviewer can rerun, where a one-time look is a claim. Keep its output
  where the builder can see it.

**Enforced by:** nothing yet; a judgement call. The gates prove the code meets
the standards, not that it does what was asked.

Source: adapted from pstack's *prove-it-works* (Lauren Tan, MIT; LICENSE-pstack).
