import XCTest
@testable import LocalBar

final class MLXLMDriverTests: XCTestCase {

    private let driver = MLXLMDriver()

    private func makeConfig(
        executablePath: String = "/usr/local/bin/uvx",
        port: Int = 8080,
        selectedModelKey: String? = "/models/llama3"
    ) -> ServerInstanceConfig {
        var c = ServerInstanceConfig(name: "Test", type: .mlxLM, port: port, executablePath: executablePath)
        c.selectedModelKey = selectedModelKey
        return c
    }

    private func fsModel(path: String, key: String? = nil) -> ModelRef {
        ModelRef(key: key ?? path, displayName: (path as NSString).lastPathComponent,
                 sizeBytes: nil, location: .filesystem(path: path))
    }

    // MARK: - makeLaunchPlan — invocation mode

    func test_makeLaunchPlan_uvxPrefix() throws {
        let config = makeConfig(executablePath: "/usr/local/bin/uvx")
        let model = fsModel(path: "/models/llama3")
        let plan = try driver.makeLaunchPlan(config: config, model: model, params: ParamValues())
        XCTAssertEqual(plan.arguments.prefix(3), ["--from", "mlx-lm", "mlx_lm.server"])
    }

    func test_makeLaunchPlan_pythonPrefix() throws {
        let config = makeConfig(executablePath: "/usr/bin/python3")
        let model = fsModel(path: "/models/llama3")
        let plan = try driver.makeLaunchPlan(config: config, model: model, params: ParamValues())
        XCTAssertEqual(plan.arguments.prefix(2), ["-m", "mlx_lm.server"])
    }

    func test_makeLaunchPlan_modelPath() throws {
        let config = makeConfig()
        let model = fsModel(path: "/models/llama3")
        let plan = try driver.makeLaunchPlan(config: config, model: model, params: ParamValues())
        let modelIdx = plan.arguments.firstIndex(of: "--model")!
        XCTAssertEqual(plan.arguments[modelIdx + 1], "/models/llama3")
    }

    func test_makeLaunchPlan_hostAndPort() throws {
        let config = makeConfig(port: 9999)
        let model = fsModel(path: "/models/llama3")
        let plan = try driver.makeLaunchPlan(config: config, model: model, params: ParamValues())
        let hostIdx = plan.arguments.firstIndex(of: "--host")!
        let portIdx = plan.arguments.firstIndex(of: "--port")!
        XCTAssertEqual(plan.arguments[hostIdx + 1], "127.0.0.1")
        XCTAssertEqual(plan.arguments[portIdx + 1], "9999")
    }

    // MARK: - makeLaunchPlan — canonical param mapping

    func test_makeLaunchPlan_temperature() throws {
        var params = ParamValues(); params.values[.temperature] = .double(0.5)
        let plan = try driver.makeLaunchPlan(config: makeConfig(), model: fsModel(path: "/m"), params: params)
        let idx = plan.arguments.firstIndex(of: "--temp")!
        XCTAssertEqual(plan.arguments[idx + 1], "0.5")
    }

    func test_makeLaunchPlan_contextLength() throws {
        var params = ParamValues(); params.values[.contextLength] = .int(4096)
        let plan = try driver.makeLaunchPlan(config: makeConfig(), model: fsModel(path: "/m"), params: params)
        let idx = plan.arguments.firstIndex(of: "--max-kv-size")!
        XCTAssertEqual(plan.arguments[idx + 1], "4096")
    }

    func test_makeLaunchPlan_maxTokens() throws {
        var params = ParamValues(); params.values[.maxTokens] = .int(512)
        let plan = try driver.makeLaunchPlan(config: makeConfig(), model: fsModel(path: "/m"), params: params)
        XCTAssertTrue(plan.arguments.contains("--max-tokens"))
    }

