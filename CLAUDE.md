## Agent skills

### Issue tracker

Issues live in GitHub Issues on CapitalCantrip/LocalBar. See `.claude/agents/issue-tracker.md`.

### Triage labels

Uses the five default triage labels. See `.claude/agents/triage-labels.md`.

### Domain docs

Single-context repo with one `CONTEXT.md` at root and `docs/adr/`. See `.claude/agents/domain.md`.

### Code comments

No code comments. The only exception is `// SAFETY:` on `unsafe` blocks. Record rationale in `docs/adr/`, known risks in GitHub issues, invariants as named tests, magic numbers as named constants. See D13.
