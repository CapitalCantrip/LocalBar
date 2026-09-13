import XCTest
@testable import LocalBar

final class OllamaDriverTests: XCTestCase {

    private let driver = OllamaDriver()

    private func makeConfig(
        executablePath: String = "/usr/local/bin/ollama",
        port: Int = 11434,
        selectedModelKey: String? = nil,
        managedModelTag: String? = nil
    ) -> ServerInstanceConfig {
        var c = ServerInstanceConfig(name: "Test", type: .ollama, port: port, executablePath: executablePath)
        c.selectedModelKey = selectedModelKey
        c.managedModelTag = managedModelTag
        return c
    }

    // MARK: - managedModelTag

    func test_managedModelTag_basicFormat() {
        let tag = driver.managedModelTag(for: "llama3.1:8b", instanceId: "ABCDEF01-1234-5678-9ABC-DEF012345678")
        XCTAssertEqual(tag, "localbar/llama3.1-8b-abcdef01")
    }

    func test_managedModelTag_slashSanitized() {
        let tag = driver.managedModelTag(for: "org/model:latest", instanceId: "AABBCCDD-0000-0000-0000-000000000000")
        XCTAssertEqual(tag, "localbar/org-model-latest-aabbccdd")
    }

    func test_managedModelTag_colonSanitized() {
        let tag = driver.managedModelTag(for: "mistral:7b-instruct", instanceId: "12345678-0000-0000-0000-000000000000")
        XCTAssertTrue(tag!.contains("mistral-7b-instruct"))
    }

    func test_managedModelTag_usesFirst8CharsOfInstanceId() {
        let tag = driver.managedModelTag(for: "llama3:latest", instanceId: "CAFEBABE-DEAD-BEEF-0000-000000000000")
        XCTAssertTrue(tag!.hasSuffix("-cafebabe"))
    }

    // MARK: - generateModelfile

    func test_generateModelfile_fromLinePresent() {
        let result = driver.generateModelfile(baseTag: "llama3:8b", params: ParamValues())
        XCTAssertTrue(result.hasPrefix("FROM llama3:8b"))
    }

    func test_generateModelfile_noParams_fromOnly() {
        let result = driver.generateModelfile(baseTag: "llama3:8b", params: ParamValues())
        XCTAssertEqual(result, "FROM llama3:8b")
    }

    func test_generateModelfile_temperature() {
        var params = ParamValues()
        params.values[.temperature] = .double(0.7)
        let result = driver.generateModelfile(baseTag: "llama3:8b", params: params)
        XCTAssertTrue(result.contains("PARAMETER temperature 0.7"))
    }

    func test_generateModelfile_contextLength() {
        var params = ParamValues()
        params.values[.contextLength] = .int(4096)
        let result = driver.generateModelfile(baseTag: "base", params: params)
        XCTAssertTrue(result.contains("PARAMETER num_ctx 4096"))
    }

    func test_generateModelfile_maxTokens() {
        var params = ParamValues()
        params.values[.maxTokens] = .int(512)
        let result = driver.generateModelfile(baseTag: "base", params: params)
        XCTAssertTrue(result.contains("PARAMETER num_predict 512"))
    }

    func test_generateModelfile_topK() {
        var params = ParamValues()
        params.values[.topK] = .int(40)
        let result = driver.generateModelfile(baseTag: "base", params: params)
        XCTAssertTrue(result.contains("PARAMETER top_k 40"))
    }

    func test_generateModelfile_repeatPenalty() {
        var params = ParamValues()
        params.values[.repeatPenalty] = .double(1.1)
        let result = driver.generateModelfile(baseTag: "base", params: params)
        XCTAssertTrue(result.contains("PARAMETER repeat_penalty 1.1"))
    }

    func test_generateModelfile_presencePenalty() {
        var params = ParamValues()
        params.values[.presencePenalty] = .double(0.5)
        let result = driver.generateModelfile(baseTag: "base", params: params)
        XCTAssertTrue(result.contains("PARAMETER presence_penalty 0.5"))
    }

    func test_generateModelfile_topP() {
        var params = ParamValues()
        params.values[.topP] = .double(0.9)
        let result = driver.generateModelfile(baseTag: "base", params: params)
        XCTAssertTrue(result.contains("PARAMETER top_p 0.9"))
    }

