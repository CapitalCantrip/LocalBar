# ADR D12 — Lifecycle logic lives in `localbar-core`, not the Tauri layer

**Status:** Accepted  
**Date:** 2026-09-22  
**Deciders:** Jay

---

## Context

After the Tauri rewrite (D-G), all instance lifecycle logic — starting, stopping, model switching, health polling, and external server adoption — lived in `localbar-tauri/src/lib.rs`. That file exceeded 1 200 lines, with three distinct concerns entangled:

1. **Pure lifecycle state-machine logic** — phase transitions, port-conflict detection, startup timeouts, adoption heuristics, model-switch rollback.
2. **Process I/O** — spawning `Child` processes, reaping exit status, issuing graceful kills.
3. **Tauri glue** — `AppHandle` event emission, Tauri `State` locking, IPC command handlers.

Concerns 1 and 3 had no test coverage, because the only seam available was the IPC command boundary — too wide to target lifecycle transitions directly.

---

## Decision

**Extract concern 1 into `localbar-core/src/lifecycle.rs` as a deep module with six free functions.**

```rust
// localbar-core/src/lifecycle.rs

pub enum LifecycleEvent { PhaseChanged(Uuid, InstancePhase) }
pub struct StopPlan      { pub grace_secs: f64 }
pub enum PollContext      { Startup, ModelSwitch { old_key: Option<String> } }
pub enum PollOutcome      { Continue, Done }

/// Precondition: id is in Starting phase, start_time recorded in registry.
/// Returns a LaunchPlan when the Tauri layer must spawn a process; None on
/// adoption (phase → Running), port conflict, or error (phase → Error).
pub fn start(reg: &mut InstanceRegistry, id: Uuid, driver: &dyn ServerDriver)
    -> (Option<LaunchPlan>, Vec<LifecycleEvent>);

/// Sets Stopping; returns the driver's grace period for the Tauri layer to use.
pub fn stop(reg: &mut InstanceRegistry, id: Uuid, driver: &dyn ServerDriver)
    -> (StopPlan, Vec<LifecycleEvent>);

/// Sets SwitchingModel and records start time. For restart drivers returns a
/// LaunchPlan; for sync drivers completes the switch inline (phase → Running).
pub fn switch_model(reg: &mut InstanceRegistry, id: Uuid, driver: &dyn ServerDriver)
    -> (Option<LaunchPlan>, Vec<LifecycleEvent>);

/// Sets Running for an already-running adopted/external instance.
pub fn adopt(reg: &mut InstanceRegistry, id: Uuid) -> Vec<LifecycleEvent>;

/// Synchronous poll tick for startup and model-switch polls.
/// On Done + health=true: phase → Running, model memory and restart duration recorded.
pub fn poll_once(
    reg: &mut InstanceRegistry, id: Uuid, driver: &dyn ServerDriver,
    health: bool, process_alive: bool, elapsed: Duration, ctx: PollContext,
) -> (PollOutcome, Vec<LifecycleEvent>);

/// Ongoing health monitoring for adopted/external instances.
/// Running → Error on health=false; Error → Running on health=true.
pub fn poll_adopted(reg: &mut InstanceRegistry, id: Uuid, health: bool)
    -> Vec<LifecycleEvent>;
```

`InstanceRegistry` gains two new methods to support timing:

```rust
pub fn record_start_time(&mut self, id: Uuid);
pub fn consume_start_time(&mut self, id: Uuid) -> Option<std::time::Instant>;
```

`start_times` moves out of `AppState` (Tauri layer) into `InstanceRegistry` (core) so that `poll_once` can record the startup duration sample without needing `AppHandle`.

---

## Key invariants

| Invariant | Reason |
|---|---|
| No `Child`, PID, or process handle in `lifecycle.rs` | Process I/O stays in the Tauri layer. Core cannot link against `std::process::Child`-dependent Tauri types. |
| No `AppHandle` in `lifecycle.rs` | Events are returned as `Vec<LifecycleEvent>` data; the Tauri layer emits them. This is the sole mechanism for notifying the frontend. |
| Blocking network I/O is permitted in `lifecycle.rs` | `health_check`, `switch_model`, and `port_is_open` are short-lived TCP operations already present in core drivers. The Tauri layer wraps calls to lifecycle in `spawn_blocking`. |
| Phase transitions are atomic with registry mutation | All phase writes go through `reg.set_phase`; the `Vec<LifecycleEvent>` returned mirrors those writes exactly. |

---

## What stays in the Tauri layer

- Spawning and reaping `Child` processes (`spawn_from_plan`, `graceful_kill`, `process_is_alive`).
- Async poll loops (`run_health_poll`, `run_restart_switch_poll`, `run_adopted_health_poll`) — thin loops that call `lifecycle::poll_once` / `lifecycle::poll_adopted` in `spawn_blocking`.
- `AppHandle` event emission via `emit_lifecycle_events`.
- IPC command guards (e.g. "already Starting → noop").
- Tray sync and `AppState` locking.

---

## Consequences

- Lifecycle state-machine logic becomes testable through the `InstanceRegistry` seam without needing a running Tauri app.
- `localbar-tauri/src/lib.rs` loses ~300 lines and becomes dispatch + I/O only.
- C2 (collapse driver factory) and C3 (collapse Persistence to 2 methods) can follow independently.
- The `start_times` field is removed from `AppState`; callers that previously accessed `state.start_times` now use `reg.record_start_time` / `reg.consume_start_time`.

---

## Alternatives considered

**ProcessSpawner seam (originally proposed):** Inject a `ProcessSpawner` trait into `lifecycle.rs` to keep spawning behind a seam. Rejected: spawning is not the thing being tested; the phase-transition logic is. Adding a seam here would be a hypothetical seam (only one adapter ever), and it would complicate the interface without improving leverage. The simpler contract — "return a plan, caller spawns" — achieves the same testability with half the surface area.

**Async lifecycle functions:** Allow lifecycle functions to `await` health checks. Rejected: drivers already do blocking HTTP calls today; keeping lifecycle synchronous preserves the existing model and avoids async executor coupling in `localbar-core`.
