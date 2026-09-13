import Foundation

/// Driver for Ollama.
///
/// Launch: ollama serve (with OLLAMA_HOST env var)
/// Health: GET / returns "Ollama is running"
/// Models: GET /api/tags (requires running server)
/// Switch: .apiCall → warm-load via POST /api/generate with empty prompt
/// Params: mostly .serverSideDefault (Modelfile) — D1: advisory badge in UI
/// Shutdown: SIGTERM, 10 s grace
struct OllamaDriver: ServerDriver {

    var serverType: ServerType { .ollama }
    var modelSwitchBehavior: ModelSwitchBehavior { .apiCall }
    var modelListRequiresRunningServer: Bool { true }

    // MARK: Param schema

    /// D1: Ollama applies most params per-request, so external tools can
    /// override them. These are marked .serverSideDefault — the UI shows
    /// an advisory badge ("External tools may override this per request").
    var paramSchema: [ParamDescriptor] {
        [
            ParamDescriptor(param: .contextLength,   serverFlagName: "options.num_ctx",           application: .serverSideDefault, valueType: .int(range: 512...128_000),  defaultValue: .int(2048),    note: "Context window size — set via Modelfile PARAMETER"),
            ParamDescriptor(param: .temperature,     serverFlagName: "options.temperature",        application: .serverSideDefault, valueType: .double(range: 0.0...2.0),   defaultValue: .double(0.8),  note: "Set via Modelfile DEFAULT parameter"),
            ParamDescriptor(param: .maxTokens,       serverFlagName: "options.num_predict",        application: .serverSideDefault, valueType: .int(range: -1...128_000),   defaultValue: .int(-1),      note: "Limits response length (-1 = unlimited)"),
            ParamDescriptor(param: .topK,            serverFlagName: "options.top_k",              application: .serverSideDefault, valueType: .int(range: 0...200),         defaultValue: .int(40),      note: "Set via Modelfile DEFAULT parameter"),
            ParamDescriptor(param: .repeatPenalty,   serverFlagName: "options.repeat_penalty",     application: .serverSideDefault, valueType: .double(range: 0.5...2.0),   defaultValue: .double(1.1),  note: "Set via Modelfile DEFAULT parameter"),
            ParamDescriptor(param: .presencePenalty, serverFlagName: "options.presence_penalty",   application: .serverSideDefault, valueType: .double(range: 0.0...1.0),   defaultValue: nil,           note: "Set via Modelfile DEFAULT parameter"),
            ParamDescriptor(param: .topP,            serverFlagName: "options.top_p",              application: .serverSideDefault, valueType: .double(range: 0.0...1.0),   defaultValue: .double(0.9),  note: "Set via Modelfile DEFAULT parameter"),
            ParamDescriptor(param: .minP,            serverFlagName: "options.min_p",              application: .serverSideDefault, valueType: .double(range: 0.0...1.0),   defaultValue: nil,           note: "Set via Modelfile DEFAULT parameter"),
            ParamDescriptor(param: .seed,            serverFlagName: "options.seed",               application: .serverSideDefault, valueType: .int(range: 0...Int.max),     defaultValue: nil,           note: "Set via Modelfile DEFAULT parameter"),
        ]
    }

    var flagSchema: [FlagDescriptor] {
        [
            FlagDescriptor(flagName: "OLLAMA_KEEP_ALIVE",     displayName: "Keep Alive",         help: "How long models stay loaded in memory after last request. e.g. '5m', '1h', '-1' (forever).",    valueType: .string, isEnvironmentVariable: true,  defaultValue: .string("5m")),
            FlagDescriptor(flagName: "OLLAMA_MAX_LOADED_MODELS", displayName: "Max Loaded Models", help: "Maximum number of models loaded concurrently.",                                                 valueType: .int(range: 1...10), isEnvironmentVariable: true, defaultValue: nil),
            FlagDescriptor(flagName: "OLLAMA_NUM_PARALLEL",   displayName: "Parallel Requests",  help: "Maximum number of parallel requests processed.",                                                  valueType: .int(range: 1...32), isEnvironmentVariable: true, defaultValue: nil),
            FlagDescriptor(flagName: "startupTimeoutSeconds", displayName: "Startup Timeout (s)", help: "How long to wait for the server to become healthy before declaring an error. Default: 120 s.", valueType: .int(range: 30...600), isEnvironmentVariable: false, isLocalOnly: true, defaultValue: .int(120)),
        ]
    }

