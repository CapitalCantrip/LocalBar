# ADR D11 — Model scan path persistence (Tauri era)

**Status:** Accepted  
**Date:** 2026-09-20  
**Deciders:** Jay  
**Supersedes:** ADR D10

---

## Context

ADR D10 decided to persist the mlx-lm model scan path in `UserDefaults` via
`@AppStorage("mlxModelSearchPath")`. That decision was made for the Swift
prototype. The Swift prototype has since been replaced by a Tauri application
(`localbar-tauri`), which uses a Rust backend with its own `FilePersistence`
layer. `UserDefaults` is not accessible from Rust without additional ObjC
bridging, and the existing `FilePersistence` approach (a single `state.json`
file) already handles all other user configuration (instances, profiles, model
memory). Using a different persistence mechanism for scan paths would create an
inconsistent split.

---

## Decision

**Persist model discovery settings in `state.json` under a new top-level
`discovery` key, managed by the existing `FilePersistence` / `Persistence`
trait.**

A new `DiscoveryConfig` struct holds:
- `mlx_lm_search_paths: Vec<String>` — ordered list of directories to scan for
  HF-cached mlx-lm models. Empty means fall back to `$HF_HOME` /
  `~/.cache/huggingface/hub` at scan time (handled by `effective_search_paths`
  in the driver).
- `ollama_executable_path: Option<String>` — optional override for the ollama
  binary path; `None` means resolve via system PATH.

`ServerInstanceConfig` gains `model_search_path_override: Option<String>`, a
per-instance scan path that, when set, replaces the global list for that
specific mlx-lm instance. Merging is handled by
`DiscoveryConfig::resolved_mlx_paths(override_path)`.

The `Persistence` trait gains `save_discovery_config` / `load_discovery_config`
methods, implemented by `FilePersistence` (reads/writes `state.json`) and
`InMemoryPersistence` (for tests). `InstanceRegistry` caches the loaded config
and exposes `get_discovery_config` / `set_discovery_config` /
`load_discovery_config`.

Two new Tauri IPC commands are exposed to the frontend:
`get_discovery_config` and `set_discovery_config`.

`MLXLMDriver` is always constructed with `new(resolved_paths)` in the IPC path
— `default()` (empty paths) is never used for list-models operations.

---

## Consequences

- Single source of truth for all user configuration: `state.json`. No split
  between `UserDefaults` and the file store.
- Loading a `state.json` without a `discovery` key (i.e., all existing state
  files) produces `DiscoveryConfig::default()` (empty paths → HF cache fallback),
  preserving backward compatibility.
- Per-instance scan path override (`model_search_path_override`) addresses the
  D10 limitation of a single global path. Multiple mlx-lm instances can now
  point to different model directories.
- `FilePersistence` performs a full read-modify-write on every save, consistent
  with how it handles other config keys.
- If the app is ever sandboxed for App Store distribution, paths stored as plain
  strings will need to be replaced with security-scoped bookmarks (as noted in
  D10). That migration path is unchanged.
