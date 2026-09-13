import Foundation

/// Driver for mlx-lm (and mlx-vlm).
///
/// Launch: venv python -m mlx_lm.server --model <path> --port <p> [params as CLI flags]
/// Health: GET /health (falls back to GET /v1/models)
/// Models: filesystem scan of HF cache + configured dirs (works offline)
/// Switch: .restartRequired — controller kills and relaunches with new --model
/// Params: .launchArgument — restart required to change any param
/// Shutdown: SIGINT, 5 s grace
struct MLXLMDriver: ServerDriver {

    var serverType: ServerType { .mlxLM }
    var modelSwitchBehavior: ModelSwitchBehavior { .restartRequired }
    var modelListRequiresRunningServer: Bool { false }

    // MARK: Param schema

    var paramSchema: [ParamDescriptor] {
        [
            ParamDescriptor(param: .contextLength,  serverFlagName: "--max-kv-size",         application: .launchArgument, valueType: .int(range: 512...128_000),    defaultValue: nil,           note: "Restart required to apply"),
            ParamDescriptor(param: .temperature,    serverFlagName: "--temp",                application: .launchArgument, valueType: .double(range: 0.0...2.0),     defaultValue: .double(0.0),  note: "Restart required to apply"),
            ParamDescriptor(param: .maxTokens,      serverFlagName: "--max-tokens",          application: .launchArgument, valueType: .int(range: 1...128_000),       defaultValue: nil,           note: "Limits response length. Restart required"),
            ParamDescriptor(param: .topK,           serverFlagName: "--top-k",               application: .launchArgument, valueType: .int(range: 0...200),           defaultValue: nil,           note: "Restart required to apply"),
            ParamDescriptor(param: .repeatPenalty,  serverFlagName: "--repetition-penalty",  application: .launchArgument, valueType: .double(range: 1.0...2.0),     defaultValue: nil,           note: "Restart required to apply"),
            ParamDescriptor(param: .presencePenalty, serverFlagName: "--presence-penalty",   application: .launchArgument, valueType: .double(range: 0.0...2.0),     defaultValue: nil,           note: "Restart required to apply"),
            ParamDescriptor(param: .topP,           serverFlagName: "--top-p",               application: .launchArgument, valueType: .double(range: 0.0...1.0),     defaultValue: nil,           note: "Restart required to apply"),
            ParamDescriptor(param: .minP,           serverFlagName: "--min-p",               application: .launchArgument, valueType: .double(range: 0.0...1.0),     defaultValue: nil,           note: "Restart required to apply"),
            ParamDescriptor(param: .seed,           serverFlagName: "--seed",                application: .launchArgument, valueType: .int(range: 0...Int.max),       defaultValue: nil,           note: "Restart required to apply"),
        ]
    }

    var flagSchema: [FlagDescriptor] {
        [
            FlagDescriptor(flagName: "--kv-cache-bits",     displayName: "KV Cache Quantization", help: "Reduce KV cache memory usage: 8 = ~half, 4 = ~quarter of bf16. Omit to use bf16 (default).", valueType: .int(range: 4...8), isEnvironmentVariable: false, defaultValue: nil),
            FlagDescriptor(flagName: "--trust-remote-code", displayName: "Trust Remote Code",      help: "Allow execution of remote model code (use only with trusted models).",                        valueType: .bool, isEnvironmentVariable: false, defaultValue: .bool(false)),
            FlagDescriptor(flagName: "--log-level",         displayName: "Log Level",              help: "Logging verbosity: DEBUG, INFO, WARNING, ERROR.",                                             valueType: .string, isEnvironmentVariable: false, defaultValue: .string("INFO")),
            FlagDescriptor(flagName: "startupTimeoutSeconds", displayName: "Startup Timeout (s)",  help: "How long to wait for the server to become healthy before declaring an error. Default: 120 s.", valueType: .int(range: 30...600), isEnvironmentVariable: false, isLocalOnly: true, defaultValue: .int(120)),
        ]
    }

    // MARK: Validation