    // MARK: Validation

    func validate(config: ServerInstanceConfig) async -> [ConfigIssue] {
        var issues: [ConfigIssue] = []
        if !FileManager.default.fileExists(atPath: config.executablePath) {
            issues.append(ConfigIssue(
                severity: .blocker,
                message: "Ollama binary not found at '\(config.executablePath)'.",
                fixSuggestion: "Install Ollama from https://ollama.com or set the correct binary path."
            ))
        }
        if config.port < 1024 {
            issues.append(ConfigIssue(severity: .warning, message: "Port \(config.port) is below 1024 and may require root privileges."))
        }
        return issues
    }

    // MARK: Launch / shutdown

    func makeLaunchPlan(
        config: ServerInstanceConfig,
        model: ModelRef?,
        params: ParamValues
    ) throws -> LaunchPlan {
        var env: [String: String] = ["OLLAMA_HOST": "\(config.host):\(config.port)"]
        // Environment-variable flags from advanced config.
        for descriptor in flagSchema where descriptor.isEnvironmentVariable {
            if let value = config.advancedFlags[descriptor.flagName] {
                env[descriptor.flagName] = value.cliString
            }
        }
        return LaunchPlan(
            executableURL: URL(fileURLWithPath: config.executablePath),
            arguments: ["serve"],
            environment: env,
            workingDirectory: nil
        )
    }

    func makeShutdownPlan(config: ServerInstanceConfig) -> ShutdownPlan {
        // Ollama may be mid-unload on SIGTERM; give it 10 s.
        ShutdownPlan(gracefulRequest: nil, gracePeriod: 10)
    }

    // MARK: Runtime

    func healthCheck(config: ServerInstanceConfig) async -> HealthStatus {
        let url = URL(string: "http://\(config.host):\(config.port)/")!
        do {
            let session = URLSession(configuration: .ephemeral)
            let (data, response) = try await session.data(from: url)
            guard (response as? HTTPURLResponse)?.statusCode == 200 else {
                return .unhealthy(reason: "Unexpected HTTP status")
            }
            let body = String(data: data, encoding: .utf8) ?? ""
            return body.contains("Ollama is running") ? .healthy : .unhealthy(reason: "Unexpected response body")
        } catch {
            return .unreachable
        }
    }

    func listModels(config: ServerInstanceConfig) async throws -> [ModelRef] {
        let url = URL(string: "http://\(config.host):\(config.port)/api/tags")!
        let (data, _) = try await URLSession(configuration: .ephemeral).data(from: url)

        struct TagsResponse: Decodable {
            struct Model: Decodable {
                let name: String
                let size: Int64?
            }
            let models: [Model]
        }

        let decoded = try JSONDecoder().decode(TagsResponse.self, from: data)
        return decoded.models.map { m in
            ModelRef(key: m.name, displayName: m.name, sizeBytes: m.size, location: .serverManaged)
        }
    }

    func switchModel(to model: ModelRef, params: ParamValues, config: ServerInstanceConfig) async throws {
        // Use the managed tag if available; fall back to the raw model key.
        let tag = config.managedModelTag ?? model.key

        // Warm-load by sending a no-op generation request.
        let url = URL(string: "http://\(config.host):\(config.port)/api/generate")!
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")

        let body: [String: Any] = [
            "model": tag,
            "prompt": "",
            "stream": false,
        ]
        request.httpBody = try JSONSerialization.data(withJSONObject: body)
        let (_, response) = try await URLSession(configuration: .ephemeral).data(for: request)
        guard (response as? HTTPURLResponse)?.statusCode == 200 else {
            throw OllamaDriverError.modelLoadFailed(model: tag)
        }
    }

    func contextUsage(config: ServerInstanceConfig) async throws -> ContextUsage? {
        // GET /api/ps returns loaded models with their context window size,
        // but not current fill level. Returning nil until a better endpoint exists.
        return nil
    }

    // MARK: Model architecture info

