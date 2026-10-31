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

The pin this skill belongs to: `v0.12.0`. Install line, used in step 2:

```bash
uv tool install --with-executables-from coverage "quality-gates[coverage] @ git+https://github.com/CapitalCantrip/quality-gates@v0.12.0"
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
   `docs/agents/` from Matt Pocock's setup templates, writes
   `.github/pull_request_template.md` with the `pr` skill's headings, adds the
   Agent skills block to `CLAUDE.md` (with a *Pull requests* section that
   sends PR bodies to the `pr` skill), and never overwrites a file the project edited. Copy
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
Rust (`Cargo.toml`), TypeScript or JavaScript (`package.json`, or `.ts`, `.tsx`,
`.js`, `.jsx`, `.cjs` or `.mjs` files outside `node_modules`). Do every block that applies.

- **All projects:** add the pinned requirement to the project's dev
  dependencies (Python), or to the CI install step (others), replacing any
  older `quality-gates` pin. Run `qg-skills`: it copies the skills and the
  stage guard into `.claude/`, registers the guard in `.claude/settings.json`,
  and adds the asking-rule line to `CLAUDE.md`.
- **Python:** `comment-debt`, `cc-check`, `crap`. The `/no-comments` skill
  describes the comment gate.
- **Swift:** the complexity gate is SwiftLint's `cyclomatic_complexity` block,
  exactly as `/cc-swift` gives it, in `.swiftlint.yml`. The CRAP gate is
  `crap --lang swift`, reading coverage as `/crap` describes: an Xcode project
  from its `.xcresult` bundle (`--xcresult`), or a SwiftPM package with a test
  target from the llvm-cov export JSON its tests write (`--llvm-cov-json`). A
  SwiftPM package with no test target has no CRAP gate; step 3 says how to
  report that.
- **Rust:** run the setup steps of `/cc-rust`: `clippy.toml` and the deny
  attributes.
- **TypeScript / JavaScript:** `cc-check --lang typescript` and
  `crap --lang typescript`, as `/cc-typescript` and `/crap` describe. CRAP
  needs Istanbul's `coverage-final.json`: Vitest with `@vitest/coverage-v8`
  and `--coverage.reporter=json`, or Jest with `--coverageReporters=json`. A
  project on `node --test` cannot yet (it writes lcov), so report CRAP as not
  done with that reason.

Done when every detected language's gates are configured, each CRAP gate step
2 or 3 says to report as not done is reported not done with its reason, and
`qg-skills` has run.

## 3. Baselines

Record today's debt so the gates fail only on new problems. Run each command
from the repo root and commit the file it writes.

- **Python:** `comment-debt --update`;
  `cc-check <source dirs> --baseline cc-baseline.json --update`; collect
  coverage (`coverage erase`, then `coverage run -m pytest` or
  `-m unittest discover`, then `coverage json`) and run
  `crap --lang python <source dirs> --coverage-json coverage.json --baseline crap-baseline.json --update`.
  Add `coverage.json` and `.coverage` to `.gitignore`. Run coverage in the
  environment the tests run in; when the tests start child processes, add
  `[tool.coverage.run] patch = ["subprocess"]` to `pyproject.toml`. If the
  update exits 1 listing `UNMEASURED` functions, coverage missed those files:
  fix the measuring or take them out of the scan, then rerun. Do not report
  them as debt.
- **Swift:** run SwiftLint. Each function already over the limit gets
  `// swiftlint:disable:next cyclomatic_complexity` above it; that pragma is its
  baseline entry. Then record CRAP from the project's coverage. In a SwiftPM
  package with a test target, run `swift test --enable-code-coverage`, then
  `crap --lang swift Sources/ --llvm-cov-json "$(swift test --show-codecov-path)" --baseline crap-baseline.json --update`.
  In an Xcode project with an `.xcresult`, run
  `crap --lang swift <source dirs> --xcresult <bundle> --baseline crap-baseline.json --update`.
  Run the tests in the same checkout as `crap`: it matches a report file only at
  its exact path, so a bundle made in another checkout or container leaves every
  function unmeasured, and unmeasured functions fail the gate.
  A SwiftPM package with no test target (no `.testTarget` in its
  `Package.swift`) has no coverage to record, so CRAP is not set up for it.
  Report CRAP as **not done**, with the reason (no test target, so
  `swift test --enable-code-coverage` reports no tests found) and what it takes
  (add a test target with tests, then rerun `/setup-standards`).
