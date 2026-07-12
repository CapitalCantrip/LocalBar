# ADR D10 — Model scan path persistence

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

When adding an mlx-lm instance, the user must specify a directory to scan for model files. The default path is `$HF_HOME` or `~/.cache/huggingface/hub`. In practice, users may store models in a custom directory (e.g. `~/SharedModels/`), which differs from the HF default. Without persistence, the user must re-enter or re-select the path every time the Add Instance sheet opens — even within the same session.

---

## Decision

**Persist the model scan path in `UserDefaults` via `@AppStorage("mlxModelSearchPath")`.**

The default value at first launch is `$HF_HOME ?? ~/.cache/huggingface/hub`. Once the user selects a different path (either by typing or using the folder picker), the choice is remembered across sheet opens and app restarts.

This is scoped to a single key; per-instance scan paths (allowing different instances to scan different directories) are deferred post-MVP.

---

## Consequences

- The most common user friction (re-selecting the model folder on every Add) is eliminated.
- The path is global, not per-instance. If the user has mlx-lm models in multiple locations, they must re-scan for each add or manually type the alternative path. A future improvement would be a list of search paths rather than a single value.
- `UserDefaults` is appropriate here (small string, user preference, not sensitive). No keychain or file bookmark needed — the path is used to start an `NSFileManager.enumerator`, which does not require a security-scoped bookmark for paths the user owns in their home directory. If the app is ever sandboxed for App Store distribution, this will need to change to a security-scoped bookmark stored in `UserDefaults` as `Data`.
