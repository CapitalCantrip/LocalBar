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

There are two ways to get LocalBar on macOS. Pick one:

| | Download the DMG | Build it yourself |
|---|---|---|
| Effort | A few clicks, plus a one-time approval step | About 10–15 minutes the first time |
| What you need | Nothing extra | Xcode Command Line Tools, Rust, Node.js |
| macOS warning | Yes, once per version (see below) | None |

### Option 1: Download the DMG

Download the latest `LocalBar_<version>_universal.dmg` from [Releases](https://github.com/CapitalCantrip/LocalBar/releases) and drag `LocalBar.app` to Applications.

**Expect macOS to block it the first time.** LocalBar is free and open source, and isn't signed with a paid Apple Developer certificate (#93). macOS blocks every unsigned app downloaded from the internet, whatever it contains. Since macOS 15 the warning offers only **Move to Bin** or **Done**, and right-click → Open no longer works. This doesn't mean the app is damaged or harmful. If you'd rather not trust a download, use Option 2.

To allow it, do this once per downloaded version:

1. Open `LocalBar.app` from Applications. When macOS blocks it, click **Done** (not Move to Bin).
2. Open **System Settings → Privacy & Security** and scroll down to **Security**. Next to "LocalBar was blocked…", click **Open Anyway**.
3. Confirm with your password or Touch ID, then click **Open**.

LocalBar opens normally from then on.

**Or, if you're comfortable with Terminal,** this one command does the same thing. It removes the "downloaded from the internet" flag from the app:

```bash
xattr -dr com.apple.quarantine /Applications/LocalBar.app
```

### Option 2: Build it yourself

macOS runs apps you build on your own Mac without any warning.

One-time setup:

```bash
xcode-select --install                                              # Apple's command line tools
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh      # Rust (or see rustup.rs)
brew install node                                                   # Node.js 20+ (or nodejs.org)
cargo install tauri-cli --version "^2" --locked                     # Tauri's build tool
```

Build:

```bash
git clone https://github.com/CapitalCantrip/LocalBar.git
cd LocalBar/localbar-tauri/ui
npm ci && npm run build          # build the interface
cd ..
cargo tauri build                # build the app (takes a few minutes the first time)
```

The finished app is at `target/release/bundle/macos/LocalBar.app` in the repository root. A `.dmg` is in `target/release/bundle/dmg/`. Drag the app to Applications.

To update later, run `git pull` and repeat the build step.

For development:

```bash
cd localbar-tauri
cargo tauri dev        # hot-reloads the frontend; Rust changes trigger a recompile
```

Note that `cargo tauri dev` inherits your terminal's environment, so it can hide problems that only show up when the app is opened from Finder. Test release behaviour with the built app.

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
