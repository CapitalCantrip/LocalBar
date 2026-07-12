# ADR D6 — External server adoption (launchd / script-managed processes)

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

LocalBar's original design assumed it owns every inference server process it manages — it spawns the process and holds the `Process` handle. In practice, an mlx-lm instance managed by launchd via a LaunchAgent plist and a companion switch script. When LocalBar tries to start a server on port 8080, the port is already occupied, the spawn fails silently, and the startup timeout fires after 120 s.

Three responses were considered:

1. **Refuse to manage externally-started servers** — simplest, but breaks the primary real-world workflow where the user already has a running model.
2. **Kill the external process and replace it** — destructive, surprising, loses any an external switch script state.
3. **Adopt the running process without spawning** — connect to the healthy server, track its PID via `lsof`, stop it with signals when asked. LocalBar gets full management without interfering with the startup path.

---

## Decision

**Adopt on health-check success.** Before spawning, `performStart()` calls `driver.healthCheck()`. If the server is already healthy on the configured port, LocalBar skips spawning and instead:

- records the PID via `lsof -t -i :<port> -sTCP:LISTEN` (stored as `adoptedPID`)
- queries `/v1/models` to identify the loaded model (`detectRunningModel()`)
- transitions to `.running` and begins health + context polling

`adoptIfRunning()` is also called:
- at app launch (for any pre-configured instances once persistence is implemented)
- in `addInstance()` (so a newly-added instance immediately adopts if the port is already live)
- when the Settings screen opens (catches servers started after LocalBar launched)

**Stop path for adopted servers:** `performStop()` checks `process == nil`. If so, it resolves the PID from `adoptedPID` or re-queries `lsof`, then escalates: SIGINT → wait grace period → SIGTERM → 500 ms → SIGKILL.

---

## Consequences

- LocalBar works with launchd-managed, script-managed, and manually-started inference servers without requiring the user to route everything through LocalBar.
- The `adoptedPID` handle is weaker than a `Process` handle — if the external manager (launchd, an external switch script) restarts the process, the PID changes and LocalBar's health poll will eventually detect the restart (though the PID record becomes stale until the next health check cycle).
- `portOccupant()` is currently a stub (always nil). A proper implementation using `lsof` would let us detect port conflicts before spawning and give a better error message.
- Process ownership is now ambiguous: LocalBar can stop an adopted server, but it did not start it — launchd may restart it automatically. This is surfaced in the UI implicitly (the instance will transition back to Running after the next health poll cycle), but no explicit "managed by launchd" label exists yet.