    func test_makeLaunchPlan_topP() throws {
        var params = ParamValues(); params.values[.topP] = .double(0.9)
        let plan = try driver.makeLaunchPlan(config: makeConfig(), model: fsModel(path: "/m"), params: params)
        XCTAssertTrue(plan.arguments.contains("--top-p"))
    }

    func test_makeLaunchPlan_seed() throws {
        var params = ParamValues(); params.values[.seed] = .int(42)
        let plan = try driver.makeLaunchPlan(config: makeConfig(), model: fsModel(path: "/m"), params: params)
        let idx = plan.arguments.firstIndex(of: "--seed")!
        XCTAssertEqual(plan.arguments[idx + 1], "42")
    }

    // MARK: - makeLaunchPlan — advanced flags

    func test_makeLaunchPlan_advancedFlag_nonLocalOnly_appended() throws {
        var config = makeConfig()
        config.advancedFlags["--kv-cache-bits"] = .int(8)
        let plan = try driver.makeLaunchPlan(config: config, model: fsModel(path: "/m"), params: ParamValues())
        let idx = plan.arguments.firstIndex(of: "--kv-cache-bits")!
        XCTAssertEqual(plan.arguments[idx + 1], "8")
    }

    func test_makeLaunchPlan_localOnlyFlag_omitted() throws {
        var config = makeConfig()
        config.advancedFlags["startupTimeoutSeconds"] = .int(300)
        let plan = try driver.makeLaunchPlan(config: config, model: fsModel(path: "/m"), params: ParamValues())
        XCTAssertFalse(plan.arguments.contains("startupTimeoutSeconds"))
    }

    func test_makeLaunchPlan_boolFlagFalse_omitted() throws {
        var config = makeConfig()
        config.advancedFlags["--trust-remote-code"] = .bool(false)
        let plan = try driver.makeLaunchPlan(config: config, model: fsModel(path: "/m"), params: ParamValues())
        XCTAssertFalse(plan.arguments.contains("--trust-remote-code"))
    }

    func test_makeLaunchPlan_boolFlagTrue_appended() throws {
        var config = makeConfig()
        config.advancedFlags["--trust-remote-code"] = .bool(true)
        let plan = try driver.makeLaunchPlan(config: config, model: fsModel(path: "/m"), params: ParamValues())
        XCTAssertTrue(plan.arguments.contains("--trust-remote-code"))
    }

    // MARK: - makeLaunchPlan — errors

    func test_makeLaunchPlan_noModel_throws() {
        XCTAssertThrowsError(
            try driver.makeLaunchPlan(config: makeConfig(), model: nil, params: ParamValues())
        ) { error in
            if case DriverError.modelRequired(let st) = error {
                XCTAssertEqual(st, .mlxLM)
            } else {
                XCTFail("Expected DriverError.modelRequired, got \(error)")
            }
        }
    }

    func test_makeLaunchPlan_serverManagedLocation_throws() {
        let model = ModelRef(key: "k", displayName: "m", sizeBytes: nil, location: .serverManaged)
        XCTAssertThrowsError(
            try driver.makeLaunchPlan(config: makeConfig(), model: model, params: ParamValues())
        ) { error in
            guard case DriverError.unexpectedModelLocation = error else {
                XCTFail("Expected DriverError.unexpectedModelLocation"); return
            }
        }
    }

    // MARK: - validate

    func test_validate_missingExecutable_blocker() async {
        let config = makeConfig(executablePath: "/nonexistent/python3", selectedModelKey: "/models/m")
        let issues = await driver.validate(config: config)
        XCTAssertTrue(issues.contains(where: { $0.severity == .blocker && $0.message.contains("not found") }))
    }

    func test_validate_noModelKey_blocker() async {
        let config = makeConfig(executablePath: "/bin/sh", selectedModelKey: nil)
        let issues = await driver.validate(config: config)
        XCTAssertTrue(issues.contains(where: { $0.severity == .blocker && $0.message.contains("No model") }))
    }

