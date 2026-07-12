import Foundation

// MARK: - Param value

/// A concrete param value. Codable via tagged enum so JSON stays self-describing.
enum ParamValue: Codable, Equatable, Sendable, Hashable {
    case double(Double)
    case int(Int)
    case string(String)
    case bool(Bool)
}

/// A bag of canonical param values.
struct ParamValues: Codable, Equatable, Sendable {
    var values: [CanonicalParam: ParamValue] = [:]
    var systemPrompt: String?

    subscript(param: CanonicalParam) -> ParamValue? {
        get { values[param] }
        set { values[param] = newValue }
    }
}

// MARK: - Server instance config

/// One configured server instance. Pure config — NO runtime state
/// (no PID, no lifecycle phase). Runtime state lives in ServerInstanceController.
struct ServerInstanceConfig: Codable, Identifiable, Equatable, Sendable {
    var schemaVersion: Int = 1
    let id: UUID
    var name: String
    var type: ServerType
    var host: String = "127.0.0.1"
    var port: Int

    /// Driver-interpreted path:
    ///  · mlx-lm: path to venv python (or uvx/pipx shim)
    ///  · Ollama: path to ollama binary
    var executablePath: String

    var modelSearchPaths: [String] = []

    /// Model key last selected for this instance. Resolved to a ModelRef
    /// via the driver at runtime; kept as a key so configs stay valid
    /// when models move.
    var selectedModelKey: String?

    /// Values for this server type's advanced FlagDescriptors, keyed by flagName.
    var advancedFlags: [String: ParamValue] = [:]

    /// If set, this profile overrides per-model auto-memory.
    var activeProfileID: UUID?

    /// Ollama only: the LocalBar-managed model tag (`localbar/<modelName>-<instanceId>`)
    /// created via `ollama create`. nil for mlx-lm instances and until a model is
    /// first selected on an Ollama instance.
    var managedModelTag: String?

    var startOnAppLaunch: Bool = false

    init(name: String, type: ServerType, port: Int, executablePath: String) {
        self.id = UUID()
        self.name = name
        self.type = type
        self.port = port
        self.executablePath = executablePath
    }
}

// MARK: - Named profile

/// A user-created named param snapshot. Overrides per-model auto-memory
/// when active on an instance.
struct NamedProfile: Codable, Identifiable, Equatable, Sendable {
    var schemaVersion: Int = 1
    let id: UUID
    var name: String
    var params: ParamValues
    /// nil = usable with any server type.
    var serverType: ServerType?
    var createdAt: Date
    var modifiedAt: Date

    init(name: String, params: ParamValues, serverType: ServerType? = nil) {
        self.id = UUID()
        self.name = name
        self.params = params
        self.serverType = serverType
        self.createdAt = Date()
        self.modifiedAt = Date()
    }
}

// MARK: - Model memory

/// Auto-memory: last-used params per model, plus timing samples for
/// the adaptive restart estimate.
struct ModelMemory: Codable, Identifiable, Equatable, Sendable {
    var schemaVersion: Int = 1
    var id: String { modelKey }
    /// D2: keyed by ModelRef.key (runtime-scoped — separate entries for
    /// Ollama and mlx-lm even when underlying weights are the same).
    var modelKey: String
    var lastUsedParams: ParamValues
    var lastUsedAt: Date

    /// Rolling window of measured stop→healthy durations (keep last 10).
    /// Estimate = EWMA over samples. First switch falls back to a
    /// size-bucket heuristic shipped with the app.
    var restartDurationSamples: [TimeInterval] = []

    var estimatedRestartDuration: TimeInterval? {
        guard !restartDurationSamples.isEmpty else { return nil }
        // EWMA with α = 0.3, most-recent sample first.
        var ewma = restartDurationSamples[0]
        for sample in restartDurationSamples.dropFirst() {
            ewma = 0.3 * sample + 0.7 * ewma
        }
        return ewma
    }

    mutating func recordRestartDuration(_ duration: TimeInterval) {
        restartDurationSamples.insert(duration, at: 0)
        if restartDurationSamples.count > 10 {
            restartDurationSamples.removeLast()
        }
    }
}

// MARK: - App settings

struct AppSettings: Codable, Equatable, Sendable {
    var schemaVersion: Int = 1
    var systemNotificationsEnabled: Bool = true
}

// MARK: - Param resolution

extension ParamValues {
    /// Resolve effective params for a given model, applying the
    /// canonical resolution order:
    ///   active profile → model memory → driver defaults
    static func resolve(
        profile: NamedProfile?,
        memory: ModelMemory?,
        driverDefaults: [ParamDescriptor]
    ) -> ParamValues {
        var resolved = ParamValues()

        // 1. Driver defaults (lowest priority)
        for descriptor in driverDefaults {
            if let def = descriptor.defaultValue {
                resolved.values[descriptor.param] = def
            }
        }

        // 2. Model memory
        if let memory {
            for (param, value) in memory.lastUsedParams.values {
                resolved.values[param] = value
            }
            if let prompt = memory.lastUsedParams.systemPrompt {
                resolved.systemPrompt = prompt
            }
        }

        // 3. Active profile (highest priority)
        if let profile {
            for (param, value) in profile.params.values {
                resolved.values[param] = value
            }
            if let prompt = profile.params.systemPrompt {
                resolved.systemPrompt = prompt
            }
        }

        return resolved
    }

    /// Filter to only params supported by the given driver schema,
    /// for building a LaunchPlan or API call.
    func filtered(to schema: [ParamDescriptor]) -> ParamValues {
        let supported = Set(schema.map(\.param))
        var filtered = ParamValues()
        filtered.values = values.filter { supported.contains($0.key) }
        filtered.systemPrompt = systemPrompt
        return filtered
    }
}
