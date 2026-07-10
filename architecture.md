# LocalBar — Foundational Architecture Design

**Scope:** ServerDriver abstraction, server instance data model, process lifecycle state machine.
**Status:** Draft for review. Greenfield — no code exists yet.
**Targets:** macOS, Swift 5.10+/SwiftUI, strict concurrency (`Sendable` throughout).

---

## 0. Architectural overview

Three layers, with a hard seam between the middle two:

```
┌────────────────────────────────────────────────────────┐
│  UI layer (SwiftUI menu bar, settings, popups)         │
│  observes @Observable controllers, never touches       │
│  drivers or Process directly                           │
├────────────────────────────────────────────────────────┤
│  Control plane (generic, server-agnostic)              │
│  · ServerInstanceController — one per instance,        │
│    owns the state machine + the Process handle         │
│  · InstanceRegistry — owns the set of controllers,     │
│    persistence, profile/memory stores                  │
├────────────────────────────────────────────────────────┤
│  ServerDriver implementations (server-specific)        │
│  · MLXLMDriver, OllamaDriver (MVP)                     │
│  · stateless: describe capabilities, build launch      │
│    plans, translate params, speak each server's API    │
└────────────────────────────────────────────────────────┘
```

**The load-bearing decision:** drivers are *stateless describers and translators*. They never own a `Process`, never hold sockets open, never track state. All state (process handle, lifecycle phase, crash detection, retry logic) lives in the generic `ServerInstanceController`. This is what makes server #3 cheap: a new server type is a new value-semantics driver conforming to one protocol, and zero changes to the lifecycle machinery.

---

## 1. The `ServerDriver` protocol

### 1.1 Design rationale

The protocol is split conceptually into two halves:

1. **Static capability description** — what params exist, what flags exist, whether model switching needs a restart. These are `let`-style properties the UI reads to render itself (greyed-out params, tooltips with real flag names, per-type advanced flags). They must be answerable *without a running server*.
2. **Runtime operations** — build a launch plan, health-check, list models, switch models, read context usage. These are `async` and take the instance config as input, because the driver holds no state of its own.

Two abstractions do the heavy lifting:

- **`LaunchPlan`** — a pure value describing *how* to launch (binary, args, env, cwd). The driver computes it; the controller executes it. This keeps `Process`/`posix_spawn` details out of every driver and makes launch behaviour unit-testable without spawning anything.
- **`ParamDescriptor` / canonical params** — the canonical param layer is a fixed enum owned by LocalBar; each driver publishes a mapping from canonical params to its own flag names and encodings. Unsupported params are simply absent from the driver's schema — the UI derives "greyed out with explanation" from that absence.

### 1.2 The protocol

