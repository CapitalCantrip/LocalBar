---
name: cc-rust
description: >
  Rust complexity gate via cargo clippy (cognitive complexity > 10, functions > 25 lines); `setup` scaffolds clippy.toml and deny attributes.
---

# /cc-rust — Rust Complexity Check & Quality Gate

Audits Rust source files for cognitive complexity and function-length violations
using `cargo clippy`, and optionally scaffolds the project-level gate.

## Invocation

```
/cc-rust          # audit current project
/cc-rust setup    # scaffold clippy.toml + deny attributes (one-time, per project)
```

## Steps — audit

1. Check for `clippy.toml` in the project root. If absent, note it and recommend
   running `/cc-rust setup` before the next coding session.
2. Run:
   ```bash
   cargo clippy --all-targets 2>&1
   ```
3. Report every `cognitive_complexity` and `too_many_lines` violation: file, function
   name, score. All violations shown — none suppressed.
4. For each violation propose a concrete refactor:
   - Extract private helper functions for the nested logic block
   - Flatten control flow with early `return` or `?` operator
   - Replace nested `if let` chains with a flat `match` or `Option`/`Result` combinator

## Steps — setup

1. Write `clippy.toml` to the project root:
   ```toml
   cognitive-complexity-threshold = 10
   too-many-lines-threshold = 25
   ```
2. Add deny attributes to `src/lib.rs` (prefer) or `src/main.rs`:
   ```rust
   #![deny(clippy::cognitive_complexity)]
   #![deny(clippy::too_many_lines)]
   ```
3. Run `cargo clippy --all-targets` to confirm the gate is wired. Report any
   violations that now block compilation — those are the first refactor targets.

## Thresholds

| Cognitive complexity | Action |
|---|---|
| ≤ 10 | ok |
| 11–15 | schedule refactor |
| > 15 | refactor before next commit |

Function length > 25 executable lines (excluding comments and attributes): schedule refactor.

## Architectural constraints — apply when writing new Rust in this session

- Maximum cognitive complexity: 10
- Maximum function length: 25 executable lines
- Nesting: ≤ 2 levels of `if` / `if let` / `loop`
- Flatten control flow: early `return`/`?` and flat `match` over nested branches
- Deconstruct before implementing: break multi-step transforms into private
  single-responsibility helpers before writing the main function body

## Dependencies

```bash
rustup component add clippy   # ships with rustup; re-run if missing
```
