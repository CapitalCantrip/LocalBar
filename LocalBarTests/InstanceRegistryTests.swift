import XCTest
@testable import LocalBar

/// Tests for InstanceRegistry's pure-logic surface (no network/process/disk I/O).
///
/// Covered:
///   - Initial state (empty controllers, empty profiles)
///   - addInstance / controller(for:)
///   - aggregateIconState / hasAnyRunning
///   - Profile CRUD (add, update, remove, profile(for:))
///   - removeProfile cascade — clears activeProfileID from instances
///   - makeController wiring — activeProfileProvider returns the right profile
@MainActor
final class InstanceRegistryTests: XCTestCase {

    // MARK: - Helpers

    private func makeRegistry(persistence: InMemoryPersistenceService = InMemoryPersistenceService()) -> InstanceRegistry {
        InstanceRegistry(persistence: persistence)
    }

    private func makeOllamaConfig(name: String = "Test", port: Int = 11434) -> ServerInstanceConfig {
        ServerInstanceConfig(name: name, type: .ollama, port: port,
                             executablePath: "/usr/local/bin/ollama")
    }

    private func makeMLXConfig(name: String = "MLX", port: Int = 8080) -> ServerInstanceConfig {
        var c = ServerInstanceConfig(name: name, type: .mlxLM, port: port,
                                     executablePath: "/usr/local/bin/uvx")
        c.selectedModelKey = "/models/llama3"
        return c
    }

    private func makeProfile(name: String = "Fast") -> NamedProfile {
        var params = ParamValues()
        params.values[.temperature] = .double(0.3)
        return NamedProfile(name: name, params: params)
    }

    // MARK: - Initial state

    func test_initial_noControllers() {
        let registry = makeRegistry()
        XCTAssertTrue(registry.controllers.isEmpty)
    }

    func test_initial_noProfiles() {
        let registry = makeRegistry()
        XCTAssertTrue(registry.profiles.isEmpty)
    }

    func test_initial_aggregateIconState_off() {
        let registry = makeRegistry()
        XCTAssertEqual(registry.aggregateIconState, .off)
    }

    func test_initial_hasAnyRunning_false() {
        let registry = makeRegistry()
        XCTAssertFalse(registry.hasAnyRunning)
    }

    // MARK: - addInstance

    func test_addInstance_appendsController() {
        let registry = makeRegistry()
        registry.addInstance(config: makeOllamaConfig(name: "Alpha"))
        XCTAssertEqual(registry.controllers.count, 1)
        XCTAssertEqual(registry.controllers.first?.config.name, "Alpha")
    }

    func test_addInstance_multiple_orderedByInsertion() {
        let registry = makeRegistry()
        registry.addInstance(config: makeOllamaConfig(name: "First", port: 11434))
        registry.addInstance(config: makeOllamaConfig(name: "Second", port: 11435))
        XCTAssertEqual(registry.controllers.count, 2)
        XCTAssertEqual(registry.controllers[0].config.name, "First")
        XCTAssertEqual(registry.controllers[1].config.name, "Second")
    }

    func test_addInstance_withInitialModels_seedsController() {
        let registry = makeRegistry()
        let models = [
            ModelRef(key: "llama3:8b", displayName: "Llama 3 8B", sizeBytes: nil, location: .serverManaged),
        ]
        registry.addInstance(config: makeOllamaConfig(), initialModels: models)
        XCTAssertEqual(registry.controllers.first?.availableModels.count, 1)
    }

    func test_addInstance_noInitialModels_emptyAvailableModels() {
        let registry = makeRegistry()
        registry.addInstance(config: makeOllamaConfig(), initialModels: [])
        XCTAssertTrue(registry.controllers.first?.availableModels.isEmpty ?? false)
    }

    // MARK: - controller(for:)

    func test_controllerForId_returnsMatchingController() {
        let registry = makeRegistry()
        registry.addInstance(config: makeOllamaConfig())
        let id = registry.controllers.first!.id
        XCTAssertNotNil(registry.controller(for: id))
        XCTAssertEqual(registry.controller(for: id)?.id, id)
    }