```swift
/// Identifies a server type. Raw string so persisted configs survive
/// driver additions/removals gracefully.
enum ServerType: String, Codable, CaseIterable, Sendable {
    case mlxLM = "mlx-lm"
    case ollama = "ollama"
    // llama.cpp, LM Studio added post-MVP
}

/// How a model switch is accomplished for this server type.
enum ModelSwitchBehavior: Sendable {
    /// Server can swap models via API without a restart (Ollama).
    case apiCall
    /// Process must be killed and relaunched with new model args (mlx-lm).
    /// LocalBar shows the warning popup + adaptive time estimate for these.
    case restartRequired
}

/// A pure-value description of how to spawn the server process.
/// Computed by the driver, executed by ServerInstanceController.
struct LaunchPlan: Sendable, Equatable {
    var executableURL: URL          // binary, or python inside a venv
    var arguments: [String]
    var environment: [String: String]  // merged over inherited env
    var workingDirectory: URL?
}

/// How the controller should ask the process to die.
struct ShutdownPlan: Sendable {
    /// Optional graceful step (e.g. an HTTP shutdown endpoint, or SIGINT
    /// preference). If nil, controller goes straight to SIGTERM.
    var gracefulRequest: GracefulShutdown?
    var gracePeriod: TimeInterval    // wait before SIGTERM → SIGKILL escalation

    enum GracefulShutdown: Sendable {
        case signal(Int32)           // e.g. SIGINT for clean mlx-lm exit
        case httpRequest(path: String, method: String)
    }
}

/// Result of a single health probe.
enum HealthStatus: Sendable, Equatable {
    case healthy
    case unhealthy(reason: String)   // reachable but wrong/failing response
    case unreachable                 // connection refused / timeout
}

/// A model as the server knows it. `key` is the stable identity used for
/// param auto-memory (Ollama tag, or mlx model repo/dir name).
struct ModelRef: Sendable, Codable, Equatable, Identifiable {
    var id: String { key }
    var key: String                  // "llama3.1:8b" or "mlx-community/Qwen2.5-7B-4bit"
    var displayName: String
    var sizeBytes: Int64?            // for restart-time estimation buckets
    var location: Location

    enum Location: Codable, Sendable, Equatable {
        case serverManaged           // Ollama's own store
        case filesystem(path: String) // mlx: HF cache dir or explicit path
    }
}

/// Context window usage — the MVP priority metric.
struct ContextUsage: Sendable, Equatable {
    var usedTokens: Int
    var maxTokens: Int
}

// ────────────────────────────────────────────────────────────────────

/// The seam between LocalBar's generic control plane and each server's
/// specific behaviour. Implementations MUST be stateless value types
/// (structs). All state lives in ServerInstanceController.
protocol ServerDriver: Sendable {

    // ── Static capability description ─────────────────────────────

    /// Which server type this driver handles. One driver per type.
    var serverType: ServerType { get }

    /// Whether switching models needs a restart or an API call.
    /// Drives the warning-popup-with-ETA flow in the UI.
    var modelSwitchBehavior: ModelSwitchBehavior { get }

    /// The canonical generation params this server supports, with their
    /// server-specific flag names and application mechanism. The UI:
    ///   · renders controls for params present here
    ///   · greys out canonical params absent here (with explanation)
    ///   · shows `serverFlagName` in the hover tooltip
    var paramSchema: [ParamDescriptor] { get }

    /// Advanced, server-type-specific flags (e.g. mlx-lm's
    /// --trust-remote-code, Ollama's OLLAMA_KEEP_ALIVE). Shown only
    /// in this server type's config UI.
    var flagSchema: [FlagDescriptor] { get }

    /// Validate a config before saving / launching. Returns issues,
    /// e.g. "python not found at path", "port below 1024".
    /// Pure and fast where possible; may touch the filesystem.
    func validate(config: ServerInstanceConfig) async -> [ConfigIssue]

    // ── Launch / shutdown ──────────────────────────────────────────

    /// Build the launch plan for this config + model + resolved params.
    /// Pure function of its inputs — no side effects, fully testable.
    /// Throws if the config is unlaunchable (missing binary, no model
    /// selected for a server type that requires one at launch, etc.).
    func makeLaunchPlan(
        config: ServerInstanceConfig,
        model: ModelRef?,
        params: ParamValues
    ) throws -> LaunchPlan

    /// How to stop this server type. Controller executes the plan and
    /// escalates (graceful → SIGTERM → SIGKILL) on timeout.
    func makeShutdownPlan(config: ServerInstanceConfig) -> ShutdownPlan

    // ── Runtime queries (server must be reachable) ────────────────

    /// One health probe. Called by the controller on its poll cadence
    /// and during startup confirmation. Must be cheap (single HTTP GET,
    /// short timeout supplied by controller via URLSession config).
    func healthCheck(config: ServerInstanceConfig) async -> HealthStatus

    /// List models available to this instance.
    ///   · Ollama: GET /api/tags (requires running server)
    ///   · mlx-lm: filesystem scan of HF cache / configured dirs —
    ///     works even when the server is stopped.
    /// `requiresRunningServer` (below) tells the UI whether to disable
    /// the model picker when the instance is down.
    func listModels(config: ServerInstanceConfig) async throws -> [ModelRef]
    var modelListRequiresRunningServer: Bool { get }

    /// Switch the active model WITHOUT a restart. Only called when
    /// `modelSwitchBehavior == .apiCall`. Drivers with .restartRequired
    /// should assertionFailure here; the controller handles their
    /// switches by re-running the launch pipeline.
    func switchModel(
        to model: ModelRef,
        params: ParamValues,
        config: ServerInstanceConfig
    ) async throws

    /// Fetch current context-window usage, or nil if this server/version
    /// doesn't expose it. UI shows "n/a" rather than erroring.
    func contextUsage(config: ServerInstanceConfig) async throws -> ContextUsage?
}
```