    /// Fetch architecture metadata for a model via POST /api/show.
    /// Used for memory footprint estimation before starting.
    /// Returns nil if the server is unreachable or the model is unknown.
    func fetchModelInfo(modelKey: String, config: ServerInstanceConfig) async -> ModelMetadata? {
        guard let url = URL(string: "http://\(config.host):\(config.port)/api/show") else { return nil }
        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        guard let body = try? JSONSerialization.data(withJSONObject: ["name": modelKey]) else { return nil }
        request.httpBody = body

        guard let (data, response) = try? await URLSession(configuration: .ephemeral).data(for: request),
              (response as? HTTPURLResponse)?.statusCode == 200,
              let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let info = json["model_info"] as? [String: Any] else { return nil }

        // Ollama uses llama.cpp key names in model_info.
        var meta = ModelMetadata()
        meta.numHiddenLayers = info["llama.block_count"] as? Int
        meta.numKVHeads      = (info["llama.attention.head_count_kv"] as? Int)
                             ?? (info["llama.attention.head_count"] as? Int)
        // Derive head_dim from embedding_length / head_count.
        if let embed = info["llama.embedding_length"] as? Int,
           let heads = info["llama.attention.head_count"] as? Int,
           heads > 0 {
            meta.headDim = embed / heads
        }
        return meta
    }

    // MARK: Managed model lifecycle

    func managedModelTag(for modelKey: String, instanceId: String) -> String? {
        // Sanitize: replace characters Ollama tag parsers treat as delimiters.
        let sanitized = modelKey
            .replacingOccurrences(of: ":", with: "-")
            .replacingOccurrences(of: "/", with: "-")
        let shortId = String(instanceId.prefix(8)).lowercased()
        return "localbar/\(sanitized)-\(shortId)"
    }

    func createManagedModel(
        baseTag: String,
        instanceId: String,
        params: ParamValues,
        executablePath: String
    ) async -> String? {
        guard let tag = managedModelTag(for: baseTag, instanceId: instanceId) else { return nil }
        let content = generateModelfile(baseTag: baseTag, params: params)

        // Write Modelfile to a temp path.
        let tmpURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("LocalBar-\(UUID().uuidString).Modelfile")
        do {
            try content.write(to: tmpURL, atomically: true, encoding: .utf8)
        } catch {
            return nil
        }
        defer { try? FileManager.default.removeItem(at: tmpURL) }

        // Run: ollama create <tag> -f <path>
        let success = await runShell(executablePath, args: ["create", tag, "-f", tmpURL.path])
        return success ? tag : nil
    }

    func removeManagedModel(tag: String, executablePath: String) async {
        _ = await runShell(executablePath, args: ["rm", tag])
    }

    // MARK: Modelfile generation

    /// Build the Modelfile content for a managed model. In Milestone A this is
    /// just a FROM directive. Milestone B adds PARAMETER and SYSTEM lines from params.
    func generateModelfile(baseTag: String, params: ParamValues) -> String {
        var lines = ["FROM \(baseTag)"]

        // Sampling PARAMETER directives. Ollama has no bool params — skip them.
        let paramMap: [(CanonicalParam, String)] = [
            (.contextLength,   "num_ctx"),
            (.temperature,     "temperature"),
            (.maxTokens,       "num_predict"),
            (.topK,            "top_k"),
            (.repeatPenalty,   "repeat_penalty"),
            (.presencePenalty, "presence_penalty"),
            (.topP,            "top_p"),
            (.minP,            "min_p"),
            (.seed,            "seed"),
        ]
        for (param, ollamaName) in paramMap {
            guard let value = params[param] else { continue }
            if case .bool = value { continue }
            lines.append("PARAMETER \(ollamaName) \(value.displayString)")
        }

        // System prompt.
        if let system = params.systemPrompt, !system.isEmpty {
            let escaped = system.replacingOccurrences(of: "\"\"\"", with: "\\\"\\\"\\\"")
            lines.append("SYSTEM \"\"\"\n\(escaped)\n\"\"\"")
        }

        return lines.joined(separator: "\n")
    }

    // MARK: Shell helper

    @discardableResult
    private func runShell(_ executablePath: String, args: [String]) async -> Bool {
        await withCheckedContinuation { continuation in
            let proc = Process()
            proc.executableURL = URL(fileURLWithPath: executablePath)
            proc.arguments = args
            proc.standardOutput = FileHandle.nullDevice
            proc.standardError = FileHandle.nullDevice
            proc.terminationHandler = { p in
                continuation.resume(returning: p.terminationStatus == 0)
            }
            do {
                try proc.run()
            } catch {
                continuation.resume(returning: false)
            }
        }
    }
}

enum OllamaDriverError: Error, Sendable {
    case modelLoadFailed(model: String)
}
