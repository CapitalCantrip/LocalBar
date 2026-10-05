---
name: setup-standards
description: >
  Set up or upgrade a project for the quality-gates standards: issue tracker
  and agent docs, gates, baselines, hooks, CI and the asking rule. Use when
  asked to set up the standards, or after bumping the quality-gates pin.
---

# /setup-standards

Prepare this project for all three layers of the standards (workflow,
principles, gates) and end with a report the builder can read without
engineering knowledge. Run on a set-up project, every step finds its work
already in place and changes nothing, so this is also the upgrade after a pin
bump.

The pin this skill belongs to: `v0.4.1`. Install line, used in step 2:

```bash
uv tool install "quality-gates @ git+https://github.com/CapitalCantrip/quality-gates@v0.4.1"
```

Ask the builder only where a choice is theirs, in the form the `standards`
skill's *ask the builder* sets out. Everything else, decide and state the
default in the report.

Keep a running list as you go: for each step, **done**, **already in place**,
or **not done** with the reason. Step 7 reads from it.

## 1. Agent docs and the glossary

1. Pick the issue tracker from `git remote -v`: GitHub for github.com, GitLab
   for a GitLab host. Ask the builder only when there is no remote or it is
   neither; offer GitHub, GitLab or local markdown files.
2. If `.claude/agents/` holds `issue-tracker.md`, `triage-labels.md` or
   `domain.md` (Pocock's earlier layout), `git mv` each into `docs/agents/`.
3. Run `qg-agent-docs --tracker <github|gitlab|local>`. It writes
   `docs/agents/` from Matt Pocock's setup templates, adds the Agent skills
   block to `CLAUDE.md`, and never overwrites a file the project edited. Copy
   every line of its output into the report, including any template diff, so
   the builder can choose to adopt it.
4. On GitHub, create each of the five labels in `docs/agents/triage-labels.md`
   that `gh label list` lacks, with `gh label create`.
5. Glossary: if `CONTEXT.md` exists and `GLOSSARY.md` does not, `git mv` it to
   `GLOSSARY.md`, replace `CONTEXT` with `GLOSSARY` in `docs/agents/domain.md`,
   and run `qg-agent-docs` again. If neither exists, write `GLOSSARY.md`
   holding the project's name as a heading and one sentence on what the
   project is, taken from its README.
6. If `docs/adr/` does not exist, create it with an empty `.gitkeep`.
7. Read `CLAUDE.md` for lines that restate what `docs/agents/` now holds: a
   label list, issue tracker commands, where the glossary lives, a path into
   `.claude/agents/` or a mention of `CONTEXT.md`. Replace each with a
   one-line context pointer to the file that holds it. A context pointer says
   what is at the other end, so an agent knows when to open it: "GitHub Issues
   on this repo, through the `gh` CLI. See `docs/agents/issue-tracker.md`.",
   not "See `docs/agents/issue-tracker.md`." Check the pointers already in
   `CLAUDE.md` the same way, and rewrite any that only name a file. Put each
   before and after in the report.

Done when `qg-agent-docs` reports every file, the five labels exist, a
glossary and `docs/adr/` exist, no line in `CLAUDE.md` contradicts
`docs/agents/`, and every line in `CLAUDE.md` that names a file in
`docs/agents/` also says what that file holds.

## 2. Gates for each language

Detect the languages: Python (`pyproject.toml`, `setup.py` or `.py` files
outside `.venv`), Swift (`Package.swift`, an `.xcodeproj` or `.swift` files),
Rust (`Cargo.toml`). Do every block that applies.

- **All projects:** add the pinned requirement to the project's dev
  dependencies (Python), or to the CI install step (others), replacing any
  older `quality-gates` pin. Run `qg-skills`: it copies the skills and the
  stage guard into `.claude/`, registers the guard in `.claude/settings.json`,
  and adds the asking-rule line to `CLAUDE.md`.
- **Python:** `comment-debt`, `cc-check`, `crap`. The `/no-comments` skill
  describes the comment gate.
- **Swift:** the complexity gate is SwiftLint's `cyclomatic_complexity` block,
  exactly as `/cc-swift` gives it, in `.swiftlint.yml`. CRAP needs an Xcode
  `.xcresult`: an Xcode project gets it as `/crap` describes; a SwiftPM-only
  package cannot yet (quality-gates issue #6), so report CRAP as not done with
  that reason.
- **Rust:** run the setup steps of `/cc-rust`: `clippy.toml` and the deny
  attributes.

Done when every detected language has its gates configured and `qg-skills`
has run.

## 3. Baselines

Record today's debt so the gates fail only on new problems. Run each command
from the repo root and commit the file it writes.

- **Python:** `comment-debt --update`;
  `cc-check <source dirs> --baseline cc-baseline.json --update`; collect
  coverage (`coverage run -m pytest` or `-m unittest discover`, then
  `coverage json`) and run
  `crap --lang python <source dirs> --coverage-json coverage.json --baseline crap-baseline.json --update`.
  Add `coverage.json` and `.coverage` to `.gitignore`.
- **Swift:** run SwiftLint. Each function already over the limit gets
  `// swiftlint:disable:next cyclomatic_complexity` above it; that pragma is its
  baseline entry. With an `.xcresult`, record `crap --lang swift` with
  `--baseline crap-baseline.json --update`.
- **Rust:** run `cargo clippy --all-targets`. Each function it fails gets
  `#[allow(clippy::cognitive_complexity)]` or `#[allow(clippy::too_many_lines)]`;
  that attribute is its baseline entry.

Report each baseline with its count: "12 functions over the complexity limit,
recorded; new ones will fail". Done when the gate passes on today's tree.

## 4. Hooks

1. `.githooks/pre-commit` runs every fast gate, stopping at the first failure:
   `comment-debt` and `cc-check <source dirs> --baseline cc-baseline.json` for
   Python; SwiftLint for Swift where installed. Keep any lines the project
   already had. Make it executable.
2. Run `git config core.hooksPath .githooks`.
3. Add a `SessionStart` hook to `.claude/settings.json` that runs
   `git config core.hooksPath .githooks`, so cloud sessions get the hook too.
   Merge into the existing settings; leave other hooks alone.
4. The stage guard came with `qg-skills` in step 2. Prove it works: pipe
   `{"tool_input":{"command":"git add -A"}}` to
   `python3 .claude/hooks/stage_guard.py` and see exit code 2.

Done when a commit runs the fast gates, the stage guard blocks `git add -A`,
and the hook path is set for local and cloud sessions.

## 5. CI

Make CI enforce what the hooks enforce, plus the slow gates.

- **Python:** add the test file in [ci-python.md](ci-python.md) to the test
  suite, and the CRAP step from the same file to the CI workflow after the
  tests.
- **Rust:** a CI step running `cargo clippy --all-targets`.
- **Swift:** a CI step running SwiftLint, on a macOS runner. If the project has
  no macOS CI job, report this as not done and say what it would cost.
- If the project has no CI workflow, add `.github/workflows/tests.yml` that
  installs the project, runs the tests and the steps above, on pull requests
  and pushes to the default branch.

Done when the test suite passes locally with the new tests, and every gate the
hooks run also runs in CI.

## 6. The asking rule

`qg-skills` added the line in step 2. Run `qg-skills --check`; it exits 0 only
when the skills, the stage guard, its registration and the asking-rule line
are all current. If the line sits under an unrelated heading, move it to just
below the file's opening paragraph; `qg-skills` keeps it where it is from then
on.

Done when `qg-skills --check` exits 0.

## 7. Report

Write the report for the builder, in plain words, as a table with one row per
step above (and per language in steps 2, 3 and 5): **done**, **already in
place**, or **not done** with the reason and what it would take. Below it, list
each choice you made on the builder's behalf, each `CLAUDE.md` line you
replaced (before and after), and each template change `qg-agent-docs` showed.
Then commit the changes, staging files by name.

Done when every step in this skill has a row, and every row says what the
builder would notice.
