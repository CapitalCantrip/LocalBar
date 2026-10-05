# Encode lessons in structure

When the same correction arrives a second time, put the fix in a mechanism
that enforces it, not in more text.

**Why:** A written rule needs the reader to notice it, remember it and comply.
A check enforces it without anyone's cooperation. Agents copy whatever the
surrounding code already does, so a weak guard becomes the next template.

**Pick the strongest place available,** in this order:
1. Architecture: make the mistake impossible to write.
2. A gate in quality-gates: a command that fails the commit or the build.
3. A behaviour test in the project.
4. Prose, last: a principle or a `CLAUDE.md` line, with an example of the
   failure.

When the fix is structural, delete the instruction it replaces; the instruction
was the symptom.

**When a lesson becomes a standard:** when it has happened at least twice and
would recur in another project. A one-off or project-specific lesson stays in
its project. A standard that becomes a gate has its principle file name the
gate under **Enforced by**.

**The loop:**
- Capture every correction: decide whether it is a one-off or a pattern.
- Route it: a one-off to the project's notes, a recurring fix to a check, a
  rule that holds across projects to quality-gates as a gate or a principle.
- Close it: apply the fix now, or open an issue for it. "I'll keep that in mind"
  persists nothing.

**Enforced by:** this is the promotion path itself. The stage guard is its
first product: committing every changed file at once kept recurring as a
written lesson, so it became a hook that refuses the command.

Source: adapted from pstack's *encode-lessons-in-structure* (Lauren Tan, MIT;
LICENSE-pstack), with the promotion order from ADR-002.
