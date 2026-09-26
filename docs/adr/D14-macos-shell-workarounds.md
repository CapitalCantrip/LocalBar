# ADR D14 — macOS shell workarounds

**Status:** Accepted
**Date:** 2026-09-27
**Deciders:** Jay

---

## Context

D13 removes rationale comments from `localbar-tauri/src/lib.rs`, keeping only `// SAFETY:`. Several of those comments justified a platform-specific workaround rather than restating what the code does. Their rationale needs a home outside the source file; this ADR is that home. This ADR supersedes the memory note `project-tauri-keyboard-native`, which held the same NSEvent rationale.

## Decision

Keep the following four workarounds, for the reasons below, without commenting on them in `lib.rs`:

1. **NSEvent local monitor for ESC and Cmd+W.** WKWebView does not deliver these key events to JS, so `install_popover_key_monitor` installs an `NSEvent` local monitor in Rust to dismiss the popover on ESC or Cmd+W. Cmd+H is deliberately not intercepted: LSUIElement apps (no Dock presence) have no meaningful "hide app" behavior, so the event is left to pass through unconsumed. The monitor is deliberately leaked for the lifetime of the app (`std::mem::forget`) rather than dropped, so it keeps intercepting events for as long as the app runs.

2. **`process::exit` on fatal tray setup failure.** `setup_handler` runs inside `applicationDidFinishLaunching`. Propagating an `Err` out of Tauri's setup callback causes Tauri to `panic!()` there, and a panic cannot unwind through the ObjC frame — it aborts the process anyway, but without a clean message. `std::process::exit(1)` is used instead for fatal tray-icon creation failures, after printing a diagnosable error to stderr.

3. **HTTP-only Ollama model discovery.** `discover_ollama_models` does not fall back to the `ollama` CLI the way `list_ollama_models_with_fallback` does for the "add instance" flow. Spawning `ollama list` activates Ollama.app through macOS launch services and steals window focus from LocalBar, which is unacceptable for a background discovery scan. Discovery accepts a smaller result set (HTTP only) over that side effect.

4. **Stop failures on adopted instances surface as an error badge.** When `stop_instance` cannot confirm that an adopted (unmanaged) server actually stopped, it moves the instance to `Error(StopFailed)` rather than silently reverting the phase to `Running`. A silent revert would look identical to a Stop that never happened, giving the user no indication that anything went wrong.

## Consequences

- These four behaviors are intentional and documented here; a change to any of them should update this ADR rather than reintroduce inline rationale.
- `project-tauri-keyboard-native` is superseded by item 1 above and should be treated as historical.
- Future workarounds of this kind get a new ADR (or an addendum here) instead of a comment in `lib.rs`, per D13.