    func validate(config: ServerInstanceConfig) async -> [ConfigIssue] {
        var issues: [ConfigIssue] = []
        let execURL = URL(fileURLWithPath: config.executablePath)
        if !FileManager.default.fileExists(atPath: execURL.path) {
            issues.append(ConfigIssue(
                severity: .blocker,
                message: "Executable not found at '\(config.executablePath)'.",
                fixSuggestion: "Use uvx (e.g. /usr/local/bin/uvx) for a zero-install setup, or point to a Python venv that has mlx-lm installed."
            ))
        }
        if config.port < 1024 {
            issues.append(ConfigIssue(severity: .warning, message: "Port \(config.port) is below 1024 and may require root privileges."))
        }
        if config.selectedModelKey == nil {
            issues.append(ConfigIssue(severity: .blocker, message: "No model selected. mlx-lm requires a model path at launch.", fixSuggestion: "Select a model in the instance settings."))
        }
        return issues
    }

    // MARK: Launch / shutdown

    func makeLaunchPlan(
        config: ServerInstanceConfig,
        model: ModelRef?,
        params: ParamValues
    ) throws -> LaunchPlan {
        guard let model else {
            throw DriverError.modelRequired(serverType: .mlxLM)
        }
        guard case .filesystem(let path) = model.location else {
            throw DriverError.unexpectedModelLocation(model: model)
        }
        let isUvx = config.executablePath.hasSuffix("/uvx") || config.executablePath == "uvx"
        var args = baseArgs(isUvx: isUvx, path: path, config: config)
        args += paramArgs(schema: paramSchema, params: params)
        args += flagArgs(schema: flagSchema, config: config)
        return LaunchPlan(
            executableURL: URL(fileURLWithPath: config.executablePath),
            arguments: args,
            environment: [:],
            workingDirectory: nil
        )
    }

    /// Build the fixed prefix args for the mlx_lm.server command.
    private func baseArgs(isUvx: Bool, path: String, config: ServerInstanceConfig) -> [String] {
        // Support two invocation modes:
        //  · uvx    → `uvx --from mlx-lm mlx_lm.server --model <path> ...`
        //  · python → `python3 -m mlx_lm.server --model <path> ...`
        var args: [String] = isUvx
            ? ["--from", "mlx-lm", "mlx_lm.server"]
            : ["-m", "mlx_lm.server"]
        args += ["--model", path, "--host", config.host, "--port", String(config.port)]
        return args
    }

    /// Translate canonical params to CLI flag pairs.
    private func paramArgs(schema: [ParamDescriptor], params: ParamValues) -> [String] {
        var args: [String] = []
        for descriptor in schema {
            guard let value = params[descriptor.param] else { continue }
            appendFlag(value, name: descriptor.serverFlagName, to: &args)
        }
        return args
    }

    /// Translate advanced flags to CLI flag pairs, skipping local-only and env-var entries.
    private func flagArgs(schema: [FlagDescriptor], config: ServerInstanceConfig) -> [String] {
        var args: [String] = []
        for descriptor in schema where !descriptor.isEnvironmentVariable && !descriptor.isLocalOnly {
            guard let value = config.advancedFlags[descriptor.flagName] else { continue }
            appendFlag(value, name: descriptor.flagName, to: &args)
        }
        return args
    }

    /// Append `name` and the string form of `value` to `args`.
    /// For bool values, the flag is omitted entirely when false.
    private func appendFlag(_ value: ParamValue, name: String, to args: inout [String]) {
        args.append(name)
        switch value {
        case .double(let d): args.append(String(d))
        case .int(let i):    args.append(String(i))
        case .string(let s): args.append(s)
        case .bool(let b):   if !b { args.removeLast() }
        }
    }

    func makeShutdownPlan(config: ServerInstanceConfig) -> ShutdownPlan {
        ShutdownPlan(gracefulRequest: .signal(SIGINT), gracePeriod: 5)
    }

    // MARK: Runtime

    func healthCheck(config: ServerInstanceConfig) async -> HealthStatus {
        let url = URL(string: "http://\(config.host):\(config.port)/health")!
        do {
            let session = URLSession(configuration: .ephemeral)
            let (_, response) = try await session.data(from: url)
            if (response as? HTTPURLResponse)?.statusCode == 200 {
                return .healthy
            }
            // Fallback: /v1/models
            let modelsURL = URL(string: "http://\(config.host):\(config.port)/v1/models")!
            let (_, r2) = try await session.data(from: modelsURL)
            return (r2 as? HTTPURLResponse)?.statusCode == 200 ? .healthy : .unhealthy(reason: "Unexpected status")
        } catch {
            return .unreachable
        }
    }

