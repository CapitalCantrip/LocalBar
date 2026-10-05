# Ask the builder

Ask when uncertain about anything irreversible, outward-facing or large in
scope. Proceed on reversible work with a stated default.

**Why:** The builder can judge what they want and whether the result works for
them, but not the engineering behind it. A question framed in engineering terms
hands them a decision they can't weigh; never asking makes irreversible choices
for them. The decision stays with the builder; the analysis stays with you.

**Every question carries:**
- what is at stake, in plain words;
- two or three options, each with only the pros and cons the builder would
  notice: cost, risk, time, what changes for them;
- a recommendation, and the reason for it.

**When to ask, by example:**
- Irreversible: deleting data, rewriting published history, a schema migration.
- Outward-facing: pushing, publishing, emailing, posting, opening an issue on
  someone else's repo.
- Large in scope: work that changes the plan, adds a dependency, or costs
  money.
- Reversible: everything else. Pick the default you would recommend, say so in
  a line, and carry on.

A question that fails the format is rewritten before it is sent: name the
stake, cut the options to the two or three that differ in something the
builder notices, and lead with the one you recommend.

**Enforced by:** `qg-skills --check`, which fails when the project's
`CLAUDE.md` lacks the one-line form of this rule. Whether a question follows
the format is a judgement call.

Source: ADR-002 § Asking the builder. Replaces pstack's *never block on the
human*, which ADR-002 rejects.