### 1.3 The canonical param layer

```swift
/// The canonical vocabulary. Owned by LocalBar, not by any driver.
/// Extending this enum is additive and safe.
enum CanonicalParam: String, Codable, CaseIterable, Sendable {
    case temperature, topP, topK, minP
    case maxTokens, repeatPenalty, seed
    case contextLength          // requested ctx size, where settable
    case systemPrompt           // in-app editing is in MVP
}

/// How a param value actually reaches the server. Critical because
/// LocalBar is not a proxy — it can only apply params it can set
/// server-side.
enum ParamApplication: Sendable, Equatable {
    /// Passed as a CLI flag at launch (mlx-lm defaults). Changing it
    /// on a running server implies a restart — UI must surface this.
    case launchArgument
    /// Applied via a server API call while running (no restart).
    case apiCall
    /// Baked into a server-side artifact (e.g. an Ollama Modelfile
    /// setting model defaults). Slow-ish but persistent.
    case serverSideDefault
}

struct ParamDescriptor: Sendable, Identifiable {
    var id: CanonicalParam { param }
    var param: CanonicalParam
    var serverFlagName: String       // e.g. "--temp", "options.temperature"
    var application: ParamApplication
    var valueType: ParamValueType    // .double(range:), .int(range:), .string, .bool
    var defaultValue: ParamValue?
    var note: String?                // shown in tooltip alongside flag name
}

enum ParamValueType: Sendable, Equatable {
    case double(range: ClosedRange<Double>?)
    case int(range: ClosedRange<Int>?)
    case string
    case bool
}

/// Advanced per-server-type flags (not part of the canonical layer).
struct FlagDescriptor: Sendable, Identifiable {
    var id: String { flagName }
    var flagName: String             // literal flag or env var name
    var displayName: String
    var help: String
    var valueType: ParamValueType
    var isEnvironmentVariable: Bool  // Ollama config is mostly env vars
    var defaultValue: ParamValue?
}

struct ConfigIssue: Sendable, Identifiable {
    enum Severity: Sendable { case warning, blocker }
    var id = UUID()
    var severity: Severity
    var message: String
    var fixSuggestion: String?
}
```

> **⚠️ Decision needed (not covered in the brief):** because LocalBar is not a proxy, generation params can only be enforced where the server supports *server-side defaults*. mlx-lm accepts launch-time defaults (`--temp` etc. — restart to change). Ollama applies most sampling params **per-request**, meaning external tools that send their own `options` will override anything LocalBar sets; enforcing defaults on Ollama means writing Modelfiles or using its limited env-var config. The design accommodates all three routes via `ParamApplication`, but product needs to decide how honest the Ollama param UI should be — I recommend showing an "advisory" badge on `.serverSideDefault` params: *"external tools may override this per request."* This is the single biggest UX/architecture tension in the locked decisions.

### 1.4 Driver registry

```swift
/// Static lookup. Adding server #3 = one new struct + one line here.
enum DriverRegistry {
    static let all: [ServerType: any ServerDriver] = [
        .mlxLM: MLXLMDriver(),
        .ollama: OllamaDriver(),
    ]
    static func driver(for type: ServerType) -> any ServerDriver { … }
}
```

### 1.5 Sketch of the two MVP drivers (to sanity-check the seam)