    func test_validate_portBelow1024_warning() async {
        let config = makeConfig(executablePath: "/bin/sh", port: 443, selectedModelKey: "/m")
        let issues = await driver.validate(config: config)
        XCTAssertTrue(issues.contains(where: { $0.severity == .warning }))
    }

    func test_validate_validConfig_noIssues() async {
        let config = makeConfig(executablePath: "/bin/sh", port: 8080, selectedModelKey: "/m")
        let issues = await driver.validate(config: config)
        XCTAssertTrue(issues.isEmpty)
    }

    func test_validate_multipleBlockers_bothReported() async {
        let config = makeConfig(executablePath: "/nonexistent/bin", port: 8080, selectedModelKey: nil)
        let issues = await driver.validate(config: config)
        let blockers = issues.filter { $0.severity == .blocker }
        XCTAssertEqual(blockers.count, 2) // missing executable + no model key
    }

    // MARK: - static properties

    func test_serverType() {
        XCTAssertEqual(driver.serverType, .mlxLM)
    }

    func test_modelSwitchBehavior_restartRequired() {
        XCTAssertEqual(driver.modelSwitchBehavior, .restartRequired)
    }

    func test_modelListRequiresRunningServer_false() {
        XCTAssertFalse(driver.modelListRequiresRunningServer)
    }

    // MARK: - modelRef(forRelativePath:root:fm:)

