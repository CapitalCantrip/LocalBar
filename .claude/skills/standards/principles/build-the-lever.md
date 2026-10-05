# Build the lever

When the work is more than a couple of obvious edits, build the tool that does
it or proves it (a codemod, a script, a generator, a check) instead of working
by hand.

**Why:** Two payoffs. Throughput: a script does the work the same way every
time and reruns for free. Confidence: the script is one artifact a reviewer can
read and rerun, where hand-done changes can only be re-verified by redoing them.
A script turns "trust me" into "run this".

**Pattern:**
- Do the first unit by hand to learn the recipe, then build the tool. Prove it
  by rerunning it on that unit and comparing with your hand-done version. Make
  it safe to rerun.
- A script that can process every unit in one pass beats handing the units out
  to subagents.
- When you do hand work out to subagents, write the recipe, the check and the
  files they must not touch in one document they all read, outside what they
  can edit.
- Applying this principle produces a file. If you cite it and there is no
  script, codemod, generator or recipe in the diff, you didn't apply it.
- Commit the lever when the work outlives the session.

**Balance:** The bar is triviality, not repetition. Build the smallest script
that does or proves the job, never a framework.

**Enforced by:** nothing yet; a judgement call. *Encode lessons in structure*
makes a recurring rule permanent; this principle is about the work in front of
you.

Source: adapted from pstack's *build-the-lever* (Lauren Tan, MIT; LICENSE-pstack).
