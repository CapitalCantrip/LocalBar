# LocalBar

A native macOS menu bar app that monitors and controls local LLM inference servers — start, stop, switch models, and tune generation parameters without touching a terminal.

## The Gap

Every major local LLM inference server — mlx-lm, Ollama, llama.cpp, LM Studio — ships with its own CLI and its own way of doing things. Power users running local models on Apple Silicon are stuck with a fragmented setup: one tool to start a server, another to switch models, manual terminal work to change parameters, and no unified view of what's actually running.

LM Studio solves this well, but only for its own server. Ollamac and OllamaBar solve it for Ollama only. Nothing is generic.

LocalBar fills that gap.

| Tool | Servers supported | Platform | Param control | Process management |
|---|---|---|---|---|
| LM Studio | LM Studio only | macOS / Win / Linux | ✅ | ✅ |
| Ollamac | Ollama only | macOS | ❌ | ❌ |
| OllamaBar | Ollama only | macOS | ❌ | ❌ |
| **LocalBar** | **Any OpenAI-compatible (mlx-lm, mlx-vlm, Ollama, llama.cpp…)** | **macOS** | **✅** | **✅** |

---

## What It Does

LocalBar lives in your menu bar and gives you full control over any configured local LLM inference server.

**Server management**
- Start, stop, and restart servers directly from the menu bar or the Settings panel
- Four icon states: running, stopped, error, and transitioning
- macOS system notifications for crashes and state changes
- Concurrent-start warning: if another server (managed or unmanaged) is already running when you click Start, LocalBar warns you before loading a second model into memory

**External server adoption**
- LocalBar detects servers already running on a configured port (launchd-managed, an external switch script, manual terminal launch) and connects to them without spawning a new process
- Port conflict surfaced explicitly on Start: shows the loaded model name and offers Adopt or port-change + Retry options
- Adopting a server auto-updates the instance name and model selection to match what's actually loaded

**Model control**
- Switch models with one click — LocalBar handles the API call or server restart depending on the backend
- Auto-rollback button on failed model switch — one click to restart with the previous model (D4)
- Auto-remembers last-used parameters per model; restores them on switch

**Generation parameters**
- Unified parameter panel with canonical names (Temperature, Top-P, Top-K, Min-P, Max Tokens, Repetition Penalty, Seed)
- Hover tooltips show the true server-specific flag name for each parameter
- Parameters unsupported by the active server are shown greyed-out — nothing hidden
- Ollama params that external clients can override per-request are badged with an advisory (D1)
- Named profiles: save and switch between named parameter presets (architecture ready, UI post-MVP)

**Performance**
- Context window usage displayed in real time (where the server exposes it)

**Multi-server**
- Multiple concurrent server instances supported (different ports, different models, different server types)
- LocalBar is a control plane only — it does not proxy requests; your tools point at server ports directly

---

## Supported Servers

| Server | Status |
|---|---|
| mlx-lm | ✅ MVP |
| mlx-vlm | ✅ MVP (same driver) |
| Ollama | ✅ MVP |
| llama.cpp | 🔜 Post-MVP |
| LM Studio | 🔜 Post-MVP |
| Other MLX ecosystem servers | 🔜 Post-MVP (incremental) |

LocalBar uses a typed `ServerDriver` protocol internally — adding a new server type is additive, not a rewrite. MLX ecosystem servers share enough common ground that each requires only a lightweight driver implementation.

---

## Design Principles

**Control plane, not a proxy.** LocalBar manages server processes and surfaces their state. It does not sit in the request path. External tools (Cursor, Continue.dev, etc.) point directly at server ports — LocalBar stays out of that line of fire.

**Adopt what's running, manage what you start.** LocalBar can connect to externally-started servers (launchd, shell scripts, an external switch script) without restarting them. When it starts a server itself, it owns the lifecycle. The distinction is explicit: port conflicts are surfaced with an Adopt button, not silently resolved.

**macOS-native, no compromises.** Built in Swift 5.10 and SwiftUI for macOS 14+. Proper menu bar citizen (`LSUIElement`), dynamic Cmd+Tab presence when Settings is open, resizable settings window. App Store ready. Minimal resource footprint.

---

## Requirements

