# ADR D7b — Future: comprehensive launcher detection

**Status:** Deferred (intended improvement to D7)  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

D7 established uvx as the primary mlx-lm launch mechanism, with a short ordered list of hard-coded paths as fallback. This is a reasonable default for the development machine but will fail silently on any machine where the user's mlx-lm lives elsewhere — in a conda env, a project-local venv, a pipx install, or a Homebrew-managed Python.

The broader intent for a future "comprehensive detection" pass is captured here so it can be designed and implemented as a coherent unit rather than added incrementally as individual machines fail.

---

## Intended approach

### 1. Broad scan before hand-holding

On first launch (and on user request), LocalBar should scan all common mlx-lm installation paths in one pass:

| Source | Paths to probe |
|---|---|
| uvx / uv | `/usr/local/bin/uvx`, `/opt/homebrew/bin/uvx`, `~/.local/bin/uvx`, `$(which uv)` sibling |
| pipx | `~/.local/pipx/venvs/mlx-lm/bin/python`, `$(pipx environment --value PIPX_LOCAL_VENVS)/mlx-lm/bin/python` |
| Homebrew Python | `/opt/homebrew/bin/python3`, `/opt/homebrew/opt/python*/bin/python3` glob |
| Conda / Mamba | `~/miniconda3/envs/*/bin/python3` glob, `~/mambaforge/envs/*/bin/python3` glob |
| pyenv | `~/.pyenv/versions/*/bin/python3` glob |
| project venvs | Common locations relative to `~`: `.venv/bin/python`, `venv/bin/python`, `env/bin/python` (one level deep only — avoid full home scan) |
| System Python | `/usr/bin/python3` (last resort; almost certainly missing mlx-lm) |

For each candidate Python binary, validate that `python3 -c "import mlx_lm"` exits 0. For uvx, validate with a dry-run or version check (`uvx --from mlx-lm mlx_lm.server --version`).

### 2. Fall back to hand-holding only when scan finds nothing

If the scan returns zero valid executables, show a setup assistant that guides the user through installing mlx-lm. Suggested steps in order: install uv → `uv tool install mlx-lm` → re-scan. This path should be rare on any machine that already runs mlx-lm.

Do not show the setup assistant if even one executable was found — the user almost certainly has a working install.

### 3. Offer user choice when multiple executables found

If the scan finds more than one valid mlx-lm-capable executable, present the list in the Add Instance sheet (or a one-time setup screen) and let the user pick. Show:
- The executable path
- The source (uvx / conda env name / pipx / venv label)
- The mlx-lm version (from `python3 -c "import mlx_lm; print(mlx_lm.__version__)"` or equivalent)

The user's choice is persisted (see D10 for path persistence). Default selection: uvx if present, otherwise the first non-system Python found.

If a single executable is found and validated, adopt it silently without prompting.

### 4. Expand the server list based on discovered executables

The scan should not be limited to mlx-lm. During the same pass, detect:

- **Ollama** — check `$(which ollama)`, `/usr/local/bin/ollama`, `/opt/homebrew/bin/ollama`. If found, offer an Ollama instance template.
- **LM Studio local server** — check if `~/.lmstudio/` exists or LM Studio.app is installed; offer to register its server endpoint.
- Future: **llamafile**, **llama.cpp server**, **vLLM** — same pattern.

The Add Instance sheet (or a future onboarding flow) can pre-populate templates for each discovered server type, so the user starts from a working configuration rather than a blank form.

---

## What does NOT change (constraints for this future work)

- The state machine, `ServerDriver` protocol, `ServerInstanceController`, and health-poll logic are not affected. Detection is entirely in `PathScanner` and the Add Instance sheet.
- The result of the scan is still a single executable path stored in `UserDefaults` (D10). The scan determines what the user chooses; persistence stores the choice.
- The uvx invocation args (`--from mlx-lm mlx_lm.server`) do not change if uvx is selected — that was the D7 fix and remains correct.

---

## When to implement

Implement as a unit when adding persistent configuration (the PersistenceService `// TODO` in InstanceRegistry). At that point, first-launch onboarding becomes possible and the scan can be wired to a natural entry point. Implementing detection without persistence would require re-running the scan on every Add — wasteful and disorienting.

Do not implement piecemeal. Adding individual paths to the D7 scan list is not the same as this; that is maintenance, not the comprehensive detection described here.

---

## Consequences

- Users on machines with non-standard installations will get working defaults without manual path configuration.
- The scan adds latency to first launch or Settings open; it should be async and cancellable, with a progress indicator in the Add Instance sheet.
- Conda/pyenv env name detection requires parsing directory names, which is fragile if the user has many envs. Limit the glob depth and add a timeout per candidate.
- Version display requires a subprocess call per candidate; parallelize with `async let` or a `TaskGroup`.
