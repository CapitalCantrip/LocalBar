# LocalBar — Concept Note

*Status: rough idea. Not a design doc yet. Revisit 2026-07-10.*

---

## The Gap

There is no open-source menu bar app that manages a local LLM inference server — shows what's running, lets you switch models, and gives LM Studio-level control over generation params — for any OpenAI-compatible endpoint (mlx-lm, Ollama, llama.cpp, LM Studio, etc.).

LM Studio has this for its own server only. Ollamac / OllamaBar are Ollama-only. Nothing is generic.

---

## Core Value Prop

One menu bar icon that tells you:
- Is a local model running right now?
- Which model?
- Lets you switch / stop / start with one click
- Lets you tweak params without touching a terminal

Audience: macOS and Linux power users running local LLMs. Growing fast.

---

## Scope (MVP)

- Auto-detect servers on common ports (:8080, :11434, :1234, :5000)
- Pull model list from `/v1/models` for any detected server
- Quick-switch between configured model profiles
- Per-profile param presets (temp, top-p, top-k, max tokens, rep penalty, seed)
- Apply & Restart flow with health polling
- System prompt viewer/editor
- Token/sec display (if server exposes it)

## Out of scope for MVP

- Model downloading / HuggingFace integration
- Multi-machine / remote servers
- Conversation history

---

## Technology options

### Tauri (Rust + web frontend)
- Cross-platform: macOS + Linux covers the full local LLM audience
- Lightweight (no Electron overhead — ironic to use Electron for an LLM resource manager)
- Large web-dev contributor pool
- Steeper initial setup but good long-term

### Native Swift/SwiftUI
- Best macOS experience, proper menu bar citizen
- App Store distribution possible (big for discoverability)
- macOS only — but that's where most local LLM power users are (M-series chips dominate benchmarks)
- Resource cost: minimal — a native Swift menu bar app with no heavy framework is ~15–30MB RAM. Not a concern.
- Could be a separate repo / fork from a Tauri version if both are desired

### Python + rumps
- Fastest to working MVP (reuses existing hermes-settings.py logic)
- macOS only
- Harder to distribute cleanly (Python dep)

### Verdict (tentative)
Tauri for cross-platform reach and contributor friendliness. Swift/SwiftUI as a potential Mac App Store fork — native feel, potentially better discoverability. Decision deferred.

---

## Name ideas

- **LocalBar** — descriptive, available-ish
- **ModelBar** — clear
- **Inferbar** — inference + menu bar
- **LLMBar** — obvious but a bit ugly
- **Hermes** is already taken (this project) but the tool concept is distinct

---

## Prior art / differentiation

| Tool | Server | Platform | Param control |
|---|---|---|---|
| LM Studio | LM Studio only | macOS/Win/Linux | Yes |
| Ollamac | Ollama only | macOS | No |
| OllamaBar | Ollama only | macOS | No |
| **LocalBar** | Any OpenAI-compat | macOS + Linux | Yes |

---

## Origin

Built as part of AgenticOS (this repo) for the Hermes local agent stack. The SwiftBar plugin + tkinter settings panel in `hermes/_install/` is the proof-of-concept. Core ideas validated 2026-07-09.
