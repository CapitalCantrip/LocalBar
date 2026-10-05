---
name: crap
description: >
  Quality gate for Python, Swift, or Rust: CRAP scores (CRAP > 8 = FAIL) for
  Python/Swift; cognitive-complexity gate via cargo clippy (CC > 10 = FAIL) for
  Rust. Use when asked about complexity, coverage, or whether code passes the gate.
---

# /crap — Quality Gate

**Python / Swift** — CRAP score: `CRAP(f) = CC(f)² × (1−cov)³ + CC(f)`.
Gate: CRAP > 8 = FAIL. Fully-covered collapses to CRAP = CC; uncovered scores CC² + CC.

**Rust** — no CRAP formula; `cargo clippy` enforces cognitive complexity directly.
Gate: CC > 10 = FAIL, function > 25 executable lines = FAIL.

## Invocation

```
/crap [--lang python|swift|rust] [paths] [coverage options]
```

## Steps

1. Detect language from project context (`Cargo.toml` → Rust; `.swift` files → Swift; else Python).
2. **Rust** — no coverage file needed; proceed to step 3.
   **Python** — look for `coverage.json` (`coverage run -m pytest && coverage json`).
   **Swift** — look for `.xcresult` bundle (`xcodebuild test --resultBundlePath ...`).
3. Run the gate:

   **Rust:**
   ```bash
   cargo clippy --all-targets 2>&1
   ```
   Report every `cognitive_complexity` and `too_many_lines` violation: file, function, score.

   **Python — with coverage:**
   ```bash
   coverage run -m pytest && coverage json
   crap --lang python <dirs> --coverage-json coverage.json
   ```

   **Python — no tests yet:**
   ```bash
   crap --lang python <dirs> --no-coverage
   ```

   **Swift — with coverage:**
   ```bash
   crap --lang swift Sources/ --xcresult /tmp/out.xcresult
   ```

   **Swift — worst-case** (SwiftPM packages: `swift test --enable-code-coverage` does not produce an `.xcresult`, so this is the only option):
   ```bash
   crap --lang swift Sources/ --no-coverage
   ```

   **JSON output (Python/Swift, for CI):**
   ```bash
   crap --lang python <dirs> --coverage-json coverage.json --json
   ```

4. With `--no-coverage`, every function at CC 3 or above scores FAIL. That mode is a risk ranking, not a gate: report the top rows and say no coverage was available, never "the gate failed".
   With coverage, report all FAIL rows in full; summarise WARN rows (CRAP 5–8 for Python/Swift; CC 11–15 for Rust).
5. For each FAIL, recommend the cheaper fix first:
   - CC is the driver (high CC, low coverage): reduce CC — extract sub-functions
   - Coverage is the driver (low coverage, moderate CC): add tests
   - Rust: extract private helpers; flatten control flow with early `return` or `?`

## Adopting the gate in a repo with existing debt (Python / Swift)

The baseline needs coverage data; `crap` refuses `--baseline` with `--no-coverage` (exit 2).

1. Record the debt once: `crap --lang python <dirs> --coverage-json coverage.json --baseline crap-baseline.json --update`. Commit the file.
2. Add a test that runs after coverage is collected and calls `crap.main([...same flags without --update...])`, asserting `SystemExit` code 0, so CI enforces it.
3. A new or worse function fails; a lowered score fails until `--update` records it. `--update` never raises a score or adds a function.

## Exit codes (Python / Swift)

| Code | Meaning |
|------|---------|
| 0 | All functions pass |
| 1 | One or more FAILs |
| 2 | Tool error (missing dep, bad path) |

## Dependencies

```bash
pip3 install radon lizard coverage   # Python CC + coverage; lizard for Swift CC
xcode-select --install               # xcrun xccov (Swift coverage, optional)
rustup component add clippy          # Rust
```

## Installing the commands

`cc-check`, `crap` and `comment-debt` come from the quality-gates Python package. If one is missing:

```bash
uv tool install "quality-gates @ git+https://github.com/CapitalCantrip/quality-gates@v0.4.0"
# or, inside a project venv:
pip install "quality-gates @ git+https://github.com/CapitalCantrip/quality-gates@v0.4.0"
```
