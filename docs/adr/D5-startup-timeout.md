# ADR D5 — Server startup timeout: 120s default, per-instance override

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

After `ServerInstanceController` spawns the server process, it polls health until the first successful probe or the startup timeout elapses. The timeout must balance two failure modes:

- **Too short:** large models (70B+, or models loaded from a cold HF cache over slow storage) can take well over a minute to be ready. A short timeout causes false `error(.healthCheckFailed)` during legitimate startup, frustrating users with large models.
- **Too long:** a genuinely broken launch (wrong binary path, OOM on model load, port conflict not caught pre-flight) leaves the UI stuck in `starting` state for the full timeout before surfacing an error.

Relevant data points: mlx-lm loading a 70B 4-bit model on M2 Ultra from NVMe takes ~40–90 s depending on thermal state. Ollama loading a pulled model is typically faster (10–30 s) but can be slow on first load from cold cache. The absolute worst case observed in testing is ~110 s for a large model from cold storage under memory pressure.

---

## Decision

**120 seconds default startup timeout, overridable per instance in advanced flags.**

The default covers observed worst cases with a safety margin. Per-instance override is exposed as a `FlagDescriptor` under the advanced flags section (integer, seconds, min 30, no enforced max — power users with very large models or slow storage may need 300s+).

On timeout expiry: `ServerInstanceController` executes the `ShutdownPlan` (best-effort graceful → SIGTERM → SIGKILL) to avoid leaving an orphaned partially-loaded process, then transitions to `error(.healthCheckFailed)` with a message that explicitly names the timeout duration: *"Server did not become healthy within 120 s. If you are loading a large model, increase the startup timeout in advanced settings."*

---

## Consequences

- `ServerInstanceConfig.advancedFlags` stores the override keyed by the flag name `"startupTimeoutSeconds"`.
- `FlagDescriptor` for `startupTimeoutSeconds` is defined in the base `ServerDriver` default implementation (or a shared utility), not per-driver — it is a control-plane concern, not a server-specific one. Drivers that need a different default can override it.
- The error message copy should cite the actual configured timeout (not hardcoded "120 s") so users with custom values get accurate guidance.
- Post-MVP, startup timeout could be informed by `ModelMemory.restartDurationSamples` — if observed switch durations approach the configured timeout, surface a proactive warning in the UI. Not implemented in MVP.