    func listModels(config: ServerInstanceConfig) async throws -> [ModelRef] {
        var searchPaths = config.modelSearchPaths
        // Include default HF cache if not already listed.
        let hfDefault = (ProcessInfo.processInfo.environment["HF_HOME"] ?? "\(NSHomeDirectory())/.cache/huggingface/hub")
        if !searchPaths.contains(hfDefault) { searchPaths.append(hfDefault) }

        var models: [ModelRef] = []
        var seenKeys = Set<String>()
        let fm = FileManager.default
        for root in searchPaths {
            // Resolve the canonical snapshot for each HF model dir up front,
            // so we include only one entry per model even with multiple snapshots.
            let preferredSnapshots = MLXLMDriver.preferredHFSnapshotIDs(root: root, fm: fm)
            guard let enumerator = fm.enumerator(atPath: root) else { continue }
            while let path = enumerator.nextObject() as? String {
                // For HF snapshot dirs (models--org--repo/snapshots/<hash>),
                // skip any snapshot that isn't the canonical one for this model.
                let parts = path.split(separator: "/", maxSplits: 3, omittingEmptySubsequences: false)
                    .map(String.init)
                if parts.count >= 3,
                   parts[0].hasPrefix("models--"),
                   parts[1] == "snapshots",
                   !preferredSnapshots.contains(parts[0] + "/" + parts[2]) {
                    continue
                }
                if let ref = MLXLMDriver.modelRef(forRelativePath: path, root: root, fm: fm) {
                    guard !seenKeys.contains(ref.key) else { continue }
                    seenKeys.insert(ref.key)
                    models.append(ref)
                }
            }
        }
        return models
    }

    /// For each `models--org--repo` directory under `root`, resolves the preferred
    /// snapshot hash from `refs/main` (a text file containing the commit hash).
    /// Falls back to the lexicographically first hash when `refs/main` is absent.
    /// Returns a set of `"models--org--repo/<hash>"` strings.
    static func preferredHFSnapshotIDs(root: String, fm: FileManager) -> Set<String> {
        var preferred = Set<String>()
        guard let entries = try? fm.contentsOfDirectory(atPath: root) else { return preferred }
        for entry in entries where entry.hasPrefix("models--") {
            let modelDir = (root as NSString).appendingPathComponent(entry)
            let refsMain = (modelDir as NSString).appendingPathComponent("refs/main")
            if let hash = (try? String(contentsOfFile: refsMain, encoding: .utf8))?
                .trimmingCharacters(in: .whitespacesAndNewlines), !hash.isEmpty {
                preferred.insert(entry + "/" + hash)
            } else {
                // No refs/main — include all snapshots (fallback for manual/partial caches).
                let snapshotsDir = (modelDir as NSString).appendingPathComponent("snapshots")
                if let hashes = try? fm.contentsOfDirectory(atPath: snapshotsDir) {
                    for hash in hashes { preferred.insert(entry + "/" + hash) }
                }
            }
        }
        return preferred
    }

    /// Build a ModelRef for one filesystem path found during enumeration.
    /// Returns nil if the path is not a directory or lacks config.json.
    static func modelRef(forRelativePath path: String, root: String, fm: FileManager) -> ModelRef? {
        let fullPath = (root as NSString).appendingPathComponent(path)
        var isDir: ObjCBool = false
        fm.fileExists(atPath: fullPath, isDirectory: &isDir)
        guard isDir.boolValue else { return nil }
        guard fm.fileExists(atPath: (fullPath as NSString).appendingPathComponent("config.json")) else { return nil }

        // Derive the model key and display name based on path structure.
        let key: String
        let displayName: String
        let parts = path.split(separator: "/", maxSplits: 3, omittingEmptySubsequences: false)
            .map(String.init)
        if parts.count >= 3, parts[0].hasPrefix("models--"), parts[1] == "snapshots" {
            // HF cache: models--org--repo/snapshots/<hash> — key from top-level dir.
            let decoded = parts[0]
                .replacingOccurrences(of: "models--", with: "")
                .replacingOccurrences(of: "--", with: "/")
            key = decoded
            displayName = (decoded as NSString).lastPathComponent
        } else if path.hasPrefix("models--"), !path.contains("/") {
            // Flat HF layout: models--org--repo at root (no snapshots subdir).
            key = path
                .replacingOccurrences(of: "models--", with: "")
                .replacingOccurrences(of: "--", with: "/")
            displayName = (fullPath as NSString).lastPathComponent
        } else {
            key = (fullPath as NSString).lastPathComponent
            displayName = key
        }

        var metadata = ModelMetadataParser.parse(directoryPath: fullPath, modelKey: key)
        // mlx-lm only serves MLX-compatible models. Safetensors is
        // the weight container MLX adopted — don't let it shadow the
        // real format. Only keep GGUF if .gguf files are present.
        if metadata.modelFormat != .gguf { metadata.modelFormat = .mlx }
        return ModelRef(
            key: key,
            displayName: displayName,
            sizeBytes: nil,
            location: .filesystem(path: fullPath),
            metadata: metadata
        )
    }

