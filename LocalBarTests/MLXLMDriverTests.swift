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
}
