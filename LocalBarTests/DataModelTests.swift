import XCTest
@testable import LocalBar

final class DataModelTests: XCTestCase {

    // MARK: - Helpers

    private func makeDescriptor(_ param: CanonicalParam, default def: ParamValue) -> ParamDescriptor {
        ParamDescriptor(param: param, serverFlagName: "--x", application: .launchArgument,
                        valueType: .double(range: nil), defaultValue: def)
    }

    private func makeDescriptorNoDefault(_ param: CanonicalParam) -> ParamDescriptor {
        ParamDescriptor(param: param, serverFlagName: "--x", application: .launchArgument,
                        valueType: .double(range: nil), defaultValue: nil)
    }

    // MARK: - ParamValues.resolve — driver defaults

    func test_resolve_driverDefaults_noMemoryNoProfile() {
        let schema = [makeDescriptor(.temperature, default: .double(0.8))]
        let result = ParamValues.resolve(profile: nil, memory: nil, driverDefaults: schema)
        XCTAssertEqual(result[.temperature], .double(0.8))
    }

    func test_resolve_driverDefault_missingWhenNoDefault() {
        let schema = [makeDescriptorNoDefault(.temperature)]
        let result = ParamValues.resolve(profile: nil, memory: nil, driverDefaults: schema)
        XCTAssertNil(result[.temperature])
    }

    // MARK: - ParamValues.resolve — memory overrides defaults

    func test_resolve_memoryOverridesDefault() {
        let schema = [makeDescriptor(.temperature, default: .double(0.8))]
        var mem = ModelMemory(modelKey: "m", lastUsedParams: ParamValues(), lastUsedAt: .now)
        mem.lastUsedParams.values[.temperature] = .double(0.5)
        let result = ParamValues.resolve(profile: nil, memory: mem, driverDefaults: schema)
        XCTAssertEqual(result[.temperature], .double(0.5))
    }

    func test_resolve_memorySystemPrompt() {
        var mem = ModelMemory(modelKey: "m", lastUsedParams: ParamValues(), lastUsedAt: .now)
        mem.lastUsedParams.systemPrompt = "be helpful"
        let result = ParamValues.resolve(profile: nil, memory: mem, driverDefaults: [])
        XCTAssertEqual(result.systemPrompt, "be helpful")
    }

    // MARK: - ParamValues.resolve — profile overrides everything

    func test_resolve_profileOverridesMemory() {
        let schema = [makeDescriptor(.temperature, default: .double(0.8))]
        var mem = ModelMemory(modelKey: "m", lastUsedParams: ParamValues(), lastUsedAt: .now)
        mem.lastUsedParams.values[.temperature] = .double(0.5)
        var profileParams = ParamValues()
        profileParams.values[.temperature] = .double(1.2)
        let profile = NamedProfile(name: "High Temp", params: profileParams)
        let result = ParamValues.resolve(profile: profile, memory: mem, driverDefaults: schema)
        XCTAssertEqual(result[.temperature], .double(1.2))
    }

    func test_resolve_profileSystemPromptOverridesMemory() {
        var mem = ModelMemory(modelKey: "m", lastUsedParams: ParamValues(), lastUsedAt: .now)
        mem.lastUsedParams.systemPrompt = "from memory"
        var profileParams = ParamValues()
        profileParams.systemPrompt = "from profile"
        let profile = NamedProfile(name: "P", params: profileParams)
        let result = ParamValues.resolve(profile: profile, memory: mem, driverDefaults: [])
        XCTAssertEqual(result.systemPrompt, "from profile")
    }

    func test_resolve_fullStack_profileWins() {
        let schema = [
            makeDescriptor(.temperature, default: .double(0.8)),
            makeDescriptor(.topP, default: .double(0.9)),
        ]
        var mem = ModelMemory(modelKey: "m", lastUsedParams: ParamValues(), lastUsedAt: .now)
        mem.lastUsedParams.values[.temperature] = .double(0.5)
        // topP not in memory — falls through to driver default
        var profileParams = ParamValues()
        profileParams.values[.temperature] = .double(1.0)
        let profile = NamedProfile(name: "P", params: profileParams)
        let result = ParamValues.resolve(profile: profile, memory: mem, driverDefaults: schema)
        XCTAssertEqual(result[.temperature], .double(1.0)) // profile wins
        XCTAssertEqual(result[.topP], .double(0.9))        // driver default
    }