    func switchModel(to model: ModelRef, params: ParamValues, config: ServerInstanceConfig) async throws {
        // mlx-lm is .restartRequired — switchModel should never be called on this driver.
        assertionFailure("switchModel called on MLXLMDriver, which requires a restart. The controller should handle this via the restart path.")
        throw DriverError.switchNotSupported(serverType: .mlxLM)
    }

    func contextUsage(config: ServerInstanceConfig) async throws -> ContextUsage? {
        // mlx-lm does not currently expose context fill for external requests.
        // Returning nil causes the UI to show "n/a". To be revisited post-MVP.
        return nil
    }
}

// MARK: - Model metadata parser

/// Best-effort metadata extraction for filesystem models.
/// Pure functions; never throws, never crashes on malformed input.
enum ModelMetadataParser {

    /// Parse metadata for a model directory. `modelKey` is the HF-style
    /// repo path or directory name (used for name-based heuristics).
    static func parse(directoryPath: String, modelKey: String) -> ModelMetadata {
        let nameSource = modelKey + " " + (directoryPath as NSString).lastPathComponent
        let fileNames = shallowFileNames(in: directoryPath)
        let configJSON = readJSON(at: (directoryPath as NSString).appendingPathComponent("config.json"))

        var metadata = ModelMetadata()
        metadata.parameterCount = parseParameterCount(from: nameSource)
        metadata.quantization = parseQuantization(from: nameSource)
        metadata.modelFormat = detectFormat(modelKey: modelKey, fileNames: fileNames)
        metadata.capabilities = detectCapabilities(directoryPath: directoryPath, modelKey: modelKey, configJSON: configJSON)
        parseArchitectureFields(from: configJSON, into: &metadata)
        return metadata
    }

    /// Extract KV-cache-relevant architecture fields from config.json.
    static func parseArchitectureFields(from config: [String: Any]?, into metadata: inout ModelMetadata) {
        guard let config else { return }

        // num_hidden_layers (standard HF key).
        if let layers = config["num_hidden_layers"] as? Int {
            metadata.numHiddenLayers = layers
        }

        // KV head count: GQA models use num_key_value_heads; others equal num_attention_heads.
        let kvHeads = (config["num_key_value_heads"] as? Int)
            ?? (config["num_attention_heads"] as? Int)
        metadata.numKVHeads = kvHeads

        // head_dim: explicit in some configs; derive from hidden_size / num_attention_heads otherwise.
        if let dim = config["head_dim"] as? Int {
            metadata.headDim = dim
        } else if let hiddenSize = config["hidden_size"] as? Int,
                  let numHeads = config["num_attention_heads"] as? Int,
                  numHeads > 0 {
            metadata.headDim = hiddenSize / numHeads
        }
    }

    // MARK: Name-based heuristics

    /// Matches tokens like "7B", "27b", "1.5B", "0.5b" in a path string.
    static func parseParameterCount(from text: String) -> String? {
        for token in tokens(in: text) {
            let lower = token.lowercased()
            guard lower.hasSuffix("b"), lower.count >= 2 else { continue }
            let digits = lower.dropLast()
            guard !digits.isEmpty,
                  digits.allSatisfy({ $0.isNumber || $0 == "." }),
                  digits.contains(where: \.isNumber),
                  Double(digits) != nil else { continue }
            return digits.uppercased() + "B"
        }
        return nil
    }

