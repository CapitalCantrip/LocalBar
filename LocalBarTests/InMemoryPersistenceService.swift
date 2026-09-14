import Foundation
@testable import LocalBar

/// In-memory adapter for `PersistenceServiceProtocol`.
/// Satisfies the seam for unit tests — no disk I/O, no ~/Library writes.
actor InMemoryPersistenceService: PersistenceServiceProtocol {
    var instances: [ServerInstanceConfig] = []
    var profiles: [NamedProfile] = []
    var modelMemory: [String: ModelMemory] = [:]

    func ensureDirectoryExists() throws {}
    func loadInstances() throws -> [ServerInstanceConfig] { instances }
    func saveInstances(_ value: [ServerInstanceConfig]) throws { instances = value }
    func loadProfiles() throws -> [NamedProfile] { profiles }
    func saveProfiles(_ value: [NamedProfile]) throws { profiles = value }
    func loadModelMemory() throws -> [String: ModelMemory] { modelMemory }
    func saveModelMemory(_ value: [String: ModelMemory]) throws { modelMemory = value }
    func upsertModelMemory(_ entry: ModelMemory, into store: inout [String: ModelMemory]) throws {
        store[entry.modelKey] = entry
        modelMemory = store
    }
}
