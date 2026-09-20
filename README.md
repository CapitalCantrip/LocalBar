# LocalBar

A menu bar app for managing local LLM inference servers — start, stop, switch models, and tune generation parameters without touching a terminal. Built on a cross-platform Rust core; macOS now, Linux next.

---

## The gap

Every major local LLM inference server — mlx-lm, Ollama, llama.cpp — ships with its own CLI and its own way of doing things. Power users running local models on Apple Silicon end up with a fragmented setup: one tool to start a server, another to switch models, manual terminal work to change parameters, no unified view of what's actually running.

LM Studio solves this, but only for its own server. Ollamac and OllamaBar solve it for Ollama only. Nothing is generic.

LocalBar fills that gap.

| Tool | Servers supported | Param control | Process management |
|---|---|---|---|
| LM Studio | LM Studio only | ✅ | ✅ |
| Ollamac / OllamaBar | Ollama only | ❌ | ❌ |
| **LocalBar** | **mlx-lm, Ollama, any OpenAI-compatible** | **✅** | **✅** |

---

## What it does

**Server lifecycle**
- Start, stop, and restart any configured instance from the menu bar or Settings
- Four tray icon states: running, stopped, error, transitioning
- Concurrent-start warning if another server is already loaded into memory

**External server adoption**
- Detects servers already running on a port (launchd, external scripts, a manual terminal launch) and connects without restarting
- Port conflicts are surfaced explicitly — shows the loaded model and offers Adopt or Retry

**Model control**
- Switch models with one click; LocalBar handles the API call or server restart as required
- Auto-remembers last-used parameters per model and restores them on switch
- One-click rollback button after a failed model switch

**Generation parameters**
- Per-engine parameter schema — each server type exposes only the parameters it actually supports
- Canonical parameter names (Temperature, Top-P, Top-K, Min-P, Max Tokens, Repeat Penalty, Seed, KV Cache Size) with hover tooltips showing the underlying server flag

**Multiple instances**
- Multiple concurrent server instances on different ports and server types
- LocalBar is a control plane only — it does not proxy requests; your tools point at server ports directly

---

## Supported servers

| Server | Status |
|---|---|
| mlx-lm | ✅ |
| mlx-vlm | ✅ (same driver) |
| Ollama | ✅ |
| External (any OpenAI-compatible) | ✅ |
| llama.cpp | 🔜 |

---

## Requirements

- macOS 14.0+
- Rust + Cargo ([rustup.rs](https://rustup.rs))
- Node.js 20+
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/): `cargo install tauri-cli`

For mlx-lm: `uv` installed (`brew install uv`) is the recommended zero-config path.

For Ollama: Ollama installed (`brew install ollama` or from [ollama.com](https://ollama.com)).

---

## Installation

### Pre-built DMG

Download the latest DMG from [Releases](https://github.com/CapitalCantrip/LocalBar/releases).

macOS will show an "unidentified developer" warning — LocalBar is not yet signed with an Apple Developer certificate. To open it:

1. Right-click `LocalBar.app` → **Open**
2. Click **Open** in the dialog

You only need to do this once. If you'd rather not take that on trust, build it yourself below — macOS will run self-built apps without any warning.

### Build from source

```bash
cd localbar-tauri
npm install            # install frontend dependencies
cargo tauri build      # produces LocalBar.app in target/release/bundle/macos/
```

For development:

```bash
cd localbar-tauri
cargo tauri dev        # hot-reloads the frontend; Rust changes trigger a recompile
```

---

## Project structure

```
localbar/
├── localbar-core/          # Pure Rust: server drivers, lifecycle, persistence, types
│   └── src/
│       ├── drivers/        # mlx-lm, Ollama, external drivers
│       └── types.rs        # Canonical types shared across crates
├── localbar-tauri/         # Tauri app shell
│   ├── src/lib.rs          # IPC commands, tray, window management
│   ├── ui/src/             # React frontend
│   │   ├── popover/        # Tray popover (operational surface)
│   │   └── settings/       # Settings window (configuration surface)
│   └── tauri.conf.json
└── docs/
    └── adr/                # Architecture decision records
```

---

## Architecture decisions

| ADR | Decision |
|---|---|
| D1 | Ollama per-request-overridable params shown with advisory badge |
| D2 | Param memory keyed per server type (mlx-lm and Ollama get separate entries for the same weights) |
| D3 | Managed servers die with LocalBar; launchd keeps LocalBar alive for persistence |
| D4 | No auto-rollback on failed model switch — land in error, offer one-click manual rollback |
| D5 | 120 s startup timeout default |
| D6 | External server adoption: health-check-first, lsof for PID, escalating signals on stop |
| D7 | uvx as primary mlx-lm launcher; Python venv as fallback |
| D8 | Concurrent servers allowed; warn before starting a second |
| D9 | Dynamic activation policy — Cmd+Tab present while Settings is open, absent otherwise |
| D10 | Model scan path persisted; HF cache default |
| D-G | Tauri rewrite — Rust core + React frontend, replaces the Swift prototype |
| D-H | No in-app chat panel — LocalBar is a control plane, not a frontend |
| D-I | Windows: CLI binary first, GUI later |

---

## Status

**MVP functional.** Active development.

**Working:**
- mlx-lm and Ollama server lifecycle (start, stop, restart, model switch)
- External server adoption with port conflict detection and explicit Adopt/Retry flow
- Auto-reconnect: servers left running after quit are re-adopted on next launch
- Per-engine parameter schema — each driver exposes only the parameters it supports
- Parameter persistence: auto-memory per model, save-on-change
- Multiple concurrent instances
- Tray popover (operational) + Settings window (configuration)
- macOS-native tray icon with four states; dynamic Dock presence when Settings is open

**Post-MVP:**
- Named profiles (save and switch named parameter presets)
- Context window usage display
- llama.cpp driver
- Linux support

---

## Licence

LocalBar is dual-licensed.

**Open-source use — [GPL v3](LICENSE)**
Free for individuals, hobbyists, and open-source projects. Derivatives must also be released under GPL v3.

**Commercial use — [Commercial Licence](LICENSE-COMMERCIAL)**
Required if you embed LocalBar in a commercial product or deploy it within a for-profit organisation at scale. [Contact us](mailto:capitalcantrip@gmail.com) to discuss.

---

## Origin

LocalBar grew out of the [AgenticOS](https://github.com/CapitalCantrip) local agent stack, where managing local inference servers became acute enough to deserve its own dedicated tool.