    func test_generateModelfile_minP() {
        var params = ParamValues()
        params.values[.minP] = .double(0.05)
        let result = driver.generateModelfile(baseTag: "base", params: params)
        XCTAssertTrue(result.contains("PARAMETER min_p 0.05"))
    }

    func test_generateModelfile_seed() {
        var params = ParamValues()
        params.values[.seed] = .int(42)
        let result = driver.generateModelfile(baseTag: "base", params: params)
        XCTAssertTrue(result.contains("PARAMETER seed 42"))
    }

    func test_generateModelfile_systemPrompt() {
        var params = ParamValues()
        params.systemPrompt = "You are a helpful assistant."
        let result = driver.generateModelfile(baseTag: "base", params: params)
        XCTAssertTrue(result.contains("SYSTEM \"\"\""))
        XCTAssertTrue(result.contains("You are a helpful assistant."))
    }

    func test_generateModelfile_systemPromptEscapesTripleQuote() {
        var params = ParamValues()
        params.systemPrompt = "Say \"\"\" here."
        let result = driver.generateModelfile(baseTag: "base", params: params)
        XCTAssertFalse(result.contains("Say \"\"\" here."))  // raw triple-quote must be escaped
        XCTAssertTrue(result.contains("Say \\\"\\\"\\\""))
    }

    func test_generateModelfile_emptySystemPrompt_omitted() {
        var params = ParamValues()
        params.systemPrompt = ""
        let result = driver.generateModelfile(baseTag: "base", params: params)
        XCTAssertFalse(result.contains("SYSTEM"))
    }

    // MARK: - makeLaunchPlan

    func test_makeLaunchPlan_setsOllamaHost() throws {
        let config = makeConfig()
        let plan = try driver.makeLaunchPlan(config: config, model: nil, params: ParamValues())
        XCTAssertEqual(plan.environment["OLLAMA_HOST"], "127.0.0.1:11434")
    }

    func test_makeLaunchPlan_argumentsIsServe() throws {
        let plan = try driver.makeLaunchPlan(config: makeConfig(), model: nil, params: ParamValues())
        XCTAssertEqual(plan.arguments, ["serve"])
    }

    func test_makeLaunchPlan_executablePath() throws {
        let plan = try driver.makeLaunchPlan(config: makeConfig(), model: nil, params: ParamValues())
        XCTAssertEqual(plan.executableURL.path, "/usr/local/bin/ollama")
    }

    func test_makeLaunchPlan_envVarAdvancedFlag() throws {
        var config = makeConfig()
        config.advancedFlags["OLLAMA_KEEP_ALIVE"] = .string("1h")
        let plan = try driver.makeLaunchPlan(config: config, model: nil, params: ParamValues())
        XCTAssertEqual(plan.environment["OLLAMA_KEEP_ALIVE"], "1h")
    }

    func test_makeLaunchPlan_intEnvVar() throws {
        var config = makeConfig()
        config.advancedFlags["OLLAMA_MAX_LOADED_MODELS"] = .int(3)
        let plan = try driver.makeLaunchPlan(config: config, model: nil, params: ParamValues())
        XCTAssertEqual(plan.environment["OLLAMA_MAX_LOADED_MODELS"], "3")
    }

    // MARK: - validate

    func test_validate_missingExecutable_blocker() async {
        let config = makeConfig(executablePath: "/nonexistent/ollama")
        let issues = await driver.validate(config: config)
        XCTAssertTrue(issues.contains(where: { $0.severity == .blocker }))
    }

    func test_validate_portBelow1024_warning() async {
        // Use /bin/sh as a stand-in for an existing executable.
        let config = makeConfig(executablePath: "/bin/sh", port: 80)
        let issues = await driver.validate(config: config)
        XCTAssertTrue(issues.contains(where: { $0.severity == .warning }))
    }

    func test_validate_validConfig_noIssues() async {
        let config = makeConfig(executablePath: "/bin/sh", port: 11434)
        let issues = await driver.validate(config: config)
        XCTAssertTrue(issues.isEmpty)
    }
}
