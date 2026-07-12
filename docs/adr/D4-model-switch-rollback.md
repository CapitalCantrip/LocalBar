# ADR D4 — No auto-rollback on failed restart-required model switch

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

When a `restartRequired` model switch fails mid-flight (e.g. the new model fails to load, port conflict after shutdown, or mlx-lm exits with an error), the old model is already down. Two recovery paths are possible:

**Option A — Auto-rollback:** LocalBar automatically re-launches the previous model and model key, returning to the prior `running` state without user action.

**Option B — Manual rollback:** LocalBar lands in `error` state with the failed switch's details, and surfaces a prominent one-click action to "Restart with previous model." The user decides whether to retry the new model or go back.

Auto-rollback sounds user-friendly but has hidden failure modes: if the old model also fails to load (weights moved, OOM, same port still in use), the auto-rollback itself fails, leaving the user confused about what state they're in and why. A double-failure during auto-recovery is harder to diagnose than a clean error state.

---

## Decision

**Option B — no auto-rollback. Land in `error` state, offer one-click manual rollback.**

When a `restartRequired` switch fails at any step (stopping old model → launching new model → awaiting healthy), `ServerInstanceController` transitions to `error(kind: <failing step>)`. The error state includes:

- `lastAttemptedModelKey` — the model that failed to load
- `previousModelKey` — the model that was running before the switch

The UI renders a **"Restart with [previous model name]"** button in the error panel. Tapping it is equivalent to the user manually initiating a start with the previous `selectedModelKey`, going through the normal `stopped → starting → running` path with full health confirmation.

---

## Consequences

- `InstanceError` needs `lastAttemptedModelKey: String?` and `previousModelKey: String?` fields to support the rollback action in the UI.
- `selectedModelKey` on `ServerInstanceConfig` is **not** updated until a switch fully succeeds (health confirmed) — this ensures the "previous model" is always recoverable from config.
- The error message must be specific enough that users can distinguish "new model failed to load" from "old model didn't shut down cleanly" — `InstanceError.kind` and stderr capture cover this.
- Auto-rollback remains a post-MVP candidate if user feedback shows the manual step is too friction-heavy; the implementation would reuse the one-click path as the triggered path.
