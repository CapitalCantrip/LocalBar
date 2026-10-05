# Stop and report when a loop runs on

After an hour of work, or three review rounds, without a commit, stop and
report to the builder before continuing.

**Why:** An agent in a loop of fix, review, fix looks busy and productive from
outside. The builder can't tell polish nobody agreed to from progress, and the
loop rarely ends on its own.

**The report:**
- what is done;
- what is left, and your estimate of it;
- what you decided along the way that nobody asked for.

Then ask whether to continue, in the form *ask the builder* sets out. A commit
of a working step resets the count; so does the builder's answer.

**Enforced by:** nothing yet; a judgement call.

Source: lessons from use, ADR-002.
