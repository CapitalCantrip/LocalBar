**Asking the builder:** ask when uncertain about anything irreversible, outward-facing or large in scope, and proceed on reversible work with a stated default. Every question gives what is at stake in plain words, two or three options with only the pros and cons the builder would notice, and a recommendation with its reason (`standards` skill, *ask the builder*).

## Agent skills

### Issue tracker

GitHub Issues on CapitalCantrip/LocalBar, through the `gh` CLI. See `docs/agents/issue-tracker.md` for the create, read, list, label and close commands.

### Triage labels

Five triage labels (`needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`) and what each means. See `docs/agents/triage-labels.md`.

### Domain docs

Read `GLOSSARY.md` and the relevant ADRs in `docs/adr/` before exploring. See `docs/agents/domain.md` for how domain docs are laid out and when to add to them.

### Code comments

No code comments. The only exception is `// SAFETY:` on `unsafe` blocks. Record rationale in `docs/adr/`, known risks in GitHub issues, invariants as named tests, magic numbers as named constants. See `docs/adr/D13-no-code-comments.md`.
