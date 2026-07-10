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
- Start, stop, and restart servers directly from the menu bar
- Four icon states: running, stopped, error, and transitioning (with estimated time and progress during model switches)
- macOS system notifications for crashes and state changes — configurable, on by default

**Model control**
- Switch models with one click — LocalBar handles the API call or server restart depending on the backend
- Restart warnings with adaptive time estimates (LocalBar learns from your actual switch durations)
- Auto-remembers last-used parameters per model; restores them on switch

**Generation parameters**
- Unified parameter panel with canonical names (Temperature, Top-P, Top-K, Min-P, Max Tokens, Repetition Penalty, Seed)
- Hover tooltips show the true server-specific flag name for each parameter
- Parameters unsupported by the active server are shown greyed-out with a clear explanation — nothing is hidden
- Named profiles: save and switch between named parameter presets ("Coding", "Creative", etc.)

**System prompt**
- View and edit system prompts in-app — no external editor required

**Performance**
- Context window usage displayed in real time — the metric that matters most when running local models

**Multi-server**
- Data model supports multiple concurrent server instances (e.g. a large model + a small draft model for speculative decoding)
- LocalBar is a control plane only — it does not proxy requests; your tools point at server ports directly

---

## Supported Servers

| Server | Status |
|---|---|
| mlx-lm | ✅ MVP |
| mlx-vlm | ✅ MVP |
| Ollama | ✅ MVP |
| llama.cpp | 🔜 Post-MVP |
| LM Studio | 🔜 Post-MVP |
| Other MLX ecosystem servers | 🔜 Post-MVP (incremental) |

LocalBar uses a typed `ServerDriver` protocol internally — adding a new server type is additive, not a rewrite. MLX ecosystem servers (mlx-lm, mlx-vlm, and others) share enough common ground that each requires only a lightweight driver implementation.

---

## Design Principles

**Control plane, not a proxy.** LocalBar manages server processes and surfaces their state. It does not sit in the request path. External tools (Cursor, Continue.dev, etc.) point directly at server ports — LocalBar stays out of that line of fire.

**Configured, not discovered.** LocalBar manages servers you explicitly configure inside it. No port scanning, no auto-detection. If you need to set up a new server, LocalBar guides you through the process — showing you the right commands, but not running them without your explicit action.

**macOS-native, no compromises.** Built in Swift and SwiftUI. Proper menu bar citizen. App Store ready. Minimal resource footprint — the irony of an Electron app managing your LLM inference server is not lost on us.

---

## Status

🔒 **Private — active development toward MVP.**

Post-MVP: open source, with optional App Store support-the-dev model.

**Completed:**
- Proof-of-concept (SwiftBar plugin + tkinter settings panel — see `hermes-status.30s.sh` and `hermes-settings.py`)
- Discovery and scoping (`docs/discovery.md`)
- Core architecture design — ServerDriver protocol, data model, process lifecycle state machine (`docs/architecture.md`)

**In progress:**
- Architecture decision records (D1–D5)
- Xcode project scaffold

---

## Project Structure

```
localbar/
├── LocalBar/          # Swift/SwiftUI app
│   ├── App/           # Entry point
│   ├── Core/          # ServerDriver protocol, data model, state machine
│   ├── Drivers/       # mlx-lm and Ollama implementations
│   ├── UI/            # Menu bar and settings views
│   ├── Services/      # Process lifecycle, health polling, notifications
│   └── Resources/     # Assets, icons
├── LocalBarTests/
├── LocalBarUITests/
├── docs/              # Discovery, architecture, ADRs
│   └── adr/
└── scripts/
```

---

## Origin

LocalBar grew out of the [AgenticOS](https://github.com/CapitalCantrip) local agent stack, where the problem of managing local inference servers became acute enough to deserve its own dedicated tool.