- macOS 14.0+
- Xcode 15+ (to build)
- [XcodeGen](https://github.com/yonaskolb/XcodeGen) (to generate the project)

For mlx-lm support: `uv` installed (`brew install uv`) is the recommended zero-config path. A Python venv with mlx-lm installed also works — point the executable path to that Python.

For Ollama support: Ollama installed (`brew install ollama` or from [ollama.com](https://ollama.com)).

---

## Installation

### Pre-built DMG (easiest)

Download the latest DMG from [Releases](https://github.com/CapitalCantrip/localbar/releases).

**macOS will show an "unidentified developer" warning.** LocalBar is not signed with an Apple Developer certificate — that costs $99/yr, which doesn't make sense for a small open-source project. To open it:

1. Right-click (or Control-click) `LocalBar.app` → **Open**
2. Click **Open** in the dialog

You only need to do this once. If you'd rather not take that on trust, the full source is right here — build it yourself in a few minutes and macOS will run it without any warning.

### Build from source

```bash
# One-time setup — generates LocalBar.xcodeproj from project.yml
./setup.sh

# Then open in Xcode and hit Run (⌘R)
open LocalBar.xcodeproj
```

Requires Xcode (full app, not just command-line tools) and [XcodeGen](https://github.com/yonaskolb/XcodeGen) (`brew install xcodegen`).

The Xcode project is generated by XcodeGen and is not committed. Run `xcodegen generate` (or `./setup.sh`) after any change to `project.yml`.

---

## Project Structure

```
localbar/
├── LocalBar/
│   ├── App/                  # Entry point (LocalBarApp.swift)
│   ├── Core/                 # ServerDriver protocol, data model, InstancePhase state machine
│   ├── Drivers/              # mlx-lm and Ollama driver implementations
│   ├── Services/             # Process lifecycle, health polling, path detection, persistence
│   ├── UI/                   # MenuBarView, SettingsView (HSplitView panel), PhaseIndicatorView
│   └── Resources/            # Info.plist, assets
├── docs/
│   ├── adr/                  # Architecture decision records D1–D10 + D7b (future intentions)
│   └── architecture-retrospective.md
├── project.yml               # XcodeGen project spec
└── setup.sh                  # One-time setup script
```

---

## Architecture Decision Records

| ADR | Decision |
|---|---|
| D1 | Ollama per-request-overridable params shown with advisory badge |
| D2 | Param memory keyed per-runtime (mlx-lm and Ollama get separate entries for the same model weights) |
| D3 | Managed servers die with LocalBar; launchd keeps LocalBar alive for persistence |
| D4 | No auto-rollback on failed model switch — land in error, offer one-click manual rollback |
| D5 | 120 s startup timeout default, per-instance override in advanced flags |
| D6 | External server adoption: health-check-first, lsof for PID, escalating signals on stop |
| D7 | uvx as primary mlx-lm launcher; Python venv as fallback |
| D7b | Future: comprehensive launcher detection across all Python environments |
| D8 | Concurrent servers allowed; warn (including unmanaged servers) before starting a second |
| D9 | Dynamic `.regular`/`.accessory` activation policy — Cmd+Tab present while Settings is open |
| D10 | Model scan path persisted via `@AppStorage`; HF cache default |

---

## Status

🚧 **Active development — MVP functional. Test coverage is minimal; feedback welcome.**

**Working:**
- mlx-lm and Ollama server lifecycle (start, stop, restart, model switch)
- External server adoption with port conflict detection and explicit Adopt/Retry flow
- Auto-reconnect on restart: servers left running after quit are silently re-adopted on next launch
- Model scan and metadata parsing (parameter count, quantization, format, capabilities)
- Executable auto-detection via login shell and common install paths
- Concurrent-start warning covering both managed and unmanaged servers
- Memory footprint warning before start (>70% RAM threshold, weights + KV cache estimate)
- Parameter panel: all canonical params, save-on-change, Ollama Modelfile badge, system prompt
- Persistence across restarts (instances, params, settings)
- Settings: resizable HSplitView panel, instance detail, model picker, error panel with rollback
- Dynamic Cmd+Tab presence when Settings is open
- macOS system notifications (respects in-app toggle)

**Next up:**
- Named profiles (save and switch named parameter presets)
- Context window display (mlx-lm doesn't expose fill level yet — tracking upstream)
- Comprehensive launcher detection (D7b)

**Post-MVP:**
- llama.cpp and LM Studio drivers
- App Store distribution

---

## Licensing

LocalBar is dual-licensed.

**Open-source use — [GPL v3](LICENSE)**
Free for individuals, hobbyists, and open-source projects. If you distribute a modified version, your derivative must also be released under GPL v3.

**Commercial use — [Commercial Licence](LICENSE-COMMERCIAL)**
A commercial licence is required if you:
- Deploy LocalBar within a for-profit organisation to serve more than 5 users
- Embed LocalBar in a product you distribute commercially
- Want to distribute a modified version without the GPL v3 share-alike obligation

[Contact us](mailto:capitalcantrip@gmail.com) to discuss commercial licensing.

Feedbacks are welcome.

---

## Origin

LocalBar grew out of the [AgenticOS](https://github.com/CapitalCantrip) local agent stack, where the problem of managing local inference servers became acute enough to deserve its own dedicated tool.
