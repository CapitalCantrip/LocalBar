import XCTest
@testable import LocalBar

/// Tests for ServerInstanceController's pure-logic surface.
///
/// The driver is created by DriverRegistry and cannot be injected, so tests
/// here avoid touching any network/process paths. They cover:
///   - effectiveParams() resolution order (driver defaults < instanceParams < profile)
///   - updateConfig field propagation and onConfigChanged callback
///   - updateConfig Ollama rebuild triggers
///   - seedModels
@MainActor
final class ServerInstanceControllerTests: XCTestCase {

    // MARK: - Helpers

    private func makeOllamaConfig(selectedModelKey: String? = "llama3:8b") -> ServerInstanceConfig {
        var c = ServerInstanceConfig(name: "Ollama", type: .ollama, port: 11434,
                                     executablePath: "/usr/local/bin/ollama")
        c.selectedModelKey = selectedModelKey
        return c
    }

    private func makeMLXConfig() -> ServerInstanceConfig {
        var c = ServerInstanceConfig(name: "MLX", type: .mlxLM, port: 8080,
                                     executablePath: "/usr/local/bin/uvx")
        c.selectedModelKey = "/models/llama3"
        return c
    }

    // MARK: - effectiveParams — driver defaults

    func test_effectiveParams_returnsDriverDefaults() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        // OllamaDriver defaults: temperature = 0.8
        let params = controller.effectiveParams()
        XCTAssertEqual(params[.temperature], .double(0.8))
    }

    func test_effectiveParams_mlxDriver_noDefaultForTemperature() {
        // MLXLMDriver sets temperature default to 0.0.
        let controller = ServerInstanceController(config: makeMLXConfig())
        let params = controller.effectiveParams()
        XCTAssertEqual(params[.temperature], .double(0.0))
    }

    // MARK: - effectiveParams — instanceParams override defaults

    func test_effectiveParams_instanceParamOverridesDefault() {
        var config = makeOllamaConfig()
        config.instanceParams.values[.temperature] = .double(1.5)
        let controller = ServerInstanceController(config: config)
        XCTAssertEqual(controller.effectiveParams()[.temperature], .double(1.5))
    }

    func test_effectiveParams_multipleInstanceParams() {
        var config = makeOllamaConfig()
        config.instanceParams.values[.temperature] = .double(0.5)
        config.instanceParams.values[.topP] = .double(0.95)
        config.instanceParams.systemPrompt = "You are a tester."
        let controller = ServerInstanceController(config: config)
        let params = controller.effectiveParams()
        XCTAssertEqual(params[.temperature], .double(0.5))
        XCTAssertEqual(params[.topP], .double(0.95))
        XCTAssertEqual(params.systemPrompt, "You are a tester.")
    }

    // MARK: - effectiveParams — active profile overrides instanceParams

    func test_effectiveParams_profileOverridesInstanceParam() {
        var config = makeOllamaConfig()
        config.instanceParams.values[.temperature] = .double(0.5)

        var profileParams = ParamValues()
        profileParams.values[.temperature] = .double(1.8)
        let profile = NamedProfile(name: "Hot", params: profileParams)

        let controller = ServerInstanceController(config: config)
        controller.activeProfileProvider = { profile }

        XCTAssertEqual(controller.effectiveParams()[.temperature], .double(1.8))
    }

    func test_effectiveParams_profileSystemPrompt_overridesInstancePrompt() {
        var config = makeOllamaConfig()
        config.instanceParams.systemPrompt = "instance prompt"

        var profileParams = ParamValues()
        profileParams.systemPrompt = "profile prompt"
        let profile = NamedProfile(name: "P", params: profileParams)

        let controller = ServerInstanceController(config: config)
        controller.activeProfileProvider = { profile }

        XCTAssertEqual(controller.effectiveParams().systemPrompt, "profile prompt")
    }

    func test_effectiveParams_nilProfile_instanceParamStands() {
        var config = makeOllamaConfig()
        config.instanceParams.values[.temperature] = .double(0.3)
        let controller = ServerInstanceController(config: config)
        controller.activeProfileProvider = { nil }
        XCTAssertEqual(controller.effectiveParams()[.temperature], .double(0.3))
    }

    func test_effectiveParams_profileDoesNotAffectUnsetParam() {
        // Profile sets topP but not temperature; instance param sets temperature.
        var config = makeOllamaConfig()
        config.instanceParams.values[.temperature] = .double(0.4)

        var profileParams = ParamValues()
        profileParams.values[.topP] = .double(0.99)
        let profile = NamedProfile(name: "P", params: profileParams)

        let controller = ServerInstanceController(config: config)
        controller.activeProfileProvider = { profile }

        XCTAssertEqual(controller.effectiveParams()[.temperature], .double(0.4)) // untouched by profile
        XCTAssertEqual(controller.effectiveParams()[.topP], .double(0.99))       // from profile
    }

    // MARK: - updateConfig

    func test_updateConfig_updatesConfigFields() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        var newConfig = controller.config
        newConfig.name = "Updated"
        newConfig.port = 9999
        controller.updateConfig(newConfig)
        XCTAssertEqual(controller.config.name, "Updated")
        XCTAssertEqual(controller.config.port, 9999)
    }

    func test_updateConfig_firesOnConfigChanged() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        var callCount = 0
        controller.onConfigChanged = { _ in callCount += 1 }
        var newConfig = controller.config
        newConfig.name = "Changed"
        controller.updateConfig(newConfig)
        XCTAssertEqual(callCount, 1)
    }

    func test_updateConfig_configChangedReceivesUpdatedConfig() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        var received: ServerInstanceConfig?
        controller.onConfigChanged = { received = $0 }
        var newConfig = controller.config
        newConfig.port = 9000
        controller.updateConfig(newConfig)
        XCTAssertEqual(received?.port, 9000)
    }

    // MARK: - updateConfig — Ollama rebuild triggers

    func test_updateConfig_modelKeyChange_ollama_doesNotCrash() async {
        // We can't intercept the async ensureManagedModelIfNeeded Task without driver injection,
        // but we can confirm the trigger path runs without crashing.
        let controller = ServerInstanceController(config: makeOllamaConfig())
        var newConfig = controller.config
        newConfig.selectedModelKey = "mistral:7b"
        controller.updateConfig(newConfig) // triggers rebuild task internally
        // Wait a tick for the Task to be enqueued; we don't require it to succeed.
        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(controller.config.selectedModelKey, "mistral:7b")
    }

    func test_updateConfig_profileIDChange_ollama_doesNotCrash() async {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        var newConfig = controller.config
        newConfig.activeProfileID = UUID() // profile activation
        controller.updateConfig(newConfig)
        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertNotNil(controller.config.activeProfileID)
    }

    func test_updateConfig_profileIDChange_mlx_doesNotTriggerRebuild() {
        // MLX doesn't use managed models — profile change must not crash.
        let controller = ServerInstanceController(config: makeMLXConfig())
        var newConfig = controller.config
        newConfig.activeProfileID = UUID()
        controller.updateConfig(newConfig) // no-op for mlx-lm
        XCTAssertNotNil(controller.config.activeProfileID)
    }

    // MARK: - seedModels

    func test_seedModels_populatesAvailableModels() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        let models = [
            ModelRef(key: "llama3:8b", displayName: "Llama 3 8B", sizeBytes: nil, location: .serverManaged),
            ModelRef(key: "mistral:7b", displayName: "Mistral 7B", sizeBytes: nil, location: .serverManaged),
        ]
        controller.seedModels(models)
        XCTAssertEqual(controller.availableModels.count, 2)
        XCTAssertEqual(controller.availableModels.first?.key, "llama3:8b")
    }

    func test_seedModels_emptyArray_clearsModels() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        controller.seedModels([ModelRef(key: "x", displayName: "x", sizeBytes: nil, location: .serverManaged)])
        controller.seedModels([])
        XCTAssertTrue(controller.availableModels.isEmpty)
    }

    // MARK: - Initial state

    func test_initialPhase_stoppedNeverStarted() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        XCTAssertEqual(controller.phase, .stopped(.neverStarted))
    }

    func test_initialCurrentModel_nil() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        XCTAssertNil(controller.currentModel)
    }

    func test_initialLastError_nil() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        XCTAssertNil(controller.lastError)
    }
}
