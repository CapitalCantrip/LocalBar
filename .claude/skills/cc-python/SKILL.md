---
name: cc-python
description: >
  Cyclomatic complexity check for Python via cc-check (radon); flags functions above CC 8 and proposes refactors. Use when asked to check Python complexity.
---

# /cc-python — Python Cyclomatic Complexity Check

Runs `cc-check` (quality_check.py) against Python source files and reports
functions above the complexity threshold.

## Invocation

```
/cc-python [path] [--threshold N]
```

- `path` — file or directory (default: `.`)
- `--threshold N` — flag CC > N (default: 8). A project may pass a lower number, never a higher one

## Steps

1. Identify Python source directories in the project. The tool skips the folders on the [skip list](references/skip-list.md) and follows symlinked folders by the [symlink rule](references/symlinked-folders.md). It names each skipped folder it found on a last `Skipped N folder(s):` line; if that line names a folder holding real source, report it. A folder marked `(already scanned)` was scanned once and is not lost source.
2. Run:
   ```bash
   cc-check <dirs> --threshold 8 --format text
   ```
   8 is the agent-code ceiling (the [complexity ceiling](references/complexity-ceiling.md)); use a lower number only if the project sets one.
   `cc-check` takes several paths in one run.
   If the repo has a CC baseline (e.g. `cc-baseline.json`), add `--baseline <file>`: it then fails only on new or worse functions.
   Do not substitute `ruff --select C901`: ruff skips `and`/`or` and comprehension clauses and scores lower than radon, which CRAP also uses
3. Report all flagged functions with file, line, CC score
4. For each violation propose a concrete refactor: extract named sub-functions, replace long if/elif chains with dispatch dicts, simplify nested loops

## Adopting the gate in a repo with existing debt

1. Record the debt once: `cc-check <dirs> --baseline cc-baseline.json --update`. Commit the file.
2. Add a test so CI enforces it:
   ```python
   def test_no_function_is_more_complex_than_the_baseline_allows(self):
       with self.assertRaises(SystemExit) as stop:
           quality_check.main(["src", "--baseline", "cc-baseline.json"])
       self.assertEqual(stop.exception.code, 0)
   ```
3. When a refactor lowers a baselined score, the check fails until you run `--update` and commit the lower baseline. `--update` never records a new or worse function: fix those instead.

Baseline keys follow the [baseline ratchet](references/baseline-ratchet.md) and the [baseline key rule](references/baseline-keys.md): a file behind a symlinked folder is keyed by its real path. Functions are named by the [function naming rule](references/function-names.md), and a name that repeats in a file gets its order as a suffix by the [repeated-names rule](references/repeated-names.md). A renamed or moved function counts as new.

## Thresholds

| CC  | Action |
|-----|--------|
| 1–5 | none |
| 6–8 | acceptable for agent code; needs tests to pass CRAP (62% at 6, 100% at 8) |
| 9+ | fails the gate; refactor before commit |

## Architectural constraints — apply when writing new Python in this session

- Maximum CC: 8 (agent ceiling)
- Deconstruct before implementing: identify branching paths first, extract each into a named helper before writing the main function body
- Flatten conditionals: dispatch dicts over long `if/elif` chains, early `return` over nested branches
- One loop level: extract the body of a nested loop into a named function

## Dependencies

radon ships with quality-gates.

## Installing the commands

`cc-check`, `crap` and `comment-debt` come from the quality-gates Python package. If one is missing, install with the install line in `/setup-standards`.