    func test_controllerForId_unknownId_returnsNil() {
        let registry = makeRegistry()
        registry.addInstance(config: makeOllamaConfig())
        XCTAssertNil(registry.controller(for: UUID()))
    }

    // MARK: - aggregateIconState / hasAnyRunning

    func test_aggregateIconState_allStopped_returnsOff() {
        let registry = makeRegistry()
        registry.addInstance(config: makeOllamaConfig(port: 11434))
        registry.addInstance(config: makeOllamaConfig(port: 11435))
        // Both are in .stopped(.neverStarted), which maps to .off
        XCTAssertEqual(registry.aggregateIconState, .off)
    }

    func test_hasAnyRunning_allStopped_false() {
        let registry = makeRegistry()
        registry.addInstance(config: makeOllamaConfig())
        XCTAssertFalse(registry.hasAnyRunning)
    }

    // MARK: - Profile CRUD

    func test_addProfile_appendsProfile() {
        let registry = makeRegistry()
        registry.addProfile(makeProfile(name: "Creative"))
        XCTAssertEqual(registry.profiles.count, 1)
        XCTAssertEqual(registry.profiles.first?.name, "Creative")
    }

    func test_addProfile_multiple_orderedByInsertion() {
        let registry = makeRegistry()
        registry.addProfile(makeProfile(name: "A"))
        registry.addProfile(makeProfile(name: "B"))
        XCTAssertEqual(registry.profiles.map(\.name), ["A", "B"])
    }

    func test_updateProfile_changesExistingProfile() {
        let registry = makeRegistry()
        let original = makeProfile(name: "Original")
        registry.addProfile(original)

        var updated = original
        updated.name = "Updated"
        registry.updateProfile(updated)

        XCTAssertEqual(registry.profiles.count, 1)
        XCTAssertEqual(registry.profiles.first?.name, "Updated")
    }

    func test_updateProfile_unknownId_doesNothing() {
        let registry = makeRegistry()
        registry.addProfile(makeProfile(name: "Existing"))
        let phantom = makeProfile(name: "Ghost")
        registry.updateProfile(phantom) // unknown ID — should be a no-op
        XCTAssertEqual(registry.profiles.count, 1)
        XCTAssertEqual(registry.profiles.first?.name, "Existing")
    }

    func test_removeProfile_removesById() {
        let registry = makeRegistry()
        let p = makeProfile(name: "ToRemove")
        registry.addProfile(p)
        XCTAssertEqual(registry.profiles.count, 1)
        registry.removeProfile(id: p.id)
        XCTAssertTrue(registry.profiles.isEmpty)
    }

    func test_removeProfile_leavesOtherProfilesIntact() {
        let registry = makeRegistry()
        let a = makeProfile(name: "A")
        let b = makeProfile(name: "B")
        registry.addProfile(a)
        registry.addProfile(b)
        registry.removeProfile(id: a.id)
        XCTAssertEqual(registry.profiles.count, 1)
        XCTAssertEqual(registry.profiles.first?.name, "B")
    }

    func test_removeProfile_clearsActiveProfileIDOnInstances() {
        let registry = makeRegistry()
        let profile = makeProfile(name: "Active")
        registry.addProfile(profile)

        // Wire the instance to use this profile
        var config = makeOllamaConfig()
        config.activeProfileID = profile.id
        registry.addInstance(config: config)

        // Sanity check the profile is wired
        XCTAssertEqual(registry.controllers.first?.config.activeProfileID, profile.id)

        // Remove the profile
        registry.removeProfile(id: profile.id)

        // activeProfileID must have been cleared
        XCTAssertNil(registry.controllers.first?.config.activeProfileID)
    }

    func test_removeProfile_unknownId_doesNothing() {
        let registry = makeRegistry()
        registry.addProfile(makeProfile(name: "Safe"))
        registry.removeProfile(id: UUID()) // doesn't exist
        XCTAssertEqual(registry.profiles.count, 1)
    }

    // MARK: - profile(for:)

    func test_profileForId_returnsMatchingProfile() {
        let registry = makeRegistry()
        let profile = makeProfile(name: "Lookup")
        registry.addProfile(profile)
        let found = registry.profile(for: profile.id)
        XCTAssertEqual(found?.name, "Lookup")
    }

