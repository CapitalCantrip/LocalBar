# ADR D7 — uvx as primary mlx-lm launch mechanism

**Status:** Accepted  
**Date:** 2026-07-12  
**Deciders:** Jay

---

## Context

mlx-lm is a Python package. To spawn it, LocalBar needs a Python environment with mlx-lm installed. The candidate launch paths are:

1. **System Python (`/usr/bin/python3`)** — Xcode's bundled Python on developer machines; has no third-party packages. Always available but almost never correct.
2. **Homebrew / venv Python** — correct if the user installed mlx-lm into one, but the path varies per machine and venv.
3. **uvx** — `uv`'s ephemeral tool runner. `uvx --from mlx-lm mlx_lm.server` downloads (or uses a cached copy of) mlx-lm and runs it in an isolated env. Requires `uv` to be installed but not a pre-existing mlx-lm venv.

The first real-world failure was that `PathScanner` detected Xcode's Python at `/Applications/Xcode.app/.../python3`, which lacks mlx-lm, producing `ModuleNotFoundError` at launch.

A second failure arose from the uvx invocation: `uvx mlx-lm server` was generated, but uvx interprets the second token as an executable name within the package, not a subcommand. The correct form is `uvx --from mlx-lm mlx_lm.server`.

---

## Decision

**PathScanner prefers uvx**, then falls back to Homebrew/local Python paths, with system Python last.

Detection order:
1. `/usr/local/bin/uvx`
2. `/opt/homebrew/bin/uvx`
3. `~/.local/bin/uvx`
4. `/opt/homebrew/bin/python3`
5. `/usr/local/bin/python3`
6. `/usr/bin/python3` (last resort)

`MLXLMDriver.makeLaunchPlan` detects the executable by suffix (`/uvx` or `== "uvx"`) and builds the argument list accordingly:

- **uvx path:** `uvx --from mlx-lm mlx_lm.server --model <path> --host <h> --port <p> [flags]`
- **python path:** `python3 -m mlx_lm.server --model <path> --host <h> --port <p> [flags]`

---

## Consequences

- uvx downloads mlx-lm on first use (requires internet); subsequent runs use the uv cache. This is acceptable for a dev tool.
- The uvx-installed mlx-lm may differ from a user's pip-installed version (e.g. an external switch script may use a different install). Version divergence is silent — no version pinning is implemented.
- Users who run mlx-lm via a custom venv or an external switch script should set the executable path manually to their actual Python. The auto-detected uvx path is a convenience default, not a guarantee of matching the user's existing setup.
- The `--from mlx-lm` form requires that the PyPI package name is `mlx-lm` and the entry point is `mlx_lm.server`. If the package renames either, the invocation breaks silently (the server fails to start and the crash message from uvx will appear in the error panel via stderr capture).
