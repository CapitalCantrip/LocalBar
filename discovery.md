# LocalBar — Discovery & Scoping Doc

*Status: open questions. Work through each section to lock decisions before design begins.*

---

## 1. What are we building?

A macOS menu bar app that monitors and controls any OpenAI-compatible local LLM inference server — shows what's running, lets you switch models, and gives you generation param control without touching a terminal.

**Proof-of-concept exists:** `hermes-status.30s.sh` + `hermes-settings.py` (Hermes-specific, SwiftBar-based). LocalBar generalises this.

---

## 2. Platform & Technology

### 2.1 Target platform(s)
**DECIDED: macOS only for MVP.**
- Linux/Windows deferred — separate project post-MVP if demand exists

### 2.2 Native Swift vs Tauri
**DECIDED: Swift/SwiftUI.**
- Best macOS citizen, App Store possible, minimal resource use
- Target audience is predominantly M-series Mac users
- Tauri is not a port — it's a full rewrite. If Linux demand emerges post-MVP, Tauri becomes a separate project using the Swift version as the spec
- Porting cost is too high to justify designing for it upfront

### 2.3 Distribution
**DECIDED: Direct download (DMG) for MVP, App Store ready from the start.**
- Build to macOS conventions throughout so App Store submission is viable later
- Sandboxing implications to be evaluated before any App Store submission

---

## 3. Server Detection & Support

### 3.1 Which servers to support at launch?
- mlx-lm and Ollama — MVP
- llama.cpp, LM Studio — deferred, added incrementally

### 3.2 How does LocalBar know about servers?
**DECIDED: User-configured only. No auto-detection or port scanning.**
- LocalBar is a **control plane for servers the user explicitly configures inside it**
- User adds a server configuration (type, model path, port) — LocalBar then owns start/stop/restart
- Server installation: LocalBar checks if the binary is present. If not, it guides the user through installation (shows the command, offers to copy it) but does not execute installs itself without explicit user action
- Model weight downloads: **out of scope**. LocalBar can point users to the right command and hand-hold through the process, but does not download weights directly

### 3.3 What's the health check?
**DECIDED: GET `/v1/models` returning 200 = healthy.**
- Partially-up state (process running, port not yet responding) = Error state in the menu bar icon
- Latency threshold for "degraded" state: deferred

### 3.4 Multi-server
**DECIDED: Data model supports N server instances from day one. MVP UI manages all running instances.**
- Key use case: large model + small draft model running simultaneously (speculative decoding pattern)
- LocalBar is a **control plane only** — it does not proxy requests
- External tools (Cursor, Continue.dev, etc.) point directly at server ports; users manage their own tool configs
- Proxy/routing mode explicitly deferred — revisit post-MVP if there's user demand and the technical cost is justified

### 3.5 Adopting already-running servers
**DECIDED: Excluded.**
- Value-to-complexity ratio too low
- No first-run detection or adoption flow
- Invest the saved effort in good setup UX instead — clear onboarding to configure servers inside LocalBar

---

## 4. Model Management

### 4.1 Model list source
- Pull from `/v1/models` — straightforward
- What if a server doesn't implement `/v1/models` fully? (some llama.cpp builds don't)
- Fallback: user-defined model list?

### 4.2 Model switching
**DECIDED:**
- LocalBar owns the full switch flow — API call for Ollama, kill + restart for mlx-lm
- **Restart warning:** show a pop-up before any restart-required switch, with a "don't show again" checkbox
- **Time estimate:** show estimated restart time in the warning. Log actual durations locally and use them to improve future estimates over time (simple local learning — no external services)
- **Fourth icon state:** "Transitioning" — shown during start/stop/switch. Distinct from On / Off / Error

### 4.3 Model profiles / presets
**DECIDED: Both auto-memory and named profiles.**
- **Default behaviour:** LocalBar remembers the last-used params per model (keyed by model name or filename) and restores them automatically on switch — no user action needed
- **Named profiles:** users can save named param snapshots that override per-model defaults (e.g. "Coding" = Qwen 32B + temp 0.3, "Creative" = same model + temp 1.1)
- **Per-user:** profiles stored in `~/Library/Application Support/LocalBar/` — macOS user-scoped by default, so multi-user machines get isolated profiles automatically

---

## 5. Generation Parameters

### 5.1 Which params to expose?
**DECIDED: Generic canonical param layer with server-specific transparency.**
- LocalBar shows a unified set of params with canonical names (e.g. "Repetition Penalty")
- Hover tooltip on each param shows the true server-specific flag name (e.g. `repeat_penalty` for Ollama, `repetition_penalty` for mlx-lm)
- Params not supported by the active server are **shown but greyed out** with a clear explanation ("mlx-lm does not support this parameter")
- Initial canonical param set (from PoC): temp, top-p, top-k, min-p, max tokens, repetition penalty, seed, chat template args

