@testable import LocalBar
import Foundation

/// Configurable stub driver for unit tests. All network/process calls are no-ops.
struct MockServerDriver: ServerDriver {

    var serverType: ServerType
    var modelSwitchBehavior: ModelSwitchBehavior
    var modelListRequiresRunningServer: Bool

    // Configurable responses
    var stubbedModels: [ModelRef]
    var stubbedHealth: HealthStatus
    var stubbedValidationIssues: [ConfigIssue]
    /// Tag returned by createManagedModel — nil means the call was not made or failed.
    var createdManagedTag: String?
    /// Records calls to createManagedModel for assertion in tests.
    var createManagedModelCallCount: Int = 0

    var paramSchema: [ParamDescriptor]
    var flagSchema: [FlagDescriptor]

    init(
        serverType: ServerType = .ollama,
        modelSwitchBehavior: ModelSwitchBehavior = .apiCall,
        paramSchema: [ParamDescriptor] = [],
        stubbedModels: [ModelRef] = [],
        stubbedHealth: HealthStatus = .healthy,
        stubbedValidationIssues: [ConfigIssue] = []
    ) {
        self.serverType = serverType
        self.modelSwitchBehavior = modelSwitchBehavior
        self.modelListRequiresRunningServer = false
        self.paramSchema = paramSchema
        self.flagSchema = []
        self.stubbedModels = stubbedModels
        self.stubbedHealth = stubbedHealth
        self.stubbedValidationIssues = stubbedValidationIssues
    }

    func validate(config: ServerInstanceConfig) async -> [ConfigIssue] {
        stubbedValidationIssues
    }

    func makeLaunchPlan(config: ServerInstanceConfig, model: ModelRef?, params: ParamValues) throws -> LaunchPlan {
        LaunchPlan(
            executableURL: URL(fileURLWithPath: "/bin/echo"),
            arguments: [],
            environment: [:],
            workingDirectory: nil
        )
    }

    func makeShutdownPlan(config: ServerInstanceConfig) -> ShutdownPlan {
        ShutdownPlan(gracefulRequest: nil, gracePeriod: 0)
    }

    func healthCheck(config: ServerInstanceConfig) async -> HealthStatus {
        stubbedHealth
    }

    func listModels(config: ServerInstanceConfig) async throws -> [ModelRef] {
        stubbedModels
    }

    func switchModel(to model: ModelRef, params: ParamValues, config: ServerInstanceConfig) async throws {}

    func contextUsage(config: ServerInstanceConfig) async throws -> ContextUsage? { nil }

    // Override managed model creation to track calls without spawning processes.
    func managedModelTag(for modelKey: String, instanceId: String) -> String? {
        "mock/\(modelKey)-\(instanceId.prefix(8))"
    }

    mutating func createManagedModel(
        baseTag: String,
        instanceId: String,
        params: ParamValues,
        executablePath: String
    ) async -> String? {
        createManagedModelCallCount += 1
        createdManagedTag = managedModelTag(for: baseTag, instanceId: instanceId)
        return createdManagedTag
    }
}
