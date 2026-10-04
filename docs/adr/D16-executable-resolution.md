# ADR D16 — Resolving server executables (GUI-app PATH)

**Status:** Accepted
**Date:** 2026-10-04
**Deciders:** Jay

---

## Context

macOS launches Finder/Dock apps with a minimal `PATH` (`/usr/bin:/bin:/usr/sbin:/sbin`); Linux desktop launchers behave similarly. Instances store the executable as the user typed it, and the Add Instance default for mlx-lm is bare `uvx`, which normally lives in `~/.local/bin`. `Command::new("uvx")` therefore failed with `No such file or directory` in the installed app, while `tauri dev` worked because it inherits the terminal's `PATH` (#87).

## Decision

Every user-configured executable is resolved to an absolute path on every spawn: `spawn_from_plan` and the Ollama `create`/`rm` CLI calls. The stored value is never rewritten, so bare `uvx` stays bare and instances stay portable between machines.

Search order:

1. **Login-shell `PATH`.** Once at startup, on a background thread started from `setup_handler` (never the main thread, never under the registry lock), LocalBar runs `$SHELL -i -l -c <script>` with stdin null and stderr discarded. `-i -l` is used because many users set `PATH` in `.zshrc`/`.bashrc` (interactive) rather than only in profile files; if that run fails, `-l -c` is tried. The script prints `$PATH` between unique markers (`__LOCALBAR_PATH_START__`/`__LOCALBAR_PATH_END__`) so banners, `nvm` messages and other profile noise are ignored; the last start marker wins. If `$SHELL` is unset, `/bin/zsh` (macOS) or `/bin/sh` (Linux) is used.
   - **fish** joins `$PATH` with spaces when quoted, so for a shell whose binary is named `fish` the script uses `(string join : $PATH)`; the parser still splits fish output on whitespace if no `:` is present.
   - **Timeout: 4 s per attempt**, after which the shell is killed. A broken or slow shell config can therefore never hang LocalBar; at worst resolution uses the folders below.
2. **The process `PATH`.**
3. **Well-known fallback folders:** `~/.local/bin`, then `/opt/homebrew/bin` and `/usr/local/bin` (macOS) or `/usr/local/bin` and `/home/linuxbrew/.linuxbrew/bin` (Linux), then `~/.cargo/bin`, `~/.pyenv/shims`, `~/.asdf/shims`, `~/.local/share/mise/shims`. These are always appended, and are the only extra folders when the shell read fails.

The merged list is de-duplicated, keeping the first occurrence. A configured value containing `/` is used as-is and never searched. The child process is started with the resolved absolute path and gets the merged list as its `PATH`, so tools it invokes resolve the same way. LocalBar never calls `std::env::set_var` (unsound once other threads exist).

**Spawns before the shell read finishes do not wait for it.** They resolve with the process `PATH` plus the fallback folders, which already covers the common installs (`uvx` in `~/.local/bin`, Homebrew). Blocking Start (or restore-at-launch) on a shell that may take up to the timeout would make the common case slower to serve a rare one.

**Not found → a dedicated error.** `InstanceErrorKind::ExecutableNotFound` (DTO `executableNotFound`) carries the message ``Couldn't find `uvx`. Looked in: …`` listing every folder searched (or just the configured path when it contains `/`). The popover row and the Settings detail panel show a **Choose…** button for this error, which opens a file picker and stores the chosen path as the instance's executable. The buttons sit in their own flex group (`MissingExecutableActions`) so #88's **Install** action can join it.

Following D12, the decisions — marker script and parsing, fish handling, shell choice, fallback list, merge order, slash rule and the not-found outcome — are pure functions in `localbar-core/src/executable.rs`. Running the shell, the timeout and stat-ing files live in `localbar-tauri/src/exec_path.rs`; the Ollama driver receives that resolver as an injected `CommandResolver`.

## Consequences

- The installed app finds `uvx`, `ollama` and similar tools wherever the user's shell would, with no prompt.
- Every spawn stats a few folders; this is negligible next to server startup.
- A shell config that only adds a folder after the 4 s timeout, and lives outside the fallback list, is not found; the user sees the not-found error and can use **Choose…**.
- Changes to the user's shell config take effect after LocalBar restarts.
