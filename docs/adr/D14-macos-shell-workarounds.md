# ADR D14 — macOS shell workarounds

**Status:** Accepted
**Date:** 2026-09-27
**Deciders:** Jay

---

## Context

D13 removes rationale comments from `localbar-tauri/src/lib.rs`, keeping only `// SAFETY:`. Several of those comments justified a platform-specific workaround rather than restating what the code does. Their rationale needs a home outside the source file; this ADR is that home. This ADR supersedes the memory note `project-tauri-keyboard-native`, which held the same NSEvent rationale.

## Decision

Keep the following five workarounds, for the reasons below, without commenting on them in `lib.rs`:

1. **NSEvent local monitor for ESC and Cmd+W.** WKWebView does not deliver these key events to JS, so `install_popover_key_monitor` installs an `NSEvent` local monitor in Rust to dismiss the popover on ESC or Cmd+W. Cmd+H is deliberately not intercepted: LSUIElement apps (no Dock presence) have no meaningful "hide app" behavior, so the event is left to pass through unconsumed. The monitor is deliberately leaked for the lifetime of the app (`std::mem::forget`) rather than dropped, so it keeps intercepting events for as long as the app runs.

2. **`process::exit` on fatal tray setup failure.** `setup_handler` runs inside `applicationDidFinishLaunching`. Propagating an `Err` out of Tauri's setup callback causes Tauri to `panic!()` there, and a panic cannot unwind through the ObjC frame — it aborts the process anyway, but without a clean message. `std::process::exit(1)` is used instead for fatal tray-icon creation failures, after printing a diagnosable error to stderr.

3. **Every Ollama model list falls back to manifests on disk.** All Ollama listings (Discovery tab, instance Models list, Add Instance picker, memory warning, adopted-model correction) go through `OllamaDriver::list_models`: HTTP `/api/tags` first, which stays authoritative while the server answers; then the manifests in the resolved models folder; then the `OLLAMA_UNREACHABLE` sentinel (#83). Spawning `ollama list` (removed by #62) activated Ollama.app through macOS launch services and stole window focus from LocalBar, so the fallback reads manifest files directly instead.

   The models folder for an Ollama instance resolves in this order (`ollama::resolve_models_dir` in core): the instance's `model_search_path_override`; `DiscoveryConfig.ollama_models_dir`; `$OLLAMA_MODELS` if non-empty; the Ollama app's own configured folder, if non-empty and the directory exists (#74); otherwise `~/.ollama/models`. Blank values count as unset. The app's setting lives in `~/Library/Application Support/Ollama/db.sqlite`, table `settings`, column `models` (row `id = 1`), read by shelling out to `/usr/bin/sqlite3 -readonly "file:<path>?mode=ro" "select models from settings where id = 1"`. The database is in WAL mode and may be open by Ollama.app, so the read is read-only and never mutates it; any failure (missing db, missing table/column, non-zero exit, missing `sqlite3`) is treated as "no setting" and falls through to the next step in the precedence order. `rusqlite` is deliberately not used, to avoid taking a dependency that could lock the WAL-mode database.

   The sqlite read is process I/O, so it lives in `localbar-tauri`, which resolves the folder and passes it into `OllamaDriver::new`. Only listing call sites (`listing_driver_for`) run the full resolution; the lifecycle `driver_for`, used by health polls under the registry lock, resolves only the two explicit sources (instance override, Discovery setting) and never spawns `sqlite3`. When LocalBar spawns `ollama serve` and one of those explicit sources is set, the launch plan passes `OLLAMA_MODELS=<folder>` so the server serves what the list shows; auto-detected folders are not passed, because the server already detects them itself.

4. **Stop failures on adopted instances surface as an error badge.** When `stop_instance` cannot confirm that an adopted (unmanaged) server actually stopped, it moves the instance to `Error(StopFailed)` rather than silently reverting the phase to `Running`. A silent revert would look identical to a Stop that never happened, giving the user no indication that anything went wrong.

5. **Spawned server children get `Stdio::null()` on all three streams.** `spawn_from_plan` previously spawned with Rust's default `Stdio::inherit()`, so a launched server (e.g. `mlx_lm.server`) shared LocalBar's stdin/stdout/stderr file descriptors. mlx-lm's `ThreadingHTTPServer` logs every HTTP request — including `/health` — to stderr before the response is sent; once LocalBar quits and the inherited pipe's read end disappears, that write raises a `BrokenPipeError` in the request's handler thread, aborting the in-flight response while the process and its listening socket stay alive. The client sees a dropped/empty response even though the server is still healthy, which is exactly the shape of a `PortConflict` from `lifecycle::start`'s point of view. Explicitly redirecting all three streams to `/dev/null` removes the dependency on LocalBar's own stdio lifetime; the trade-off is that a spawned server's stdout/stderr is no longer observable by attaching to LocalBar's own streams (it was never captured or displayed anywhere, so nothing observable is lost).

## Consequences

- These five behaviors are intentional and documented here; a change to any of them should update this ADR rather than reintroduce inline rationale.
- `project-tauri-keyboard-native` is superseded by item 1 above and should be treated as historical.
- Future workarounds of this kind get a new ADR (or an addendum here) instead of a comment in `lib.rs`, per D13.
