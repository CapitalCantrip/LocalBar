# ADR D3 — Server process lifetime: children die with LocalBar

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

LocalBar spawns inference servers as child processes using `Process` (wrapping `posix_spawn`). macOS does not automatically kill children when a parent exits, but `Process`-spawned children do receive `SIGHUP` when the parent terminal (if any) closes — and for a menu bar app there is no terminal, so children may outlive LocalBar depending on how each server handles signal disposition.

Two lifecycle models are possible:

**Option A — Children die with LocalBar (or are killed on quit)**  
LocalBar installs a `terminationHandler` that kills all managed processes on app quit/crash. Servers cannot outlive their manager. If LocalBar crashes, servers go down too.

**Option B — Servers outlive LocalBar (persist across crashes/restarts)**  
Servers are launched as detached daemons or launchd agents. LocalBar reattaches on restart via PID file or port re-probe. More resilient but significantly more complex: PID-file management, reattach logic, stale-PID detection, lifecycle ownership questions ("who started this — launchd or LocalBar?").

The architecture doc explicitly flags this as a hard fork in the process model.

---

## Decision

**Option A for MVP — managed servers die with (or are killed by) LocalBar.**

On app quit (`applicationShouldTerminate`), LocalBar executes each controller's `ShutdownPlan` in parallel before returning `.terminateNow`. On unexpected crash, the OS delivers `SIGHUP` to any surviving children — server processes that handle `SIGHUP` will exit; those that don't will be orphaned. In practice, both mlx-lm (Python) and Ollama respond to `SIGHUP` with exit.

Rationale:
- The locked product decision is "LocalBar is an active controller" — a server that continues running when its controller is gone is inconsistent with that model.
- Reattach/PID-file logic is substantial complexity that buys little for solo Mac users who have LocalBar in login items anyway.
- Users who want servers to persist across reboots use `startOnAppLaunch: true` — LocalBar starts the server fresh each time, which is simpler and more predictable than reattaching to a server of unknown state.

---

## Consequences

- `ServerInstanceController` sends shutdown plans to all managed processes in `applicationWillTerminate` / `applicationShouldTerminate`.
- A LocalBar crash that kills mid-generation is a known limitation of MVP — acceptable given the target audience (developers who understand local LLM tooling).
- `startOnAppLaunch` + LaunchAgent for LocalBar itself (`launchd` keeping LocalBar alive) is the recommended pattern for users who want persistent availability.
- Post-MVP, if demand exists for server persistence across LocalBar restarts, `LifecycleOwnership.attached` (sketched in the architecture doc) provides a clean extension path without changing the driver protocol or the existing managed path.
- The architecture doc note requesting an *explicit product call* on this question is hereby resolved.
