# PR #14 review and fix report

- **Review:** PR #14 review 5222188485 (https://github.com/CapitalCantrip/LocalBar/pull/14#pullrequestreview-5222188485), 16 inline findings
- **Fix branch:** `fix/pr-14-review` (https://github.com/CapitalCantrip/LocalBar/tree/fix/pr-14-review)
- **Date:** 2026-09-17

## Findings and fixes

Proof lines refer to `fix/pr-14-review` at commit `0174118`.

| # | Finding (PR #14 anchor) | Status | Proof |
|---|---|---|---|
| 1 | `launch_instance` ignores `manages_lifecycle()`; External instances spawn `""` (lib.rs:207) | Fixed | `localbar-tauri/src/lib.rs:232` gates `!driver.manages_lifecycle()` after the adopt and port-conflict checks and returns before `launch()` |
| 2 | Stop marks adopted instances Stopped while the server keeps running (lib.rs:358) | Fixed | `lib.rs:408-413`: with no owned child, `do_stop` re-runs `health_check` in `spawn_blocking` and returns Err if still Healthy; `lib.rs:432` reverts the phase to Running |
| 3 | Sync `start_instance` / `stop_instance` block the main thread on network I/O (lib.rs:329) | Fixed | `start_instance` is `async` (`lib.rs:381`) and runs `launch_instance` via `spawn_blocking` (`lib.rs:394`); `stop_instance` is `async` (`lib.rs:420`) and kills via `spawn_blocking` (`lib.rs:401`). `health_check`, `launch`, and `detect_port_conflict` all execute inside `launch_instance`, so they are on the blocking pool |
| 4 | Any read error becomes an empty `StorageFile`, then the next write clobbers the file (persistence.rs:81) | Fixed | `localbar-core/src/persistence.rs:81-82`: only `NotFound` maps to default, other errors propagate. New test `file_persistence_error_on_corrupt_json` at line 163 |
| 5 | Corrupt state.json is swallowed at startup (lib.rs:47) | Fixed | `load()` result captured at `lib.rs:539-540` into `AppState::load_error`; logged and emitted as `startup-error` at `lib.rs:478-481`. No frontend listener for `startup-error` yet |
| 6 | `detect_port_conflict` repeats the health check (lib.rs:184) | Fixed | `lib.rs:210` takes `health: &HealthStatus`; computed once at `lib.rs:222`, passed at `lib.rs:228` |
| 7 | Blocking ureq health check inside async `run_health_poll` (lib.rs:163) | Fixed | `lib.rs:188` wraps `check_health_once` in `spawn_blocking` |
| 8 | `processes` mutex held across `child.kill()` / `wait()` (lib.rs:351) | Fixed | Child is removed in a standalone statement so the guard drops before kill: `lib.rs:426` (stop) and `lib.rs:343` (remove) |
| 9 | `ShutdownPlan` grace period never used; every stop is SIGKILL (lib.rs:353) | Fixed | `graceful_kill` (`lib.rs:114`) sends SIGTERM at 118, polls `try_wait` until the deadline at 122, then `kill()` at 131. `stop_instance` derives grace from `driver.stop(&c).grace_period_secs` at `lib.rs:428`; `remove_instance` does the same at `lib.rs:346` and passes it at `lib.rs:349` (fixed in 8ce8c7b) |
| 10 | Start then Stop in quick succession can orphan the spawned child (lib.rs:341) | Fixed | `lib.rs:252-259`: after spawn, phase is re-checked under the registry lock; if no longer Starting the child is killed before insert |
| 11 | CI never runs `cargo clippy` (ci.yml:59) | Fixed | `.github/workflows/ci.yml:58-59` adds `cargo clippy --all-targets --workspace -- -D warnings` |
| 12 | `on_startup` auto-launches without the `start_warning` gate (lib.rs:398) | Deferred to #15 | Gate intentionally bypassed for now; comment at `lib.rs:483-486` states the memory risk and points at #15 (startup restore chooser) |
| 13 | MLX health URL bypasses `base_url()` (mlx_lm.rs:76) | Fixed | `localbar-core/src/drivers/mlx_lm.rs:75` uses `super::http::base_url(config)` |
| 14 | `quick_agent()` triplicated; Ollama copy lacks connect timeout (ollama.rs:111) | Fixed | `localbar-core/src/drivers/http.rs:10-22`: single `quick_agent` and `load_agent`, both with a 5 s connect timeout; private copies removed from all three drivers |
| 15 | Polling effect and start-warning flow duplicated across PopoverApp and SettingsApp (PopoverApp.tsx:128) | Fixed | `useInstances` hook (`localbar-tauri/ui/src/useInstances.ts`) and `startWithWarnings` (`ui/src/startWithWarnings.ts`) used by both apps (`PopoverApp.tsx:117,124`, `SettingsApp.tsx:226,153`). `useInstances` takes an `onRemoved` callback (`useInstances.ts:6,25-27`) and SettingsApp passes `() => setSelectedId(null)` (`SettingsApp.tsx:226`), restoring the selection reset on removal (8ce8c7b) |
| 16 | `ParamValues::resolve` has no production caller (lib.rs:208) | Fixed (documented) | Comment at `lib.rs:262` states resolution is scaffolded for T8+ |

## Other checks

- `Cargo.lock` is committed (8ce8c7b) and lists `libc` under `localbar-tauri` (`Cargo.lock:1805`).
- `.vscode/` appears in no commit on the branch; it is untracked only.
- Health poll cadence is unchanged at 500 ms (`lib.rs:179`). No review comment requested a change.

## Verification

Run on `fix/pr-14-review` at `0174118` with both crate roots touched to force a fresh check.

```
$ cargo clippy --all-targets --workspace -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.47s
exit 0

$ cargo test --workspace
localbar-core:  test result: ok. 63 passed; 0 failed; 0 ignored
localbar-tauri: test result: ok. 0 passed; 0 failed (lib, main, and 2 more targets)
exit 0

$ npx tsc --noEmit -p .   (in localbar-tauri/ui)
exit 0
```

## Commits on the fix branch

```
0174118 docs: correct on_startup auto-start comment
8ce8c7b fix: apply driver grace period in remove_instance; restore selectedId reset on removal
d4f3fff refactor(ui): extract useInstances hook and startWithWarnings helper
601ccba ci: add cargo clippy step to enforce complexity gate
7c29e78 refactor: consolidate HTTP agent builders in drivers/http.rs
340d0c1 fix: surface load errors, gate ExternalDriver launch, async stop/start
752444f fix: propagate non-NotFound errors in FilePersistence::read
```

**Addendum (a278e12):** `SettingsApp` now wraps the `onRemoved` callback in `useCallback(() => setSelectedId(null), [])` (`SettingsApp.tsx:227`), so `useInstances` no longer re-registers its interval and listeners every render; clippy, `cargo test` (63 passed) and `tsc` re-run clean at a278e12.
