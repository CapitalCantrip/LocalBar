---
name: crap
description: >
  Quality gate for Python, Swift or TypeScript/JavaScript: CRAP scores
  (CRAP > 8 = FAIL). Use when asked about complexity, coverage, or whether code passes the gate.
---

# /crap — Quality Gate

CRAP score: `CRAP(f) = CC(f)² × (1−cov)³ + CC(f)`.
Gate: CRAP > 8 = FAIL. Fully-covered collapses to CRAP = CC; uncovered scores CC² + CC.

Rust has no CRAP score: use `/cc-rust`.

## Invocation

```
/crap [--lang python|swift|typescript] [paths] [coverage options]
```

## Steps

1. Detect language from project context (`.swift` files → Swift; `package.json` or `.ts`/`.tsx`/`.js`/`.jsx`/`.cjs`/`.mjs` files → TypeScript; else Python).
2. **Python** — look for `coverage.json` (`coverage run -m pytest && coverage json`).
   **Swift** — an Xcode project: look for an `.xcresult` bundle (`xcodebuild test -resultBundlePath ...`). A SwiftPM package (`Package.swift`): run `swift test --enable-code-coverage`; the report is at `swift test --show-codecov-path`. A SwiftPM package with no test target has no coverage, so CRAP cannot gate it; `--no-coverage` still ranks its functions (step 4), but the gate needs tests.
   **TypeScript** — look for `coverage/coverage-final.json` (Istanbul's `json` reporter, written by Vitest and Jest).
3. Run the gate:

   **Python — with coverage:**
   ```bash
   coverage run -m pytest && coverage json
   crap --lang python <dirs> --coverage-json coverage.json
   ```

   **Python — no tests yet:**
   ```bash
   crap --lang python <dirs> --no-coverage
   ```

   **Swift — with coverage, Xcode project:**
   ```bash
   crap --lang swift Sources/ --xcresult /tmp/out.xcresult
   ```
   Run both in the same checkout. A report file counts only at the exact path `crap` scans, so a bundle made in another checkout or container leaves every function unmeasured, and a file name shared with another folder's file is never borrowed. When one file's functions are all `UNMEASURED` while others have figures, check that the Xcode project's file reference spells the file in the same case as the disk.

   **Swift — with coverage, SwiftPM package:**
   ```bash
   swift test --enable-code-coverage
   crap --lang swift Sources/ --llvm-cov-json "$(swift test --show-codecov-path)"
   ```
   Run both in the same checkout, as for an Xcode project. A function whose lines hold the opening of another function's body is unmeasured: two functions on one line both do, and so does a function that ends on the line where the next one opens, while that next one keeps its own figure. Put each function on its own lines. A closure or autoclosure default argument on the line of the `{` (`= { _ in }`, `= {}`, `= flagA && flagB`, `= x ?? nil`) also leaves its function unmeasured; move it to a line above the `{`. A literal default (`= 0`, `= nil`, `= []`, `= .main`) is fine.
   Failing tests must fail the job at the `swift test --enable-code-coverage` step; `--show-codecov-path` runs no tests. When the path command itself fails (a broken `Package.swift`, or no package in the directory), the flag gets an empty path and `crap` exits 2: fix the package, not the flag.

   **Swift — no tests yet:**
   ```bash
   crap --lang swift Sources/ --no-coverage
   ```
   Which Swift initialisers and accessors become Functions, including `init?` and `private(set)`: [how lizard reads Swift](references/swift-reading.md).

   **TypeScript — with coverage** (Vitest needs `@vitest/coverage-v8`; under Jest use `npx jest --coverage --coverageReporters=json`):
   ```bash
   npx vitest run --coverage --coverage.reporter=json
   crap --lang typescript src --istanbul-json coverage/coverage-final.json
   ```

   **JSON output (for CI):**
   ```bash
   crap --lang python <dirs> --coverage-json coverage.json --json
   ```

4. With `--no-coverage`, every function at CC 3 or above scores FAIL. That mode is a risk ranking, not a gate: report the top rows and say no coverage was available, never "the gate failed".
   An `UNMEASURED` row is a function the report holds no figure for: it has no CRAP score (`—`), it fails the gate, and the summary names its file. It is a gap in measuring, not untested code: report it as such, then measure the file (same checkout, coverage run in the environment the tests use) or take it out of the scan. `--baseline --update` never records it and keeps any entry it already has, so it still exits 1 until the gap is fixed.
   When `crap` prints `no scored function got a figure from the coverage report`, every function in this run is unmeasured, whatever the language: do not report it as a gate result. Measure coverage again in this checkout, check that the report holds figures for the files you scan, and rerun.
   With coverage, report all FAIL rows in full; summarise WARN rows (CRAP 5–8).
   `crap` skips the folders on the [skip list](references/skip-list.md) and follows symlinked folders by the [symlink rule](references/symlinked-folders.md). It names each skipped folder it found on a last `Skipped N folder(s):` line (the `skipped` list in `--json`); if it names a folder holding real source, report it. A folder marked `(already scanned)` was scanned once and is not lost source.
5. For each FAIL, recommend the cheaper fix first:
   - CC is the driver (high CC, low coverage): reduce CC — extract sub-functions
   - Coverage is the driver (low coverage, moderate CC): add tests

## TypeScript coverage matching

Functions that start on the same line are matched to their Istanbul entries by the [shared start line rule](references/shared-start-line.md), and coverage is still counted by line. When a figure for such a function looks too high or too low, check for a neighbour function on its line before adding tests.

## Adopting the gate in a repo with existing debt

The baseline needs coverage data; `crap` refuses `--baseline` with `--no-coverage` (exit 2).

1. Record the debt once: `crap --lang python <dirs> --coverage-json coverage.json --baseline crap-baseline.json --update`. Commit the file.
2. Add a test that runs after coverage is collected and calls `crap.main([...same flags without --update...])`, asserting `SystemExit` code 0, so CI enforces it.
3. Baseline keys follow the [baseline ratchet](references/baseline-ratchet.md) and the [baseline key rule](references/baseline-keys.md): a file behind a symlinked folder is keyed by its real path. Functions are named by the [function naming rule](references/function-names.md), and a name that repeats in a file gets its order as a suffix by the [repeated-names rule](references/repeated-names.md).
4. A new or worse function fails; a lowered score fails until `--update` records it. `--update` never raises a score or adds a function.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | All functions pass |
| 1 | One or more FAILs |
| 2 | Tool error (missing dep, bad path, empty coverage path, unreadable coverage file) |

## Dependencies

```bash
pip3 install coverage               # Python coverage; radon and lizard ship with quality-gates
xcode-select --install               # xcrun xccov (Swift --xcresult coverage only; SwiftPM's --llvm-cov-json needs nothing extra)
```

## Installing the commands

`cc-check`, `crap` and `comment-debt` come from the quality-gates Python package. If one is missing, install with the install line in `/setup-standards`.