- **TypeScript / JavaScript:**
  `cc-check --lang typescript <source dirs> --baseline cc-baseline.json --update`;
  collect coverage as in step 2 and run
  `crap --lang typescript <source dirs> --istanbul-json coverage/coverage-final.json --baseline crap-baseline.json --update`.
  Add `coverage/` to `.gitignore`. A project on `node --test` gets no CRAP
  baseline: report CRAP as not done, as step 2 says.
- **Rust:** run `cargo clippy --all-targets`. Each function it fails gets
  `#[allow(clippy::cognitive_complexity)]` or `#[allow(clippy::too_many_lines)]`;
  that attribute is its baseline entry.

A project in any language with no tests has no coverage for `crap` to read, so
CRAP is not set up for it. Report CRAP as **not done**, with the reason (no
tests, so no coverage) and what it takes (write tests, then rerun
`/setup-standards`). Its other gates are set up as usual.

Report each baseline with its count: "12 functions over the complexity limit,
recorded; new ones will fail". Done when each gate passes on today's tree,
except a CRAP gate step 2 or 3 says to report as not done, which is reported
not done with its reason.

On an upgrade, a baseline that fails after the bump is re-recorded as the
release's CHANGELOG entry says under *When you bump the pin*.

## 4. Hooks

1. `.githooks/pre-commit` runs every fast gate, stopping at the first failure:
   `comment-debt` and `cc-check <source dirs> --baseline cc-baseline.json` for
   Python; `cc-check --lang typescript <source dirs> --baseline cc-baseline.json`
   for TypeScript or JavaScript; `swiftlint` for Swift where installed. Keep any lines the project
   already had. Make it executable. If the project has a `.venv`, the hook's
   first line after the shebang is `[ -d .venv/bin ] && PATH="$PWD/.venv/bin:$PATH"`:
   VS Code and cloud shells run the hook without the venv activated.
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
- **TypeScript / JavaScript:** the steps in [ci-typescript.md](ci-typescript.md).
- **Swift:** a CI step running SwiftLint, on a macOS runner. If the project has
  no macOS CI job, report this as not done and say what it would cost. A
  SwiftPM package with a test target also gets a CRAP step, which runs on Linux
  or macOS. Put it on the one the baseline was recorded on, and run both
  commands in the same job:

  ```bash
  swift test --enable-code-coverage
  crap --lang swift Sources/ --llvm-cov-json "$(swift test --show-codecov-path)" --baseline crap-baseline.json
  ```

  A function under `#if os(macOS)` or `canImport(UIKit)` has no coverage
  record on Linux, so it is unmeasured there and fails the gate. A report made
  in another checkout or container leaves every function unmeasured, because
  `crap` matches a report file only at its exact path.

  A SwiftPM package with no test target gets no CRAP step: report CRAP as
  **not done**, as step 3 says.
- If the project has no CI workflow, add `.github/workflows/tests.yml` that
  installs the project, runs the tests and the steps above, on pull requests
  and pushes to the default branch. A SwiftPM package with no test target has
  no tests to run (`swift test` exits 1 there), so the Swift part of its
  workflow runs SwiftLint only.

A project with no tests gets no CRAP step: report CRAP as **not done**, as
step 3 says.

Done when the test suite, where the project has one, passes locally with the
new tests, and every gate the hooks run also runs in CI.

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
