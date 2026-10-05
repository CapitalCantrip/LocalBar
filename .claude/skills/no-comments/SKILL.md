---
name: no-comments
description: >
  Set up or run the no-code-comments gate (comment-debt) in a repo, or pay down
  one file's comment debt. Use when adding the gate to a project, when
  comment-debt fails, or when asked to convert or pay down comments.
---

# /no-comments — comment gate

Code carries no comments: no `#` comments, docstrings, or strings used as
comments. Bare pragmas (`# noqa: E402`) are exempt. Existing comments are debt,
held by a per-file baseline that only goes down.

## Run

```bash
comment-debt            # exit 1 if any file is above or below its baseline
comment-debt --status   # remaining debt by area, rationale-flagged files first
comment-debt --update   # lower the baseline after paying debt; never raises
```

Options: `--baseline PATH` (default `comment-debt.json` at the repo root),
`--exclude PREFIX` (repeatable).

## Set up in a repo

1. Install the package (see below) and add it to the project's dev
   dependencies, pinned to a tag.
2. `comment-debt --update` writes the first baseline. A new repo gets `{}`.
3. Add `.githooks/pre-commit` containing `exec comment-debt` and run
   `git config core.hooksPath .githooks`. For cloud sessions, add a
   `SessionStart` hook to `.claude/settings.json` that runs that git config.
4. Add a test that calls `quality_gates.comment_debt.main([], root=<repo>)` and
   asserts 0, so the suite and CI enforce it.

## Where a comment's knowledge goes

Prefer somewhere that fails when the knowledge stops being true.

| The comment says… | It goes to |
|---|---|
| why an edge case behaves as it does | a test named as a sentence |
| why a value is what it is | a named constant, plus a test if load-bearing |
| what a caller must do first | the signature, or the function does the step itself |
| why this design and not another | the repo's ADR, as a dated addendum |
| what a domain word means | the repo's glossary |
| a known risk or deferred fix | an issue |
| `--help` text read through `__doc__` | argparse `description=` |
| what the next line plainly does | nowhere; delete it |

## Paying down one file

1. `comment-debt --status`, pick the top file.
2. Write tests for the edge cases the comments describe; they must pass first.
3. Remove the comments; run the suite.
4. `comment-debt --update`; commit the file, its tests and the baseline
   together, saying where each piece of knowledge went.

## Installing the commands

```bash
uv tool install "quality-gates @ git+https://github.com/CapitalCantrip/quality-gates@v0.4.1"
```