    /// Matches "4bit", "8bit", "4-bit", "8-bit", GGUF-style "Q4_K_M"/"Q8_0",
    /// and float dtypes "f16"/"bf16"/"fp16" (case-insensitive).
    static func parseQuantization(from text: String) -> String? {
        let lower = text.lowercased()

        // GGUF-style: q4_k_m, q8_0, q4_0, q5_k_s, …
        // (extended-delimiter literal: works without -enable-bare-slash-regex)
        if let match = lower.firstMatch(of: #/q\d(?:_[a-z0-9]+)+/#) {
            return String(match.output).uppercased()
        }

        // N-bit variants (both "4bit" and "4-bit").
        for bits in ["4", "8", "2", "3", "5", "6"] {
            if lower.contains("\(bits)bit") || lower.contains("\(bits)-bit") {
                return "\(bits)bit"
            }
        }

        // Float dtypes — check longer tokens first so "bf16" doesn't match as "f16".
        for dtype in ["bf16", "fp16", "f16"] {
            if tokens(in: lower).contains(dtype) { return dtype }
        }
        return nil
    }

    // MARK: Format detection

    static func detectFormat(modelKey: String, fileNames: [String]) -> ModelMetadata.ModelFormat {
        let lowerFiles = fileNames.map { $0.lowercased() }
        if lowerFiles.contains(where: { $0.hasSuffix(".gguf") }) {
            return .gguf
        }
        // mlx-community repos and .npz weights are MLX regardless of container.
        if modelKey.lowercased().hasPrefix("mlx-community/")
            || lowerFiles.contains(where: { $0.hasSuffix(".npz") }) {
            return .mlx
        }
        if lowerFiles.contains(where: { $0.hasSuffix(".safetensors") }) {
            return .safetensors
        }
        // Default for the mlx-lm driver.
        return .mlx
    }

    // MARK: Capability detection (best-effort, silent on error)

    static func detectCapabilities(directoryPath: String, modelKey: String, configJSON: [String: Any]? = nil) -> Set<ModelMetadata.Capability> {
        let config = configJSON ?? readJSON(at: (directoryPath as NSString).appendingPathComponent("config.json"))
        return capabilitiesFromConfig(config)
            .union(capabilitiesFromModelKey(modelKey.lowercased()))
            .union(capabilitiesFromTokenizerConfig(at: directoryPath))
    }

    private static func capabilitiesFromConfig(_ config: [String: Any]?) -> Set<ModelMetadata.Capability> {
        guard let config else { return [] }
        var caps: Set<ModelMetadata.Capability> = []
        if config["vision_config"] != nil { caps.insert(.vision) }
        // Keyword table: if model_type contains the keyword, grant the capability.
        // "bert" is caught by contains (bert, bert-base, roberta all contain "bert").
        let modelType = (config["model_type"] as? String)?.lowercased() ?? ""
        let typeKeywords: [(String, ModelMetadata.Capability)] = [
            ("code", .code), ("embed", .embedding), ("bert", .embedding)
        ]
        for (keyword, cap) in typeKeywords where modelType.contains(keyword) { caps.insert(cap) }
        return caps
    }

    private static func capabilitiesFromModelKey(_ lowerKey: String) -> Set<ModelMetadata.Capability> {
        // "coder" contains "code" so a single substring check is sufficient.
        var caps: Set<ModelMetadata.Capability> = []
        if lowerKey.contains("code")  { caps.insert(.code) }
        if lowerKey.contains("embed") { caps.insert(.embedding) }
        return caps
    }

    private static func capabilitiesFromTokenizerConfig(at directoryPath: String) -> Set<ModelMetadata.Capability> {
        let path = (directoryPath as NSString).appendingPathComponent("tokenizer_config.json")
        guard let config = readJSON(at: path),
              let template = config["chat_template"] as? String,
              template.lowercased().contains("tool") else { return [] }
        return [.toolUse]
    }

    // MARK: Helpers

    private static func tokens(in text: String) -> [String] {
        text.split(whereSeparator: { !$0.isLetter && !$0.isNumber && $0 != "." })
            .map(String.init)
    }

    private static func shallowFileNames(in directoryPath: String) -> [String] {
        (try? FileManager.default.contentsOfDirectory(atPath: directoryPath)) ?? []
    }

    private static func readJSON(at path: String) -> [String: Any]? {
        guard let data = FileManager.default.contents(atPath: path),
              data.count < 10_000_000, // sanity cap
              let object = try? JSONSerialization.jsonObject(with: data) else { return nil }
        return object as? [String: Any]
    }
}

// MARK: - Driver errors

enum DriverError: Error, Sendable {
    case modelRequired(serverType: ServerType)
    case unexpectedModelLocation(model: ModelRef)
    case switchNotSupported(serverType: ServerType)
}
