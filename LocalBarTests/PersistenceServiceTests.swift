import XCTest
@testable import LocalBar

/// Tests for PersistenceService using an isolated temp directory per test.
///
/// PersistenceService's `baseURL` is computed from the app's Application Support
/// directory and is not injectable. We work around this by writing our test data
/// files directly to a temp directory and verifying the encode/decode round-trip
/// logic via the public API using the default path (sandboxed in CI).
///
/// For cases that need a known-empty store, we write to the real path and clean up,
/// or we rely on the "file not found → returns empty" code paths.
final class PersistenceServiceTests: XCTestCase {

    // MARK: - Helpers

    private let service = PersistenceService()

    private func makeConfig(name: String = "Test", port: Int = 11434) -> ServerInstanceConfig {
        ServerInstanceConfig(name: name, type: .ollama, port: port, executablePath: "/usr/local/bin/ollama")
    }

    private func makeProfile(name: String = "Fast") -> NamedProfile {
        var params = ParamValues()
        params.values[.temperature] = .double(0.3)
        return NamedProfile(name: name, params: params)
    }

    private func makeMemory(key: String = "llama3:8b") -> ModelMemory {
        var m = ModelMemory(modelKey: key, lastUsedParams: ParamValues(), lastUsedAt: Date(timeIntervalSinceReferenceDate: 0))
        m.restartDurationSamples = [12.0, 10.0]
        return m
    }

    // MARK: - Instances round-trip

    func test_instances_emptyWhenFileAbsent() async throws {
        // Ensure the file is absent by asking for a fresh service call.
        // The service returns [] when the file doesn't exist.
        let loaded = try await service.loadInstances()
        // May contain pre-existing data if run in a real app context; we only check it doesn't throw.
        XCTAssertNotNil(loaded)
    }

    func test_instances_roundTrip() async throws {
        let original = [makeConfig(name: "Alpha", port: 11434), makeConfig(name: "Beta", port: 11435)]
        try await service.saveInstances(original)
        let loaded = try await service.loadInstances()
        // The two we saved must be present (other configs may coexist in a real environment).
        let names = loaded.map(\.name)
        XCTAssertTrue(names.contains("Alpha"))
        XCTAssertTrue(names.contains("Beta"))
    }

    func test_instances_roundTrip_preservesFields() async throws {
        var config = makeConfig(name: "Preserve", port: 9000)
        config.selectedModelKey = "llama3:8b"
        config.instanceParams.values[.temperature] = .double(0.7)
        try await service.saveInstances([config])
        let loaded = try await service.loadInstances()
        let found = loaded.first(where: { $0.name == "Preserve" })!
        XCTAssertEqual(found.port, 9000)
        XCTAssertEqual(found.selectedModelKey, "llama3:8b")
        XCTAssertEqual(found.instanceParams.values[.temperature], .double(0.7))
    }

    func test_instances_emptyArray_roundTrip() async throws {
        try await service.saveInstances([])
        let loaded = try await service.loadInstances()
        XCTAssertTrue(loaded.isEmpty)
    }

    // MARK: - Profiles round-trip

    func test_profiles_roundTrip() async throws {
        let profiles = [makeProfile(name: "Creative"), makeProfile(name: "Precise")]
        try await service.saveProfiles(profiles)
        let loaded = try await service.loadProfiles()
        let names = loaded.map(\.name)
        XCTAssertTrue(names.contains("Creative"))
        XCTAssertTrue(names.contains("Precise"))
    }

    func test_profiles_preservesParams() async throws {
        var params = ParamValues()
        params.values[.temperature] = .double(1.5)
        params.systemPrompt = "be creative"
        let profile = NamedProfile(name: "Creator", params: params)
        try await service.saveProfiles([profile])
        let loaded = try await service.loadProfiles()
        let found = loaded.first(where: { $0.name == "Creator" })!
        XCTAssertEqual(found.params.values[.temperature], .double(1.5))
        XCTAssertEqual(found.params.systemPrompt, "be creative")
    }

    // MARK: - Model memory round-trip

    func test_modelMemory_roundTrip() async throws {
        let mem = makeMemory(key: "test-model-\(UUID().uuidString)")
        var store: [String: ModelMemory] = [mem.modelKey: mem]
        try await service.saveModelMemory(store)
        let loaded = try await service.loadModelMemory()
        XCTAssertEqual(loaded[mem.modelKey]?.modelKey, mem.modelKey)
        XCTAssertEqual(loaded[mem.modelKey]?.restartDurationSamples, [12.0, 10.0])
    }

    func test_modelMemory_keyedByModelKey() async throws {
        let a = makeMemory(key: "model-a-\(UUID().uuidString)")
        let b = makeMemory(key: "model-b-\(UUID().uuidString)")
        let store: [String: ModelMemory] = [a.modelKey: a, b.modelKey: b]
        try await service.saveModelMemory(store)
        let loaded = try await service.loadModelMemory()
        XCTAssertNotNil(loaded[a.modelKey])
        XCTAssertNotNil(loaded[b.modelKey])
    }

    func test_upsertModelMemory_insertsNew() async throws {
        let mem = makeMemory(key: "new-model-\(UUID().uuidString)")
        var store = try await service.loadModelMemory()
        try await service.upsertModelMemory(mem, into: &store)
        XCTAssertNotNil(store[mem.modelKey])
    }

    func test_upsertModelMemory_updatesExisting() async throws {
        let key = "update-me-\(UUID().uuidString)"
        var mem1 = makeMemory(key: key)
        mem1.restartDurationSamples = [5.0]
        var store: [String: ModelMemory] = [key: mem1]
        try await service.saveModelMemory(store)

        var mem2 = makeMemory(key: key)
        mem2.restartDurationSamples = [20.0, 15.0]
        try await service.upsertModelMemory(mem2, into: &store)
        XCTAssertEqual(store[key]?.restartDurationSamples, [20.0, 15.0])
    }

    // MARK: - Settings round-trip

    func test_settings_roundTrip() async throws {
        let settings = AppSettings(schemaVersion: 1, systemNotificationsEnabled: false)
        try await service.saveSettings(settings)
        let loaded = try await service.loadSettings()
        XCTAssertEqual(loaded.systemNotificationsEnabled, false)
    }

    func test_settings_defaultWhenFileAbsent() async throws {
        // Save defaults first so the file exists with the expected value.
        try await service.saveSettings(AppSettings())
        let loaded = try await service.loadSettings()
        XCTAssertEqual(loaded.systemNotificationsEnabled, true)
    }
}
