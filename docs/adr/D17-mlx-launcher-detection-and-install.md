# ADR D17 — mlx-lm launcher detection and the Install offer

**Status:** Accepted
**Date:** 2026-10-04
**Deciders:** Jay
**Supersedes:** the detection order in [D7](D7-uvx-launch-mechanism.md); implements [D7b](D7b-launcher-detection-future.md) in reduced form

---

## Context

D7 made `uvx` the mlx-lm default and kept a fixed list of six paths. D7b described a broader scan but was deferred. D16 (#87) fixed *finding* a configured executable at spawn time, but the default was still `uvx` whether or not uv was installed, and a machine with no launcher at all got only a not-found error and a **Choose…** button (#88).

## Decision

### Launch forms

The argument prefix is chosen from the executable's basename (`launcher::mlx_launch_prefix`):

| Executable basename | Command |
|---|---|
| `mlx_lm.server` | `mlx_lm.server --model … --host … --port …` |
| `uv` | `uv tool run --from mlx-lm mlx_lm.server …` |
| `uvx` | `uvx --from mlx-lm mlx_lm.server …` (unchanged from D7, so existing instances keep working) |
| anything else | `<python> -m mlx_lm.server …` |

Every form is recognised by `MLXLMDriver::recognises_process` (D12 adoption): `uv tool run` contains the same `--from mlx-lm mlx_lm.server` tokens as `uvx`, and a direct `mlx_lm.server` is the console-script form already matched.

### Detection order (Add Instance default)

`detect_mlx_launcher` searches the D16 search path (login-shell `PATH`, process `PATH`, fallback folders), in this order, and returns the first hit:

1. **`mlx_lm.server`** on the search path (from `uv tool install mlx-lm` or pipx): offline, fastest start, fixed version.
2. **`uv`**, then **`uvx`**. `uvx` ships with uv, so `uv` alone is enough; `uvx` is still accepted for setups that only expose it.
3. **A Python that passes `python -c "import mlx_lm"`**: first `python3` in each search-path folder, then the D7b locations — pipx's `mlx-lm` venv, `~/.venv`, `~/venv`, `~/env`, Homebrew/`/usr/local` `python3`, `/opt/homebrew/opt/python*/bin/python3`, and `bin/python3` in each subfolder of `~/.pyenv/versions`, `~/miniconda3/envs`, `~/miniforge3/envs`, `~/mambaforge/envs`, `~/anaconda3/envs` — and `/usr/bin/python3` last.

Results 1 and 2 are stored as **bare names** so D16 resolves them on every spawn and instances stay portable; a Python is stored as its absolute path. If nothing is found, Add Instance keeps `uvx` and says mlx-lm wasn't found.

Bounds: no home-wide scan; each "each subfolder" location lists one directory and keeps at most 16 entries (`MAX_ENTRIES_PER_GLOB`, sorted, hidden names skipped); only existing executables are run; each import check has a 15 s timeout and is killed after it; the scan stops at the first Python that imports mlx-lm. The whole scan has a 30 s deadline (`MAX_DETECTION_TIME`); once it passes, no further import check starts and detection returns nothing.

On macOS without the Command Line Tools, `/usr/bin/python3` is a stub: running it at all, even for an import check, pops the system "install developer tools" dialog. Detection therefore runs `/usr/bin/xcode-select -p` once (3 s timeout; it never shows the dialog) and, if it fails, never runs `/usr/bin/python3` — neither the fixed candidate nor `python3` found in a search-path `/usr/bin`. The filter is the pure `DetectionHost` input in core. Detection runs in `spawn_blocking`, never on the main thread or under the registry lock.

D7b's multi-choice picker, version display and Ollama/LM Studio templates are not implemented: the single best launcher is pre-filled and can be edited in the sheet.

### Install offer

The not-found error shows **Install…** next to **Choose…** for mlx-lm and Ollama instances.

- **mlx-lm:** `launcher::mlx_install_plan(uv, brew)` returns the steps given what is present:
  - uv found → `<abs uv> tool install mlx-lm`;
  - no uv, `brew` found → `<abs brew> install uv`, then `uv tool install mlx-lm`;
  - neither → `/bin/sh -c 'curl -LsSf https://astral.sh/uv/install.sh | sh'` (the official installer, which installs into `~/.local/bin` without admin rights), then `uv tool install mlx-lm`.
  The `uv` for the second step is resolved to an absolute path *after* the first step runs, through the D16 search path (which includes `~/.local/bin` and `/opt/homebrew/bin`).
- **Ollama:** not installed automatically. Ollama ships as a signed macOS app that installs its CLI with an administrator prompt and runs its own menu-bar daemon; scripting that (or a Homebrew cask) would mean LocalBar driving admin prompts and app placement it cannot verify. The dialog links to `https://ollama.com/download` instead.

Before anything runs, a dialog shows the exact commands and waits for **Install**. Each step runs with stdin closed, stdout/stderr captured, the merged D16 `PATH`, and a **15-minute timeout** (downloads of mlx and its dependencies can be large); a timed-out step is killed. On failure the dialog shows the failing command and its output with **Try again**. Only one install runs at a time. The plan is recomputed by the backend when Install is pressed; the frontend never sends commands to run.

After success, detection runs again and the instance's executable is set to the result (normally bare `mlx_lm.server`); an `executableNotFound` error is cleared to Stopped, and the dialog offers **Start**.

**The dialog lives in the Settings window.** The popover hides when it loses focus (`WindowEvent::Focused(false)`), which would drop a confirmation or a minutes-long progress view. **Install…** in the popover therefore calls `open_settings_for_instance(id, install: true)`, which opens Settings, selects that instance and opens the dialog there.

Following D12, the launch forms, launcher ranking, Python candidate list, subfolder expansion cap and install plan are pure functions in `localbar-core/src/launcher.rs`. Running commands, timeouts, directory listing and stat-ing live in `localbar-tauri/src/tool_install.rs`.

## Consequences

- New mlx-lm instances default to whatever already works on the machine, and machines with nothing installed get a one-click, confirmed install with no admin rights.
- `uv tool install` pins the installed mlx-lm version until the user upgrades it (`uv tool upgrade mlx-lm`); `uvx` instances keep D7's download-on-demand behaviour.
- The official uv installer edits shell profile files to add `~/.local/bin` to `PATH`; LocalBar does not need that edit (D16 searches `~/.local/bin` anyway) but does not suppress it.
- A Python install outside the listed locations, or more than 16 envs deep in one folder, is not detected; the user can still type the path or use **Choose…**.