| Concern | `MLXLMDriver` | `OllamaDriver` |
|---|---|---|
| Launch | venv python, `-m mlx_lm.server --model <path> --port <p> --temp …` | `ollama serve` with `OLLAMA_HOST=127.0.0.1:<p>` env |
| Health | `GET /health` (falls back to `GET /v1/models`) | `GET /` returns "Ollama is running" |
| Model list | filesystem scan of HF cache + user dirs (works offline) | `GET /api/tags` (needs running server) |
| Switch | `.restartRequired` → controller re-launches | `.apiCall` → load via `POST /api/generate` with model name (empty prompt warms it) |
| Params | launch args (`--temp`, `--top-p`, `--max-tokens`) → `.launchArgument` | mostly per-request → `.serverSideDefault` via Modelfile, or advisory |
| Shutdown | SIGINT, 5 s grace | SIGTERM, 10 s grace (may be unloading a model) |

Both fit the protocol without contortion; the asymmetries (offline model list, env-var config, switch behaviour) are all expressed as declared capabilities rather than special cases in the control plane. That is the test the abstraction had to pass.

---

## 2. The data model

### 2.1 Layout on disk

```
~/Library/Application Support/LocalBar/
├── instances.json        # [ServerInstanceConfig] — the configured servers
├── profiles.json         # [NamedProfile] — user-created named snapshots
├── model-memory.json     # [ModelMemory] — auto-remembered params + timing samples
└── settings.json         # AppSettings — notification toggle, etc.
```

Separate files because they change at different rates (model-memory churns on every switch; instances rarely) and a corrupt file loses only its own domain. All top-level types carry a `schemaVersion: Int` for forward migration. Writes are atomic (`Data.write(options: .atomic)`) via a single serialized persistence actor.

### 2.2 Core types

```swift
/// One configured server instance. Pure config — NO runtime state
/// (no PID, no lifecycle phase). Runtime state lives in
/// ServerInstanceController and is never persisted.
struct ServerInstanceConfig: Codable, Identifiable, Equatable, Sendable {
    var schemaVersion: Int = 1
    let id: UUID
    var name: String                     // "Main (Qwen 72B)", "Draft model"
    var type: ServerType
    var host: String = "127.0.0.1"
    var port: Int

    /// Where the server lives. Driver-interpreted:
    ///  · mlx-lm: path to venv python (or uvx/pipx shim)
    ///  · Ollama: path to ollama binary (default /usr/local/bin/ollama)
    var executablePath: String

    /// Extra model search dirs (mlx-lm filesystem scanning).
    var modelSearchPaths: [String] = []

    /// Model key last selected for this instance. Resolved to a
    /// ModelRef via the driver at runtime; kept as a key so configs
    /// stay valid when models move.
    var selectedModelKey: String?

    /// Values for this server type's advanced FlagDescriptors,
    /// keyed by flagName. Unknown keys preserved on round-trip.
    var advancedFlags: [String: ParamValue] = [:]

    /// If set, this profile overrides per-model auto-memory.
    var activeProfileID: UUID?

    var startOnAppLaunch: Bool = false
}

/// A concrete param value. Codable via tagged enum so JSON stays
/// self-describing.
enum ParamValue: Codable, Equatable, Sendable {
    case double(Double)
    case int(Int)
    case string(String)
    case bool(Bool)
}

/// A bag of canonical param values. This is what gets remembered,
/// profiled, translated, and applied.
struct ParamValues: Codable, Equatable, Sendable {
    var values: [CanonicalParam: ParamValue] = [:]
    /// System prompt kept alongside sampling params — it is part of
    /// the "how this model behaves" snapshot users expect to restore.
    var systemPrompt: String?
}

/// A user-created named snapshot. Overrides per-model auto-memory
/// when active on an instance.
struct NamedProfile: Codable, Identifiable, Equatable, Sendable {
    var schemaVersion: Int = 1
    let id: UUID
    var name: String                     // "Creative", "Deterministic eval"
    var params: ParamValues
    /// nil = usable with any server type; set = created for one type
    /// (UI warns when applying across types — unsupported params are
    /// dropped at translation time, not stored per-type).
    var serverType: ServerType?
    var createdAt: Date
    var modifiedAt: Date
}

/// Auto-memory: last-used params per model, plus the timing samples
/// that feed the adaptive restart estimate.
struct ModelMemory: Codable, Identifiable, Equatable, Sendable {
    var schemaVersion: Int = 1
    var id: String { modelKey }
    var modelKey: String                 // ModelRef.key
    var lastUsedParams: ParamValues
    var lastUsedAt: Date

    /// Rolling window (keep last 10) of measured restart durations,
    /// i.e. stop → healthy for restart-required switches to this model.
    /// Estimate = EWMA over samples; first-ever switch falls back to a
    /// size-bucket heuristic (GB → seconds table shipped with the app).
    var restartDurationSamples: [TimeInterval] = []
}

struct AppSettings: Codable, Equatable, Sendable {
    var schemaVersion: Int = 1
    var systemNotificationsEnabled: Bool = true   // single toggle, MVP
}
```

