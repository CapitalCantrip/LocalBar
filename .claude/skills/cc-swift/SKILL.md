---
name: cc-swift
description: >
  Swift complexity check via SwiftLint and crap --lang swift (lizard); flags functions above the threshold and proposes refactors.
---

# /cc-swift — Swift Cyclomatic Complexity Check

Runs SwiftLint (if configured) and/or `crap --lang swift` (lizard) against
Swift source files and reports functions above the complexity threshold.

## Invocation

```
/cc-swift [Sources dir]
```

## Steps

1. Locate the Swift source directory (typically `Sources/`)
2. **If a `.swiftlint.yml` is present**, run SwiftLint first:
   ```bash
   swiftlint lint --reporter emoji
   # or if Makefile has the target:
   make lint-swift
   ```
3. **Always** run lizard for raw CC numbers:
   ```bash
   crap --lang swift <Sources dir> --no-coverage
   ```
   (Use `--xcresult <path>` instead of `--no-coverage` when an `.xcresult` bundle is available)
4. Report SwiftLint warnings (CC > 6) and errors (CC > 8). With `--no-coverage`, lizard's CRAP column is a worst-case ranking, not a gate: every function at CC 3 or above "fails" it
5. For each violation propose: extract helper functions, break up long `switch` bodies, split large `body` computed properties into `@ViewBuilder` helpers

## Thresholds

| CC  | SwiftLint rule | Action |
|-----|----------------|--------|
| ≤ 6 | ok | none |
| 7–8 | warning | note it; refactor if you are in the function anyway |
| > 8 | error — blocks build | fix now |

The project's `.swiftlint.yml` should hold:

```yaml
cyclomatic_complexity:
  warning: 6
  error: 8
  ignores_case_statements: true
```

`ignores_case_statements: true` stops an exhaustive `switch` over an enum from
counting one per case (ADR-001). If a project's file disagrees, report it.

## Architectural constraints — apply when writing new Swift in this session

- Maximum CC: 8 (agent ceiling, ADR-001)
- Deconstruct before implementing: break multi-step logic into private helpers before writing the main function body
- Flatten control flow: guard/early return over nested `if` branches
- SwiftUI views: split large `body` computed properties into focused `@ViewBuilder` helpers; break long `switch` arms into named view functions

## Dependencies

```bash
brew install swiftlint          # CC linting
pip3 install lizard             # CC measurement (used by crap)
xcode-select --install          # xcrun xccov for coverage (optional)
```

## Installing the commands

`cc-check`, `crap` and `comment-debt` come from the quality-gates Python package. If one is missing:

```bash
uv tool install "quality-gates @ git+https://github.com/CapitalCantrip/quality-gates@v0.4.0"
# or, inside a project venv:
pip install "quality-gates @ git+https://github.com/CapitalCantrip/quality-gates@v0.4.0"
```
