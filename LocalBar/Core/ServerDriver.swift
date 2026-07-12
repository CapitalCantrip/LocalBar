import Foundation
import Darwin

// MARK: - Server type identity

/// Identifies a server type. Raw string so persisted configs survive
/// driver additions/removals gracefully.
enum ServerType: String, Codable, CaseIterable, Sendable {
    case mlxLM  = "mlx-lm"
    case ollama = "ollama"
}

// MARK: - Model switch behaviour

/// How a model switch is accomplished for this server type.
enum ModelSwitchBehavior: Sendable {
    /// Server can swap models via API without a restart (Ollama).
    case apiCall
    /// Process must be killed and relaunched with new model args (mlx-lm).
    /// LocalBar shows the warning popup + adaptive time estimate for these.
    case restartRequired
}

// MARK: - Launch / shutdown plans

/// A pure-value description of how to spawn the server process.
/// Computed by the driver, executed by ServerInstanceController.
struct LaunchPlan: Sendable, Equatable {
    var executableURL: URL
    var arguments: [String]
    /// Merged over the inherited environment.
    var environment: [String: String]
    var workingDirectory: URL?
}

/// How the controller should ask the process to stop.
struct ShutdownPlan: Sendable {
    var gracefulRequest: GracefulShutdown?
    /// Wait this long before escalating to SIGTERM; SIGKILL follows at 2×.
    var gracePeriod: TimeInterval

    enum GracefulShutdown: Sendable {
        case signal(Int32)
        case httpRequest(path: String, method: String)
    }
}

// MARK: - Health

/// Result of a single health probe.
enum HealthStatus: Sendable, Equatable {
    case healthy
    case unhealthy(reason: String)
    case unreachable
}

// MARK: - Model metadata

/// Best-effort descriptive metadata about a model, parsed from its
/// name and on-disk config files. All fields optional/empty when unknown.
struct ModelMetadata: Sendable, Codable, Equatable {
    /// "7B", "27B", "1.5B" — parsed from name/config.
    var parameterCount: String?
    /// "4bit", "8bit", "Q4_K_M", "Q8_0", "f16" etc.
    var quantization: String?
    var modelFormat: ModelFormat = .other
    var capabilities: Set<Capability> = []

    // Architecture fields used for KV-cache memory estimation.
    // Parsed from config.json (mlx-lm) or /api/show (Ollama).
    var numHiddenLayers: Int?
    /// GQA-aware: num_key_value_heads; falls back to num_attention_heads.
    var numKVHeads: Int?
    /// head_dim = hidden_size / num_attention_heads when not explicit.
    var headDim: Int?

    enum ModelFormat: String, Codable, Sendable, Equatable {
        case mlx = "MLX"
        case gguf = "GGUF"
        case safetensors = "SafeTensors"
        case other = "Other"
    }

    enum Capability: String, Codable, Sendable, Hashable {
        case toolUse = "Tool Use"
        case vision = "Vision"
        case code = "Code"
        case embedding = "Embedding"
    }
}

// MARK: - Model reference

/// A model as the server knows it.
struct ModelRef: Sendable, Codable, Equatable, Identifiable {
    var id: String { key }
    /// Stable identity used for param auto-memory.
    /// Ollama: tag string ("llama3.1:8b"). mlx-lm: HF repo/dir path.
    var key: String
    var displayName: String
    var sizeBytes: Int64?
    var location: Location
    /// Best-effort descriptive metadata (nil when the driver can't parse any).
    var metadata: ModelMetadata? = nil

    enum Location: Codable, Sendable, Equatable {
        case serverManaged
        case filesystem(path: String)
    }
}

// MARK: - Context usage

/// Context window usage — the MVP priority metric.
struct ContextUsage: Sendable, Equatable {
    var usedTokens: Int
    var maxTokens: Int
}

// MARK: - Param types

/// The canonical vocabulary. Owned by LocalBar, not by any driver.
enum CanonicalParam: String, Codable, CaseIterable, Sendable, Hashable, CodingKey {
    case temperature
    case topP
    case topK
    case minP
    case maxTokens
    case repeatPenalty
    case presencePenalty
    case seed
    case contextLength
    case systemPrompt
}

/// How a param value actually reaches the server.
enum ParamApplication: Sendable, Equatable {
    /// Passed as a CLI flag at launch. Changing it implies a restart.
    case launchArgument
    /// Applied via a server API call while running (no restart).
    case apiCall
    /// Baked into a server-side artifact (e.g. Ollama Modelfile).
    /// D1: UI shows an advisory badge on these for Ollama.
    case serverSideDefault
}

enum ParamValueType: Sendable, Equatable {
    case double(range: ClosedRange<Double>?)
    case int(range: ClosedRange<Int>?)
    case string
    case bool
}

struct ParamDescriptor: Sendable, Identifiable {
    var id: CanonicalParam { param }
    var param: CanonicalParam
    /// The actual server flag name — shown in hover tooltips.
    var serverFlagName: String
    var application: ParamApplication
    var valueType: ParamValueType
    var defaultValue: ParamValue?
    /// Shown in tooltip alongside the flag name.
    var note: String?
}

/// Advanced, server-type-specific flags (not part of the canonical layer).
struct FlagDescriptor: Sendable, Identifiable {
    var id: String { flagName }
    var flagName: String
    var displayName: String
    var help: String
    var valueType: ParamValueType
    var isEnvironmentVariable: Bool
    var defaultValue: ParamValue?
}

struct ConfigIssue: Sendable, Identifiable {
    enum Severity: Sendable { case warning, blocker }
    var id = UUID()
    var severity: Severity
    var message: String
    var fixSuggestion: String?
}

// MARK: - ServerDriver protocol