### 2.3 Param resolution order

When an instance starts or switches models, effective params are resolved as:

```
active NamedProfile (if set on the instance)
  → else ModelMemory.lastUsedParams for the target model key
  → else driver ParamDescriptor.defaultValue per param
```

The resolved `ParamValues` then passes through the driver's `paramSchema` for **translation**: params absent from the schema are silently dropped from what is sent/launched (they remain stored — switching to a server that supports them restores them). This resolution lives in the control plane, not in drivers.

> **⚠️ Decision needed:** the brief keys auto-memory by "model name/filename". `ModelRef.key` follows that, but the same model pulled in Ollama (`llama3.1:8b`) and mlx (`mlx-community/Llama-3.1-8B…`) will have different keys and therefore separate memories. I've assumed that is acceptable for MVP (arguably even correct — params tune differently per runtime). Confirm.

### 2.4 Runtime layer (not persisted)

```swift
@Observable @MainActor
final class ServerInstanceController: Identifiable {
    let id: UUID                       // == config.id
    private(set) var config: ServerInstanceConfig
    private(set) var phase: InstancePhase = .stopped(.neverStarted)
    private(set) var currentModel: ModelRef?
    private(set) var contextUsage: ContextUsage?
    private(set) var lastError: InstanceError?

    private let driver: any ServerDriver
    private var process: Process?      // only while managed
    private var healthPollTask: Task<Void, Never>?
    // start(), stop(), restart(), switchModel(to:) — see §3
}

@Observable @MainActor
final class InstanceRegistry {
    private(set) var controllers: [ServerInstanceController]
    // derived: overall menu-bar icon state = max-severity fold over
    // controllers (any .error → Error; any transitioning → Transitioning;
    // any running → On; else Off)
}
```

---

## 3. Process lifecycle state machine

### 3.1 States

```swift
enum InstancePhase: Equatable, Sendable {
    case stopped(StopReason)      // no process; StopReason: .neverStarted, .userStopped, .exitedCleanly
    case starting                 // process spawned, awaiting first healthy probe
    case running                  // healthy; health poll active
    case stopping                 // shutdown plan executing (graceful → TERM → KILL)
    case switchingModel           // restart-required switch: composite stopping→starting
    case error(InstanceError)     // crashed / failed to start / unhealthy / port conflict
}

struct InstanceError: Equatable, Sendable {
    enum Kind: Sendable { case launchFailed, crashed(exitCode: Int32?),
                          healthCheckFailed, portConflict, shutdownTimedOut }
    var kind: Kind
    var message: String
    var occurredAt: Date
}
```

Mapping to the four locked icon states: `running` → **On**; `stopped` → **Off**; `error` → **Error**; `starting` / `stopping` / `switchingModel` → **Transitioning**.

`switchingModel` is a distinct state (not just `stopping`+`starting`) because the UI needs to show the ETA countdown and suppress the "server stopped" notification mid-switch, and the timing sample must span the whole composite operation.

### 3.2 Diagram

