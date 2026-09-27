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

---

## Addendum (2026-09-27, #61): quit behaviour in the Tauri rewrite

- On quit, LocalBar stops only the servers it spawned (those with a `Child` in `AppState.processes`, whatever their phase, so Starting and SwitchingModel are included). Adopted servers (`adopted_pids`) and External instances are left running, because LocalBar did not start them.
- `was_running_when_quit` is recorded before any stop, so the next launch restarts spawned servers and re-adopts the rest.
- The app-level setting `AppSettings.keep_servers_running_on_quit` (persisted under `settings` in state.json, default off) turns the stop off entirely.
- The decision is the pure fn `localbar_core::quit::ids_to_stop_on_quit`. The Tauri shell calls `prevent_exit`, runs `graceful_kill` for each server on its own thread, waits at most `quit_shutdown_budget_secs` (largest grace capped at `MAX_QUIT_GRACE_SECS`, plus `QUIT_KILL_SLACK_SECS`), then calls `exit(0)`. The `AppState.quitting` flag lets that second `ExitRequested` pass straight through. A kill that hangs past the budget cannot block quit.

## Addendum (2026-09-28, #82): restoring servers on launch

- `mark_running_instances_for_reconnect` always records `was_running_when_quit`, independent of any setting — the flag is a fact about what was running, not a decision about what to do with it.
- `AppSettings.restore_running_servers_on_launch` (default true) is the decision: whether `was_running_when_quit` instances relaunch on the next startup. `start_on_launch` instances launch regardless of this setting.
- The decision is the pure fn `localbar_core::quit::ids_to_start_on_launch`, called from `on_startup` in the Tauri shell.
