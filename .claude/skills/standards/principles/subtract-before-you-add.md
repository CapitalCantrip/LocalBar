# Subtract before you add

When changing a system, remove complexity first, then build on the simpler
base.

**Why:** Adding to a complex system compounds the complexity. Removing first
leaves less code, shows the essential structure, and usually makes the next
design obvious. Leave the design slightly simpler than you found it, behind the
same or a smaller surface.

**Pattern:**
- Remove dead code, redundant checks and stale references before construction.
- Get to the minimum that works before polishing it.
- Design for the usage you have observed, not edge cases you imagine.
- Add validators, parsers and guards only where the spec demands them.
- Remove redundant instructions from prompts and documents.
- When a reference has nothing new in it, delete it rather than leaving a stub.

**Enforced by:** nothing yet; a judgement call. `comment-debt` enforces one
case: a comment cannot be added, so the knowledge goes to a test, a name or an
ADR instead.

Source: adapted from pstack's *subtract-before-you-add* (Lauren Tan, MIT; LICENSE-pstack).