    func test_profileForId_nilReturnsNil() {
        let registry = makeRegistry()
        registry.addProfile(makeProfile(name: "Exists"))
        XCTAssertNil(registry.profile(for: nil))
    }

    func test_profileForId_unknownIdReturnsNil() {
        let registry = makeRegistry()
        registry.addProfile(makeProfile(name: "Exists"))
        XCTAssertNil(registry.profile(for: UUID()))
    }

    // MARK: - makeController wiring: activeProfileProvider

    func test_makeController_activeProfileProvider_returnsCorrectProfile() {
        let registry = makeRegistry()
        let profile = makeProfile(name: "Wired")
        registry.addProfile(profile)

        var config = makeOllamaConfig()
        config.activeProfileID = profile.id
        registry.addInstance(config: config)

        let controller = registry.controllers.first!
        let resolved = controller.activeProfileProvider?()
        XCTAssertEqual(resolved?.name, "Wired")
    }

    func test_makeController_activeProfileProvider_nilWhenNoProfile() {
        let registry = makeRegistry()
        registry.addInstance(config: makeOllamaConfig()) // no activeProfileID set
        let controller = registry.controllers.first!
        XCTAssertNil(controller.activeProfileProvider?())
    }

    func test_makeController_activeProfileProvider_nilAfterProfileRemoved() {
        let registry = makeRegistry()
        let profile = makeProfile(name: "Soon Gone")
        registry.addProfile(profile)

        var config = makeOllamaConfig()
        config.activeProfileID = profile.id
        registry.addInstance(config: config)

        registry.removeProfile(id: profile.id)

        // After removal, activeProfileID is cleared, so provider returns nil
        let controller = registry.controllers.first!
        XCTAssertNil(controller.activeProfileProvider?())
    }

    // MARK: - Persistence seam

    func test_addInstance_savesInstancesToPersistence() async {
        let store = InMemoryPersistenceService()
        let registry = makeRegistry(persistence: store)
        registry.addInstance(config: makeOllamaConfig(name: "Persisted"))
        // Fire-and-forget Task in persistInstances — drain the cooperative queue.
        try? await Task.sleep(nanoseconds: 10_000_000)
        let saved = await store.instances
        XCTAssertEqual(saved.count, 1)
        XCTAssertEqual(saved.first?.name, "Persisted")
    }

    func test_addProfile_savesProfileToPersistence() async {
        let store = InMemoryPersistenceService()
        let registry = makeRegistry(persistence: store)
        registry.addProfile(makeProfile(name: "Saved"))
        try? await Task.sleep(nanoseconds: 10_000_000)
        let saved = await store.profiles
        XCTAssertEqual(saved.count, 1)
        XCTAssertEqual(saved.first?.name, "Saved")
    }

    func test_bootstrap_loadsInstancesFromPersistence() async {
        let store = InMemoryPersistenceService()
        // Seed the store before bootstrap.
        try? await store.saveInstances([makeOllamaConfig(name: "Restored")])
        let registry = makeRegistry(persistence: store)
        await registry.bootstrap()
        XCTAssertEqual(registry.controllers.count, 1)
        XCTAssertEqual(registry.controllers.first?.config.name, "Restored")
    }

    func test_bootstrap_loadsProfilesFromPersistence() async {
        let store = InMemoryPersistenceService()
        try? await store.saveProfiles([makeProfile(name: "RestoredProfile")])
        let registry = makeRegistry(persistence: store)
        await registry.bootstrap()
        XCTAssertEqual(registry.profiles.count, 1)
        XCTAssertEqual(registry.profiles.first?.name, "RestoredProfile")
    }

    func test_removeInstance_savesUpdatedInstancesToPersistence() async {
        let store = InMemoryPersistenceService()
        let registry = makeRegistry(persistence: store)
        let config = makeOllamaConfig(name: "ToRemove")
        registry.addInstance(config: config)
        let id = registry.controllers.first!.id
        registry.removeInstance(id: id)
        // removeInstance dispatches a Task — allow it to settle.
        try? await Task.sleep(nanoseconds: 50_000_000)
        let saved = await store.instances
        XCTAssertTrue(saved.isEmpty)
    }
}