```
                          user Start / startOnAppLaunch / retry
        ┌──────────────────────────────────────────────────────────┐
        │                                                          │
        ▼                                                          │
  ┌──────────┐  spawn OK   ┌──────────┐  first healthy  ┌─────────┴┐
  │ STOPPED  │────────────▶│ STARTING │────────────────▶│ RUNNING  │
  └──────────┘             └──────────┘  probe          └──────────┘
        ▲                     │                        │   │    │
        │        spawn fails, │                        │   │    │ user Stop /
        │        early exit,  │              process   │   │    │ app quit
        │        health t/o,  │              exits     │   │    ▼
        │        port in use  │              unexpect- │   │  ┌──────────┐
        │                     ▼              edly, or  │   │  │ STOPPING │
        │                ┌──────────┐        N failed  │   │  └──────────┘
        │   user Retry / │  ERROR   │◀───────probes────┘   │    │      │
        │   user Stop    └──────────┘                      │    │exit  │kill
        │   (acknowledge)     ▲  │                         │    │conf. │t/o──▶ ERROR
        │                     │  └── user Retry ─▶ STARTING│    ▼      (shutdownTimedOut)
        │                     │                            │ STOPPED(.userStopped)
        │                     │            restart-required│
        │                     │            model switch    ▼
        │                     │  failure  ┌────────────────────┐
        │                     └───────────│  SWITCHING_MODEL   │
        │                                 │ (stop → relaunch → │
        └── clean composite abort ────────│  await healthy)    │──▶ RUNNING
                                          └────────────────────┘   (records duration
                                                                    sample on success)
```

### 3.3 Transition table

| From | Trigger | To | Side effects |
|---|---|---|---|
| stopped | user Start / app-launch autostart | starting | pre-flight: `validate()`, port-free check; resolve params; `makeLaunchPlan`; spawn `Process`; install `terminationHandler`; start startup timeout (default 120 s, configurable) |
| starting | health probe returns `.healthy` | running | begin poll loop (5 s cadence); notify "started"; icon → On |
| starting | process exits before healthy | error(.launchFailed / .crashed) | capture last N KB of stderr into error message; notify |
| starting | startup timeout elapses | error(.healthCheckFailed) | send ShutdownPlan (best effort), then kill |
| starting | pre-flight port check fails | error(.portConflict) | never spawns; message names the port and (if resolvable via `lsof`) the squatter |
| running | health probe fails **3 consecutive** times | error(.healthCheckFailed) | process may still be alive → execute shutdown plan; notify |
| running | `terminationHandler` fires unexpectedly | error(.crashed(exitCode:)) | this is the crash detector — push-based, not poll-based; notify |
| running | user Stop / app quitting | stopping | execute ShutdownPlan: graceful → SIGTERM at gracePeriod → SIGKILL at 2× |
| running | model switch, `.apiCall` driver | running (no state change) | driver `switchModel()`; on throw → surface error banner, stay running |
| running | model switch, `.restartRequired` | switchingModel | after user confirms ETA popup; start duration stopwatch |
| stopping | termination confirmed | stopped(.userStopped) | icon → Off; notify (unless app quitting) |
| stopping | SIGKILL also times out | error(.shutdownTimedOut) | pathological; surface PID so user can act |
| switchingModel | relaunch reaches healthy | running | record duration sample into `ModelMemory`; update `selectedModelKey`; persist |
| switchingModel | any step fails | error(kind of failing step) | old model is already down — do NOT auto-rollback in MVP (flagged below) |
| error | user Retry | starting | clears lastError |
| error | user Stop/acknowledge | stopped(.userStopped) | clears process handle if any |

Transitions not listed are illegal; the controller funnels every trigger through a single `transition(_:)` method on the MainActor that validates against this table and `assertionFailure`s on violations in debug builds (logs + ignores in release).

### 3.4 Crash detection — the two channels

1. **Push:** `Process.terminationHandler` fires the instant the child exits. Any firing while phase ∉ {stopping, switchingModel} is by definition unexpected → `error(.crashed)`. This catches the common case in <100 ms.
2. **Poll:** the 5 s health loop catches the zombie case — process alive but server wedged (deadlocked, OOM-thrashing, socket leaked). 3 consecutive failures (≈15 s) before declaring error avoids flapping during heavy generation load, when servers legitimately respond slowly. Probe timeout: 3 s.