### 5.2 How are params applied?
- mlx-lm: write to config, restart required — LocalBar owns the restart flow (see 4.2)
- Ollama: API call where supported, restart otherwise
- Deferred: define exact apply behaviour per server type during implementation

### 5.3 Advanced server flags
**DECIDED: Per-server-type, shown only when that server type is configured.**
- mlx-lm advanced flags (log level, concurrency, prefill step size, prompt cache) shown for mlx-lm configs only
- Ollama equivalents shown for Ollama configs only
- Not part of the canonical param layer — these are server internals, not generation params

---

## 6. System Prompt

### 6.1 Should LocalBar manage system prompts?
**DECIDED: Yes — in-app editing included in MVP.**
- Full edit capability in-app (not just read-only viewer as in PoC)
- Per-model system prompt presets: deferred

---

## 7. Token / Performance Monitoring

### 7.1 What metrics to show?
**DECIDED: Context window usage is the MVP priority metric.**
- Tokens used / max context — most actionable for users
- Other metrics (tokens/sec, CPU/GPU/memory) deferred to post-MVP

### 7.2 Where does this live in the UI?
- Deferred — decide during UI design phase

---

## 8. UX & Menu Bar Design

### 8.1 Menu bar icon states
**DECIDED: Three states — On / Off / Error.**
- On: server running normally
- Off: server stopped
- Error: operational problem or fault
- Visual treatment (colour, SF Symbol variants) TBD during UI design

### 8.2 Primary menu structure
- Status block (model name, endpoint, params)
- Quick-switch (top N models)
- Start / Stop
- Settings window
- What else?

### 8.3 Settings window
- PoC is a 4-tab tkinter window — replace with SwiftUI
- Same tabs? (Status/Model, Generation, Advanced, System Prompt)
- Should it be a floating panel or a standard window?

### 8.4 Notifications
**DECIDED: Two-layer system — icon always, system notifications configurable.**
- **Layer 1 (always on):** menu bar icon reflects state change immediately — colour/animation for error/crash, normal transition for online
- **Layer 2 (configurable, default on):** macOS system notifications for all events
  - Error/crash → persistent alert (requires dismissal)
  - Server online / switch complete → slide-in banner (auto-dismisses)
- Single notification toggle in settings — on/off for the entire layer 2. No per-event granularity for MVP.
- Per-event notification config: deferred post-MVP

---

## 9. Configuration & Storage

### 9.1 Where does config live?
**DECIDED: `~/Library/Application Support/LocalBar/`**
- Format TBD (JSON most likely — human-readable, easy to version control)

### 9.2 Migration from Hermes
**DECIDED: Excluded.** Setup is too bespoke to build a migration path for. No other users will have it. Good onboarding UX covers the gap.

### 9.3 Portability
- Should config be iCloud-syncable? (put it in `~/Library/Mobile Documents/`?)
- Probably not for MVP

---

## 10. Process Lifecycle

### 10.1 Does LocalBar own the server process?
**DECIDED: Active controller.**
- LocalBar starts, stops, and restarts server processes
- Initial focus: mlx-lm and Ollama
- Server launch configs added incrementally as new server types are supported

### 10.2 If active: how do we start servers?
- mlx-lm: needs a virtualenv, specific CLI args, model path
- Ollama: `ollama serve`
- Each server type gets its own launch config schema, added over time
- See extensibility note in section 10.4

### 10.3 Crash recovery
- Deferred — decide after core process management is working

### 10.4 Extensibility approach
- LocalBar acts as a generic wrapper / controller for OpenAI-compatible inference servers
- Per-server launch configs are the extension point — each supported server type defines how it starts, stops, and what params it accepts
- This means we don't need to design a plugin system upfront; adding a new server type is adding a new config schema and handler

---

## 11. Open Source & Community

### 11.1 License
- MIT? Apache 2.0?
- Does it matter for App Store distribution?

### 11.2 Contributor experience
- Should the repo include a dev setup script from day one?
- CI from day one (GitHub Actions)?

### 11.3 Name & branding
**DECIDED: LocalBar.**
- No conflicting software found. Only prior use is a defunct 2012 BlackBerry PlayBook sideloading utility — no brand presence
- Domain / GitHub org: deferred until open source release

---

## 12. What's explicitly out of scope (for now)

Per the concept note — confirm these are still excluded:
- Model downloading / HuggingFace integration
- Multi-machine / remote servers
- Conversation history / chat UI
- Windows support
- Plugin system

---

## 13. Commercialization

**Decided:**
- Repo stays **private** until MVP is reached (solo development phase)
- At MVP, most likely path is **open source + optional paid support** (e.g. App Store one-time price as a "support the dev" model, while source remains public on GitHub)
- No paywalled features — the local LLM audience has strong open source expectations and will fork anything closed

**Deferred until MVP:**
- Exact license (MIT / Apache 2.0)
- Whether to use App Store, GitHub Sponsors, or both
- Pricing (if any)

---

## Next step

Go through each section above. For each question: **decide**, **defer**, or **exclude**.