    func test_modelRef_nonDirectory_returnsNil() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        try FileManager.default.createDirectory(atPath: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: root) }
        // Create a plain file (not a directory).
        let filePath = (root as NSString).appendingPathComponent("weights.safetensors")
        FileManager.default.createFile(atPath: filePath, contents: nil)
        XCTAssertNil(MLXLMDriver.modelRef(forRelativePath: "weights.safetensors", root: root, fm: .default))
    }

    func test_modelRef_directoryWithoutConfigJson_returnsNil() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        let modelDir = (root as NSString).appendingPathComponent("my-model")
        try FileManager.default.createDirectory(atPath: modelDir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: root) }
        XCTAssertNil(MLXLMDriver.modelRef(forRelativePath: "my-model", root: root, fm: .default))
    }

    func test_modelRef_validModelDir_returnsRef() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        let modelDir = (root as NSString).appendingPathComponent("llama-3-8b")
        try FileManager.default.createDirectory(atPath: modelDir, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: (modelDir as NSString).appendingPathComponent("config.json"), contents: "{}".data(using: .utf8))
        defer { try? FileManager.default.removeItem(atPath: root) }
        let ref = try XCTUnwrap(MLXLMDriver.modelRef(forRelativePath: "llama-3-8b", root: root, fm: .default))
        XCTAssertEqual(ref.key, "llama-3-8b")
        XCTAssertEqual(ref.displayName, "llama-3-8b")
        if case .filesystem(let path) = ref.location {
            XCTAssertEqual(path, modelDir)
        } else {
            XCTFail("Expected filesystem location")
        }
        XCTAssertEqual(ref.metadata?.modelFormat, .mlx)
    }

    func test_modelRef_hfCacheLayout_decodesKey() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        // HF cache flat layout: "models--org--repo" at root (no snapshots subdir).
        let hfDir = (root as NSString).appendingPathComponent("models--mlx-community--llama3-8b")
        try FileManager.default.createDirectory(atPath: hfDir, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: (hfDir as NSString).appendingPathComponent("config.json"), contents: "{}".data(using: .utf8))
        defer { try? FileManager.default.removeItem(atPath: root) }
        let ref = try XCTUnwrap(MLXLMDriver.modelRef(forRelativePath: "models--mlx-community--llama3-8b", root: root, fm: .default))
        XCTAssertEqual(ref.key, "mlx-community/llama3-8b")
    }

    func test_modelRef_hfSnapshotPath_decodesKeyFromGrandparent() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        // Real HF cache: models--org--repo/snapshots/<hash>/config.json
        let snapshotDir = ((root as NSString)
            .appendingPathComponent("models--mlx-community--llama3-8b")
            as NSString).appendingPathComponent("snapshots/abc123def456")
        try FileManager.default.createDirectory(atPath: snapshotDir, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: (snapshotDir as NSString).appendingPathComponent("config.json"), contents: "{}".data(using: .utf8))
        defer { try? FileManager.default.removeItem(atPath: root) }
        let ref = try XCTUnwrap(MLXLMDriver.modelRef(
            forRelativePath: "models--mlx-community--llama3-8b/snapshots/abc123def456",
            root: root, fm: .default))
        XCTAssertEqual(ref.key, "mlx-community/llama3-8b")
        XCTAssertEqual(ref.displayName, "llama3-8b")
    }

    // MARK: - preferredHFSnapshotIDs

    func test_preferredHFSnapshotIDs_refsMain_returnsCanonicalHash() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        let modelDir = (root as NSString).appendingPathComponent("models--mlx-community--llama3-8b")
        let refsDir = (modelDir as NSString).appendingPathComponent("refs")
        try FileManager.default.createDirectory(atPath: refsDir, withIntermediateDirectories: true)
        let canonicalHash = "abc123def456abc123def456abc123def456abc1"
        FileManager.default.createFile(atPath: (refsDir as NSString).appendingPathComponent("main"),
                                       contents: (canonicalHash + "\n").data(using: .utf8))
        defer { try? FileManager.default.removeItem(atPath: root) }
        let preferred = MLXLMDriver.preferredHFSnapshotIDs(root: root, fm: .default)
        XCTAssertTrue(preferred.contains("models--mlx-community--llama3-8b/" + canonicalHash))
        XCTAssertEqual(preferred.count, 1)
    }

    func test_preferredHFSnapshotIDs_noRefsMain_includesAllSnapshots() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        let modelDir = (root as NSString).appendingPathComponent("models--mlx-community--llama3-8b")
        let snapshotsDir = (modelDir as NSString).appendingPathComponent("snapshots")
        let hash1 = "aaa111"
        let hash2 = "bbb222"
        try FileManager.default.createDirectory(atPath: (snapshotsDir as NSString).appendingPathComponent(hash1),
                                                withIntermediateDirectories: true)
        try FileManager.default.createDirectory(atPath: (snapshotsDir as NSString).appendingPathComponent(hash2),
                                                withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: root) }
        let preferred = MLXLMDriver.preferredHFSnapshotIDs(root: root, fm: .default)
        XCTAssertTrue(preferred.contains("models--mlx-community--llama3-8b/" + hash1))
        XCTAssertTrue(preferred.contains("models--mlx-community--llama3-8b/" + hash2))
    }

    func test_preferredHFSnapshotIDs_emptyRoot_returnsEmpty() {
        let preferred = MLXLMDriver.preferredHFSnapshotIDs(root: "/nonexistent/\(UUID().uuidString)", fm: .default)
        XCTAssertTrue(preferred.isEmpty)
    }

    func test_modelRef_ggufFile_keepsGgufFormat() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        let modelDir = (root as NSString).appendingPathComponent("gguf-model")
        try FileManager.default.createDirectory(atPath: modelDir, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: (modelDir as NSString).appendingPathComponent("config.json"), contents: "{}".data(using: .utf8))
        FileManager.default.createFile(atPath: (modelDir as NSString).appendingPathComponent("model.gguf"), contents: nil)
        defer { try? FileManager.default.removeItem(atPath: root) }
        let ref = try XCTUnwrap(MLXLMDriver.modelRef(forRelativePath: "gguf-model", root: root, fm: .default))
        XCTAssertEqual(ref.metadata?.modelFormat, .gguf)
    }

    // MARK: - listModels

    func test_listModels_findsModelInSearchPath() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        let modelDir = (root as NSString).appendingPathComponent("my-model")
        try FileManager.default.createDirectory(atPath: modelDir, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: (modelDir as NSString).appendingPathComponent("config.json"), contents: "{}".data(using: .utf8))
        defer { try? FileManager.default.removeItem(atPath: root) }

        var config = makeConfig()
        config.modelSearchPaths = [root]
        let models = try await driver.listModels(config: config)
        XCTAssertTrue(models.contains(where: { $0.key == "my-model" }))
    }

    func test_listModels_skipsNonModelDirs() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        // A dir without config.json should not appear in results.
        let emptyDir = (root as NSString).appendingPathComponent("not-a-model")
        try FileManager.default.createDirectory(atPath: emptyDir, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(atPath: root) }

        var config = makeConfig()
        config.modelSearchPaths = [root]
        let models = try await driver.listModels(config: config)
        XCTAssertTrue(models.isEmpty)
    }

    func test_listModels_emptyWhenSearchPathAbsent() async throws {
        var config = makeConfig()
        config.modelSearchPaths = ["/nonexistent/path/\(UUID().uuidString)"]
        let models = try await driver.listModels(config: config)
        // Should silently return empty — no crash.
        XCTAssertTrue(models.isEmpty)
    }

    func test_listModels_hfSnapshot_deduplicatesMultipleSnapshots() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        let canonicalHash = "abc123def456abc123def456abc123def456abc1"
        let staleHash    = "stale000stale000stale000stale000stale000"
        // Write refs/main pointing at the canonical hash.
        let modelDir = (root as NSString).appendingPathComponent("models--mlx-community--llama3-8b")
        let refsDir = (modelDir as NSString).appendingPathComponent("refs")
        try FileManager.default.createDirectory(atPath: refsDir, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: (refsDir as NSString).appendingPathComponent("main"),
                                       contents: canonicalHash.data(using: .utf8))
        // Create two snapshot dirs, each with config.json.
        for hash in [canonicalHash, staleHash] {
            let dir = (((modelDir as NSString).appendingPathComponent("snapshots")) as NSString)
                .appendingPathComponent(hash)
            try FileManager.default.createDirectory(atPath: dir, withIntermediateDirectories: true)
            FileManager.default.createFile(atPath: (dir as NSString).appendingPathComponent("config.json"),
                                           contents: "{}".data(using: .utf8))
        }
        defer { try? FileManager.default.removeItem(atPath: root) }

        var config = makeConfig()
        config.modelSearchPaths = [root]
        let models = try await driver.listModels(config: config)
        let matching = models.filter { $0.key == "mlx-community/llama3-8b" }
        XCTAssertEqual(matching.count, 1, "expected one entry per model, got \(matching.count)")
    }

    func test_listModels_hfSnapshot_usesCanonicalSnapshotPath() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString).path
        let canonicalHash = "canonical000canonical000canonical000cano"
        let modelDir = (root as NSString).appendingPathComponent("models--mlx-community--phi3")
        let refsDir = (modelDir as NSString).appendingPathComponent("refs")
        try FileManager.default.createDirectory(atPath: refsDir, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: (refsDir as NSString).appendingPathComponent("main"),
                                       contents: canonicalHash.data(using: .utf8))
        let snapshotDir = (((modelDir as NSString).appendingPathComponent("snapshots")) as NSString)
            .appendingPathComponent(canonicalHash)
        try FileManager.default.createDirectory(atPath: snapshotDir, withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: (snapshotDir as NSString).appendingPathComponent("config.json"),
                                       contents: "{}".data(using: .utf8))
        defer { try? FileManager.default.removeItem(atPath: root) }

        var config = makeConfig()
        config.modelSearchPaths = [root]
        let models = try await driver.listModels(config: config)
        let ref = try XCTUnwrap(models.first(where: { $0.key == "mlx-community/phi3" }))
        if case .filesystem(let path) = ref.location {
            XCTAssertTrue(path.hasSuffix(canonicalHash), "expected canonical snapshot path, got \(path)")
        } else {
            XCTFail("Expected filesystem location")
        }
    }
}