Because LocalBar spawns children directly (not via launchd), children die with the app. **Decision needed:** is that acceptable for MVP? If servers should outlive LocalBar (or survive its crash), the process model changes materially (launchd agents or detached daemons + PID-file reattachment), and "LocalBar owns the full lifecycle" gets harder. I recommend children-die-with-app for MVP — it's also the honest reading of the locked decision — but it should be an explicit product call.

---

## 4. If this design is wrong — top risks and early signals

**Risk 1: The no-proxy + params combination under-delivers (highest risk).**
Assumption: users mainly want params as *server-side defaults*. But Ollama applies sampling per-request, so external tools silently override LocalBar's settings — users will set temperature in LocalBar, see no effect, and file it as a bug. **Early signal:** during OllamaDriver implementation, count how many canonical params end up `.serverSideDefault`-only; if it's most of them, the param panel is theatre for Ollama. **Escape hatch:** the `ParamApplication` enum already carves the space; adding an *optional* per-instance transparent proxy later would be a new application mode, not a protocol rewrite. Don't remove the enum "for simplicity."

**Risk 2: "LocalBar owns the process" doesn't generalize to server #3/#4.**
LM Studio is a GUI app with its own daemon; many users already run Ollama via its login item. The protocol currently assumes every instance is spawn-managed. **Early signal:** the first support question that says "LocalBar shows my Ollama as stopped but it's running." **Escape hatch:** add `enum LifecycleOwnership { case managed, attached }` to `ServerInstanceConfig`; `attached` instances skip spawn/kill and use only healthCheck/listModels/switchModel. The driver protocol needs *zero* changes for this — only the controller grows a second, smaller transition table (no starting/stopping states). That this bolt-on is cheap is deliberate; if it ever stops being cheap, the abstraction has failed.

**Risk 3: Static `paramSchema` vs. real server version drift.**
mlx-lm renames/adds flags across releases faster than LocalBar ships. A hardcoded schema means launch failures on `--flag-that-no-longer-exists`. **Early signal:** `error(.launchFailed)` whose stderr contains "unrecognized arguments" — log that pattern specifically and count it in (opt-in) diagnostics. **Escape hatch:** make `paramSchema` a function of a detected server version (`func paramSchema(version: ServerVersion?)`); the property form is the degenerate case, so migrating is additive.

**Risk 4: Context-window monitoring may be unobservable without a proxy.**
Neither mlx-lm nor Ollama reliably exposes "current context fill" for requests LocalBar didn't make. The priority MVP metric might be structurally unavailable. **Early signal:** week one of driver spike work — verify what `contextUsage()` can actually return against real servers *before* building the UI around it. If the answer is "nothing useful," the honest options are (a) approximate from Ollama's `/api/ps` loaded-model info, or (b) accept this metric as the first argument for the optional proxy. This should be validated first, since it can invalidate a headline feature.

**Risk 5: Model identity keying by name/filename.**
Two quantizations of the same repo, or a re-pulled Ollama tag pointing at new weights, collide or silently reset memory. **Early signal:** user reports of "my params reset" or "wrong params restored." **Escape hatch:** `ModelRef.key` is already an opaque string; enriching it (digest suffix for Ollama, path hash for mlx) is a data migration, not a model change.

**What I'd validate in the first spike (ordered):** (1) context-usage observability per server, (2) Ollama param enforcement reality, (3) mlx-lm restart timing variance — if restart times vary wildly with system memory pressure, the EWMA estimate needs a load-aware term or humbler UI copy ("usually ~40 s").

---

## Appendix: decisions flagged for the owner

| # | Question | Recommendation |
|---|---|---|
| D1 | How to present unenforceable (per-request-overridable) params on Ollama | Advisory badge, not greyed out |
| D2 | Separate param memory for the "same" model on different runtimes | Yes, keep separate (per-runtime tuning differs) |
| D3 | Do managed servers die when LocalBar quits/crashes? | Yes for MVP (children of app process) |
| D4 | Rollback to previous model if a restart-required switch fails mid-flight? | No auto-rollback MVP; show one-click "restart with previous model" in the error state |
| D5 | Startup timeout default (large models can take minutes to load) | 120 s default, per-instance override in advanced flags |
