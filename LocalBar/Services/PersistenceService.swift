import Foundation

/// The seam between InstanceRegistry and its storage backend.
/// `PersistenceService` is the disk adapter; tests substitute an in-memory adapter.
protocol PersistenceServiceProtocol: Sendable {
    func ensureDirectoryExists() async throws
    func loadInstances() async throws -> [ServerInstanceConfig]
    func saveInstances(_ instances: [ServerInstanceConfig]) async throws
    func loadProfiles() async throws -> [NamedProfile]
    func saveProfiles(_ profiles: [NamedProfile]) async throws
    func loadModelMemory() async throws -> [String: ModelMemory]
    func saveModelMemory(_ memory: [String: ModelMemory]) async throws
    func upsertModelMemory(_ entry: ModelMemory, into store: inout [String: ModelMemory]) async throws
}

/// Serialized actor that owns all disk I/O for LocalBar's data stores.
/// Separate files per domain — corrupt file loses only its own domain.
actor PersistenceService: PersistenceServiceProtocol {

    // MARK: Paths

    private let baseURL: URL = {
        let appSupport = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        return appSupport.appendingPathComponent("LocalBar", isDirectory: true)
    }()

    private var instancesURL: URL { baseURL.appendingPathComponent("instances.json") }
    private var profilesURL:  URL { baseURL.appendingPathComponent("profiles.json") }
    private var memoryURL:    URL { baseURL.appendingPathComponent("model-memory.json") }
    private let encoder: JSONEncoder = {
        let e = JSONEncoder()
        e.outputFormatting = [.prettyPrinted, .sortedKeys]
        e.dateEncodingStrategy = .iso8601
        return e
    }()

    private let decoder: JSONDecoder = {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return d
    }()

    // MARK: Bootstrap

    func ensureDirectoryExists() throws {
        try FileManager.default.createDirectory(at: baseURL, withIntermediateDirectories: true)
    }

    // MARK: Instances

    func loadInstances() throws -> [ServerInstanceConfig] {
        guard FileManager.default.fileExists(atPath: instancesURL.path) else { return [] }
        let data = try Data(contentsOf: instancesURL)
        return try decoder.decode([ServerInstanceConfig].self, from: data)
    }

    func saveInstances(_ instances: [ServerInstanceConfig]) throws {
        let data = try encoder.encode(instances)
        try data.write(to: instancesURL, options: .atomic)
    }

    // MARK: Profiles

    func loadProfiles() throws -> [NamedProfile] {
        guard FileManager.default.fileExists(atPath: profilesURL.path) else { return [] }
        let data = try Data(contentsOf: profilesURL)
        return try decoder.decode([NamedProfile].self, from: data)
    }

    func saveProfiles(_ profiles: [NamedProfile]) throws {
        let data = try encoder.encode(profiles)
        try data.write(to: profilesURL, options: .atomic)
    }

    // MARK: Model memory

    func loadModelMemory() throws -> [String: ModelMemory] {
        guard FileManager.default.fileExists(atPath: memoryURL.path) else { return [:] }
        let data = try Data(contentsOf: memoryURL)
        let array = try decoder.decode([ModelMemory].self, from: data)
        return Dictionary(uniqueKeysWithValues: array.map { ($0.modelKey, $0) })
    }

    func saveModelMemory(_ memory: [String: ModelMemory]) throws {
        let array = Array(memory.values)
        let data = try encoder.encode(array)
        try data.write(to: memoryURL, options: .atomic)
    }

    func upsertModelMemory(_ entry: ModelMemory, into store: inout [String: ModelMemory]) throws {
        store[entry.modelKey] = entry
        try saveModelMemory(store)
    }

}
