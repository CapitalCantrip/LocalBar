---
name: cc-typescript
description: >
  Cyclomatic complexity check for TypeScript and JavaScript via cc-check --lang typescript (lizard); flags functions above CC 8 and proposes refactors. Use when asked to check TypeScript or JavaScript complexity.
---

# /cc-typescript — TypeScript and JavaScript Cyclomatic Complexity Check

Runs `cc-check --lang typescript` against `.ts`, `.tsx`, `.js`, `.jsx`, `.cjs` and
`.mjs` files and reports functions above the complexity threshold. lizard counts, by the
[TypeScript counting rule](references/typescript-counting.md).

## Invocation

```
/cc-typescript [path] [--threshold N]
```

- `path` — file or directory (default: `src`)
- `--threshold N` — flag CC > N (default: 8). A project may pass a lower number, never a higher one

## Steps

1. Identify the source directories. The tool skips the folders on the [skip list](references/skip-list.md) and follows symlinked folders by the [symlink rule](references/symlinked-folders.md). It names each skipped folder it found on a last `Skipped N folder(s):` line; if that line names a folder holding real source, report it. A folder marked `(already scanned)` was scanned once and is not lost source.
2. Run:
   ```bash
   cc-check --lang typescript <dirs> --threshold 8
   ```
   If the repo has a CC baseline (e.g. `cc-baseline.json`), add `--baseline <file>`: it then fails only on new or worse functions.
   Use `cc-check` as the gate rather than ESLint's `complexity` rule, so CC and CRAP share one counter.
3. Report every flagged function with file, line and CC.
4. For each violation propose a concrete refactor: extract named helpers, replace `switch` or `if/else if` chains with a lookup object, return early instead of nesting.

Done when every flagged function is reported with a refactor, or the run is clean.

## What counts

The [TypeScript counting rule](references/typescript-counting.md) says what lizard counts, including default values.

Functions that start on the same line, such as a callback in the first line of its holder, are each counted on their own. `crap` matches their coverage by the [shared start line rule](references/shared-start-line.md); `cc-check` is unaffected.

## Baseline keys

A name that repeats in a file is keyed with its order, `path::(anonymous)#2`, by the [repeated-names rule](references/repeated-names.md). A file behind a symlinked folder is keyed by its real path, by the [baseline key rule](references/baseline-keys.md).

## Dependencies

lizard ships with quality-gates. No Node packages are needed for CC.

## Installing the commands

`cc-check`, `crap` and `comment-debt` come from the quality-gates Python package. If one is missing, install with the install line in `/setup-standards`.
