# ADR D8 — Concurrent server instances: allow with resource warning

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

LocalBar supports multiple configured instances (different ports, different models, different server types). Nothing prevents a user from starting two or more simultaneously. On Apple Silicon, each loaded MLX model occupies a significant portion of unified memory (a 9B 4-bit model ~5 GB, a 27B ~14 GB). Running two large models concurrently can exhaust memory, cause thrashing, and degrade both models' performance.

The question is whether LocalBar should prevent, warn, or silently allow concurrent starts.

**Options considered:**

1. **Prevent** — enforce a single running instance. Simple, but unnecessarily restrictive: a user may legitimately run a small embedding model alongside a large chat model, or run Ollama alongside mlx-lm.
2. **Silent allow** — no friction. Matches the user's explicit action but provides no guard against accidental double-start.
3. **Warn before starting a second running instance** — non-blocking confirmation dialog that describes the risk and names the already-running instance(s). User can proceed or cancel.

---

## Decision

**Allow concurrent servers; warn before starting a second one.**

When `start()` is called and `registry.hasAnyRunning` is true, the UI presents a confirmation alert naming the running instance(s) and noting the memory cost. The user can proceed or cancel. No restriction is imposed at the controller layer — the decision is purely a UI-layer gate.

Implementation note: the warning is not yet built (as of D8 acceptance). The controller layer accepts any `start()` call; the UI guard is the intended location for the alert.

---

## Consequences

- Power users (embedding model + chat model) are not blocked.
- Accidental double-starts are caught before they consume resources.
- The warning must name running instances and ideally estimate combined memory use — requires surfacing model parameter counts and quantization from `ModelMetadata`, which is already parsed.
- The controller layer has no enforcement; a caller bypassing the UI (e.g. `startOnAppLaunch` auto-start) will not trigger the warning. This is acceptable for auto-start scenarios where the user explicitly configured both instances to start.