    // MARK: - ParamValues.filtered

    func test_filtered_keepsOnlySupportedParams() {
        var pv = ParamValues()
        pv.values[.temperature] = .double(0.7)
        pv.values[.topK] = .int(40)
        let schema = [makeDescriptor(.temperature, default: .double(0.8))] // topK not in schema
        let filtered = pv.filtered(to: schema)
        XCTAssertEqual(filtered[.temperature], .double(0.7))
        XCTAssertNil(filtered[.topK])
    }

    func test_filtered_keepsSystemPromptRegardlessOfSchema() {
        var pv = ParamValues()
        pv.systemPrompt = "hello"
        let filtered = pv.filtered(to: [])
        XCTAssertEqual(filtered.systemPrompt, "hello")
    }

    func test_filtered_emptySchema_stripsAllValues() {
        var pv = ParamValues()
        pv.values[.temperature] = .double(1.0)
        pv.values[.topP] = .double(0.9)
        let filtered = pv.filtered(to: [])
        XCTAssertTrue(filtered.values.isEmpty)
    }

    // MARK: - ModelMemory.estimatedRestartDuration

    func test_estimatedRestartDuration_emptyReturnsNil() {
        let mem = ModelMemory(modelKey: "m", lastUsedParams: ParamValues(), lastUsedAt: .now)
        XCTAssertNil(mem.estimatedRestartDuration)
    }

    func test_estimatedRestartDuration_singleSampleReturnsThatSample() {
        var mem = ModelMemory(modelKey: "m", lastUsedParams: ParamValues(), lastUsedAt: .now)
        mem.restartDurationSamples = [10.0]
        XCTAssertEqual(mem.estimatedRestartDuration ?? -1, 10.0, accuracy: 0.001)
    }

    func test_estimatedRestartDuration_twoSamples_ewmaNewestHighestWeight() throws {
        var mem = ModelMemory(modelKey: "m", lastUsedParams: ParamValues(), lastUsedAt: .now)
        // Stored newest-first: [20, 10]. EWMA seeds on oldest (10), then applies newest (20).
        mem.restartDurationSamples = [20.0, 10.0]
        // ewma = 0.3 * 20 + 0.7 * 10 = 6 + 7 = 13
        let result = try XCTUnwrap(mem.estimatedRestartDuration)
        XCTAssertEqual(result, 13.0, accuracy: 0.001)
    }

    func test_estimatedRestartDuration_convergesToNewestOverManyConsistentSamples() throws {
        var mem = ModelMemory(modelKey: "m", lastUsedParams: ParamValues(), lastUsedAt: .now)
        // 10 identical samples of 30 s — EWMA should equal 30.
        mem.restartDurationSamples = Array(repeating: 30.0, count: 10)
        let result = try XCTUnwrap(mem.estimatedRestartDuration)
        XCTAssertEqual(result, 30.0, accuracy: 0.001)
    }

    // MARK: - ModelMemory.recordRestartDuration

    func test_recordRestartDuration_insertsNewestFirst() {
        var mem = ModelMemory(modelKey: "m", lastUsedParams: ParamValues(), lastUsedAt: .now)
        mem.recordRestartDuration(5.0)
        mem.recordRestartDuration(10.0)
        XCTAssertEqual(mem.restartDurationSamples.first, 10.0)
        XCTAssertEqual(mem.restartDurationSamples.last, 5.0)
    }

    func test_recordRestartDuration_capsAtTen() {
        var mem = ModelMemory(modelKey: "m", lastUsedParams: ParamValues(), lastUsedAt: .now)
        for i in 1...12 { mem.recordRestartDuration(TimeInterval(i)) }
        XCTAssertEqual(mem.restartDurationSamples.count, 10)
        XCTAssertEqual(mem.restartDurationSamples.first, 12.0) // newest at front
    }
}
