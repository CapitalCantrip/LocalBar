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

    // MARK: - resolvedContextLength

    func test_resolvedContextLength_explicitParam_usesIt() {
        var config = makeMLXConfig()
        config.instanceParams.values[.contextLength] = .int(8192)
        let controller = ServerInstanceController(config: config)
        let params = controller.effectiveParams()
        XCTAssertEqual(controller.resolvedContextLength(params: params), 8192)
    }

    func test_resolvedContextLength_noParam_ollama_defaults2048() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        XCTAssertEqual(controller.resolvedContextLength(params: ParamValues()), 2048)
    }

    func test_resolvedContextLength_noParam_mlx_defaults4096() {
        let controller = ServerInstanceController(config: makeMLXConfig())
        XCTAssertEqual(controller.resolvedContextLength(params: ParamValues()), 4096)
    }

    // MARK: - resolvedKVCacheBits

    func test_resolvedKVCacheBits_mlx_withFlag_usesFlag() {
        var config = makeMLXConfig()
        config.advancedFlags["--kv-cache-bits"] = .int(4)
        let controller = ServerInstanceController(config: config)
        XCTAssertEqual(controller.resolvedKVCacheBits(), 4)
    }

    func test_resolvedKVCacheBits_mlx_noFlag_returns16() {
        let controller = ServerInstanceController(config: makeMLXConfig())
        XCTAssertEqual(controller.resolvedKVCacheBits(), 16)
    }

    func test_resolvedKVCacheBits_ollama_alwaysReturns16() {
        var config = makeOllamaConfig()
        // Ollama doesn't have a kv-cache-bits flag; always bf16.
        config.advancedFlags["--kv-cache-bits"] = .int(8) // would be ignored for Ollama
        let controller = ServerInstanceController(config: config)
        XCTAssertEqual(controller.resolvedKVCacheBits(), 16)
    }

    // MARK: - estimatedMemoryFootprint

    func test_estimatedMemoryFootprint_noModel_returnsZeroEstimate() async {
        var config = makeMLXConfig()
        config.selectedModelKey = nil
        let controller = ServerInstanceController(config: config)
        let estimate = await controller.estimatedMemoryFootprint()
        XCTAssertNil(estimate.weightBytes)
        XCTAssertNil(estimate.kvCacheBytes)
        XCTAssertEqual(estimate.totalBytes, 0)
    }

    func test_estimatedMemoryFootprint_seededModel_usesDirectSize() async {
        var config = makeMLXConfig()
        config.selectedModelKey = "test-model"
        let controller = ServerInstanceController(config: config)
        let model = ModelRef(
            key: "test-model", displayName: "Test Model",
            sizeBytes: 4_000_000_000, location: .filesystem(path: "/models/test")
        )
        controller.seedModels([model])
        let estimate = await controller.estimatedMemoryFootprint()
        XCTAssertEqual(estimate.weightBytes, 4_000_000_000)
    }

    func test_estimatedMemoryFootprint_seededModelWithArchitecture_computesKVCache() async {
        var config = makeMLXConfig()
        config.selectedModelKey = "arch-model"
        let controller = ServerInstanceController(config: config)
        var meta = ModelMetadata()
        meta.numHiddenLayers = 32
        meta.numKVHeads = 8
        meta.headDim = 128
        let model = ModelRef(
            key: "arch-model", displayName: "Arch Model",
            sizeBytes: nil, location: .filesystem(path: "/models/arch"), metadata: meta
        )
        controller.seedModels([model])
        let estimate = await controller.estimatedMemoryFootprint()
        // kvCacheBytes = 2 × 32 × 8 × 128 × 4096 × 2 bytes (bf16)
        let expected = Int64(2 * 32 * 8 * 128 * 4096) * 2
        XCTAssertEqual(estimate.kvCacheBytes, expected)
    }

    func test_estimatedMemoryFootprint_contextLengthParam_affectsKVCache() async {
        var config = makeMLXConfig()
        config.selectedModelKey = "ctx-model"
        config.instanceParams.values[.contextLength] = .int(2048)
        let controller = ServerInstanceController(config: config)
        var meta = ModelMetadata()
        meta.numHiddenLayers = 32
        meta.numKVHeads = 8
        meta.headDim = 128
        let model = ModelRef(
            key: "ctx-model", displayName: "Ctx Model",
            sizeBytes: nil, location: .filesystem(path: "/m"), metadata: meta
        )
        controller.seedModels([model])
        let estimate = await controller.estimatedMemoryFootprint()
        // At ctx=2048 instead of default 4096, KV cache is halved.
        let expected = Int64(2 * 32 * 8 * 128 * 2048) * 2
        XCTAssertEqual(estimate.kvCacheBytes, expected)
    }

    // MARK: - transition

    func test_transition_setsPhase() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        controller.transition(to: .running)
        XCTAssertEqual(controller.phase, .running)
    }

    func test_transition_error_setsLastError() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        let err = InstanceError(kind: .launchFailed, message: "boom")
        controller.transition(to: .error(err))
        XCTAssertEqual(controller.lastError, err)
        XCTAssertEqual(controller.phase, .error(err))
    }

    func test_transition_stopped_doesNotOverwriteLastError() {
        let controller = ServerInstanceController(config: makeOllamaConfig())
        let err = InstanceError(kind: .launchFailed, message: "boom")
        controller.transition(to: .error(err))
        controller.transition(to: .stopped(.userStopped))
        XCTAssertEqual(controller.phase, .stopped(.userStopped))
        // lastError stays; it's cleared by the caller (e.g. retryFromError)
        XCTAssertEqual(controller.lastError, err)
    }

    // MARK: - syncWasRunningWhenQuit

    func test_syncWasRunningWhenQuit_running_setsTrue() {
        var config = makeOllamaConfig()
        config.wasRunningWhenQuit = false
        let controller = ServerInstanceController(config: config)
        var callCount = 0
        controller.onConfigChanged = { _ in callCount += 1 }
        controller.syncWasRunningWhenQuit(for: .running)
        XCTAssertTrue(controller.config.wasRunningWhenQuit)
        XCTAssertEqual(callCount, 1)
    }

    func test_syncWasRunningWhenQuit_running_alreadyTrue_noCallback() {
        var config = makeOllamaConfig()
        config.wasRunningWhenQuit = true
        let controller = ServerInstanceController(config: config)
        var callCount = 0
        controller.onConfigChanged = { _ in callCount += 1 }
        controller.syncWasRunningWhenQuit(for: .running)
        XCTAssertTrue(controller.config.wasRunningWhenQuit)
        XCTAssertEqual(callCount, 0) // no change, no callback
    }

    func test_syncWasRunningWhenQuit_stopped_setsFalse() {
        var config = makeOllamaConfig()
        config.wasRunningWhenQuit = true
        let controller = ServerInstanceController(config: config)
        var callCount = 0
        controller.onConfigChanged = { _ in callCount += 1 }
        controller.syncWasRunningWhenQuit(for: .stopped(.userStopped))
        XCTAssertFalse(controller.config.wasRunningWhenQuit)
        XCTAssertEqual(callCount, 1)
    }

    func test_syncWasRunningWhenQuit_error_setsFalse() {
        var config = makeOllamaConfig()
        config.wasRunningWhenQuit = true
        let controller = ServerInstanceController(config: config)
        let err = InstanceError(kind: .crashed(exitCode: 1), message: "x")
        controller.syncWasRunningWhenQuit(for: .error(err))
        XCTAssertFalse(controller.config.wasRunningWhenQuit)
    }

    func test_syncWasRunningWhenQuit_starting_isNoop() {
        var config = makeOllamaConfig()
        config.wasRunningWhenQuit = false
        let controller = ServerInstanceController(config: config)
        var callCount = 0
        controller.onConfigChanged = { _ in callCount += 1 }
        controller.syncWasRunningWhenQuit(for: .starting)
        XCTAssertFalse(controller.config.wasRunningWhenQuit) // unchanged
        XCTAssertEqual(callCount, 0)
    }
}