/// The seam between LocalBar's generic control plane and each server's
/// specific behaviour. Implementations MUST be stateless value types (structs).
protocol ServerDriver: Sendable {

    // MARK: Static capability description

    var serverType: ServerType { get }
    var modelSwitchBehavior: ModelSwitchBehavior { get }
    var paramSchema: [ParamDescriptor] { get }
    var flagSchema: [FlagDescriptor] { get }
    var modelListRequiresRunningServer: Bool { get }

    func validate(config: ServerInstanceConfig) async -> [ConfigIssue]

    // MARK: Launch / shutdown

    func makeLaunchPlan(
        config: ServerInstanceConfig,
        model: ModelRef?,
        params: ParamValues
    ) throws -> LaunchPlan

    func makeShutdownPlan(config: ServerInstanceConfig) -> ShutdownPlan

    // MARK: Runtime queries

    func healthCheck(config: ServerInstanceConfig) async -> HealthStatus
    func listModels(config: ServerInstanceConfig) async throws -> [ModelRef]
    func switchModel(to model: ModelRef, params: ParamValues, config: ServerInstanceConfig) async throws
    func contextUsage(config: ServerInstanceConfig) async throws -> ContextUsage?

    // MARK: Managed model lifecycle (Ollama only — default no-ops for other drivers)

    /// Derive the managed model tag for a given base model key and instance ID.
    /// Returns nil for drivers that don't use managed models (e.g. mlx-lm).
    func managedModelTag(for modelKey: String, instanceId: String) -> String?

    /// Create (or recreate) a managed model Modelfile via `ollama create`.
    /// Returns the managed tag on success, nil if unsupported or failed.
    /// `params` is used to bake sampling defaults into the Modelfile.
    func createManagedModel(
        baseTag: String,
        instanceId: String,
        params: ParamValues,
        executablePath: String
    ) async -> String?

    /// Remove a LocalBar-managed model via `ollama rm`. No-op if unsupported.
    func removeManagedModel(tag: String, executablePath: String) async
}

// MARK: - Default implementations (no-op for non-Ollama drivers)

extension ServerDriver {
    func managedModelTag(for modelKey: String, instanceId: String) -> String? { nil }
    func createManagedModel(baseTag: String, instanceId: String, params: ParamValues, executablePath: String) async -> String? { nil }
    func removeManagedModel(tag: String, executablePath: String) async {}
}

// MARK: - Memory footprint estimation

/// Breakdown of an estimated memory footprint for display in the warning sheet.
struct MemoryFootprintEstimate: Sendable {
    /// Estimated weight bytes (nil if unknown).
    var weightBytes: Int64? = nil
    /// Estimated KV-cache bytes at the configured context length (nil if unknown).
    var kvCacheBytes: Int64? = nil
    /// Total estimated bytes consumed (sum of non-nil parts; 0 if nothing known).
    var totalBytes: Int64 { (weightBytes ?? 0) + (kvCacheBytes ?? 0) }
}

/// Total physical RAM in bytes via `hw.memsize`. Returns 0 on failure.
func systemTotalMemoryBytes() -> Int64 {
    var value: Int64 = 0
    var size = MemoryLayout<Int64>.size
    sysctlbyname("hw.memsize", &value, &size, nil, 0)
    return value
}

extension ModelMetadata {
    /// Estimated weight footprint in bytes.
    /// Uses `sizeBytes` from the ModelRef when available (preferred for Ollama).
    /// Falls back to parameterCount × bitsPerParam heuristic.
    func estimatedWeightBytes(directSizeBytes: Int64? = nil) -> Int64? {
        if let direct = directSizeBytes, direct > 0 { return direct }
        guard let paramStr = parameterCount,
              let paramDouble = parseParamCountToDouble(paramStr) else { return nil }
        let bits = bitsPerParam(for: quantization)
        return Int64(paramDouble * bits / 8.0)
    }

    /// Estimated KV-cache footprint in bytes for a given context length.
    /// - Parameters:
    ///   - contextLength: Active context window size (tokens).
    ///   - kvCacheBits: Bits per element (16 = bf16 default, 8 or 4 if quantised).
    func estimatedKVCacheBytes(contextLength: Int, kvCacheBits: Int = 16) -> Int64? {
        guard let layers = numHiddenLayers,
              let kvHeads = numKVHeads,
              let dim = headDim else { return nil }
        let bytesPerElement = Double(kvCacheBits) / 8.0
        // 2 = K + V tensors.
        return Int64(2.0 * Double(layers) * Double(kvHeads) * Double(dim) * Double(contextLength) * bytesPerElement)
    }

    // MARK: Private helpers

    private func parseParamCountToDouble(_ s: String) -> Double? {
        // Accepts "7B", "1.5B", "70B" etc.
        guard s.hasSuffix("B") || s.hasSuffix("b") else { return nil }
        let digits = String(s.dropLast())
        guard let d = Double(digits) else { return nil }
        return d * 1_000_000_000.0
    }

    private func bitsPerParam(for quantization: String?) -> Double {
        guard let q = quantization?.lowercased() else { return 16.0 }
        // GGUF Q4 variants: ~4.5 effective bits
        if q.hasPrefix("q4") { return 4.5 }
        if q.hasPrefix("q5") { return 5.5 }
        if q.hasPrefix("q6") { return 6.5 }
        if q.hasPrefix("q8") { return 8.0 }
        if q.contains("4bit") || q.contains("4-bit") { return 4.5 }
        if q.contains("8bit") || q.contains("8-bit") { return 8.0 }
        if q.contains("f16") || q.contains("fp16") || q.contains("bf16") { return 16.0 }
        return 16.0 // conservative default
    }
}
