import Foundation
import Observation

/// Owns the set of ServerInstanceControllers, provides the aggregate icon state,
/// and mediates persistence.
@Observable @MainActor
final class InstanceRegistry {

    private(set) var controllers: [ServerInstanceController] = []
    private let persistence = PersistenceService()

    // MARK: Derived state

    var aggregateIconState: IconState {
        controllers.map(\.phase).aggregateIconState
    }

    var hasAnyRunning: Bool {
        controllers.contains(where: { $0.phase.isRunning })
    }

    /// True if any inference server is running — LocalBar-managed OR external
    /// on a common default port. `excludingPort` is the port the caller is
    /// about to start on, so it isn't counted against itself.
    /// Performs async health checks on well-known ports not already managed.
    func hasAnyRunningIncludingExternal(excludingPort: Int) async -> Bool {
        if controllers.contains(where: { $0.phase.isRunning }) { return true }
        let managedPorts = Set(controllers.map(\.config.port))
        let commonPorts = [8080, 8081, 11434]
        for port in commonPorts {
            guard port != excludingPort, !managedPorts.contains(port) else { continue }
            guard let url = URL(string: "http://127.0.0.1:\(port)/health") else { continue }
            if let (_, resp) = try? await URLSession(configuration: .ephemeral).data(from: url),
               (resp as? HTTPURLResponse)?.statusCode == 200 {
                return true
            }
        }
        return false
    }

    // MARK: Bootstrap

    /// Load persisted instances from disk. Call once at app launch before
    /// adoptRunningServers(). Non-fatal — swallows errors and starts with
    /// an empty registry if the file is missing or corrupt.
    func bootstrap() async {
        do {
            try await persistence.ensureDirectoryExists()
            let configs = try await persistence.loadInstances()
            for config in configs {
                let controller = makeController(config: config)
                controllers.append(controller)
                if config.startOnAppLaunch {
                    Task { await controller.start() }
                }
            }
        } catch {
            // Non-fatal: missing file or decode failure starts with empty state.
            // Errors will be visible on next save attempt.
        }
    }

    // MARK: Managing instances

    func addInstance(config: ServerInstanceConfig, initialModels: [ModelRef] = []) {
        let controller = makeController(config: config)
        if !initialModels.isEmpty {
            controller.seedModels(initialModels)
        }
        controllers.append(controller)
        persistInstances()

        // Do NOT auto-adopt here. The user explicitly added this entry with a
        // specific model selection; showing it as "running" immediately (by
        // connecting to whatever happens to be on that port) is confusing.
        // Adoption happens via: (a) the user clicking Start (performStart does a
        // pre-flight health check and adopts if the port is already healthy),
        // or (b) adoptRunningServers() on Settings open.
        if config.startOnAppLaunch {
            Task { await controller.start() }
        }
    }

    func removeInstance(id: UUID) {
        guard let controller = controllers.first(where: { $0.id == id }) else { return }
        Task { @MainActor in
            await controller.stop()
            await controller.cleanupManagedResources()
            self.controllers.removeAll { $0.id == id }
            self.persistInstances()
        }
    }

    // MARK: Private helpers

    /// Create a controller and wire the persistence callback so any config
    /// mutation (model selection, managedModelTag update, etc.) auto-saves.
    private func makeController(config: ServerInstanceConfig) -> ServerInstanceController {
        let controller = ServerInstanceController(config: config)
        controller.onConfigChanged = { [weak self] _ in
            self?.persistInstances()
        }
        return controller
    }

    /// Fire-and-forget save. Errors are silently dropped — UI never shows a
    /// "save failed" alert since instance state is always recoverable by
    /// re-adding the instance.
    private func persistInstances() {
        let configs = controllers.map(\.config)
        Task { try? await persistence.saveInstances(configs) }
    }

    func controller(for id: UUID) -> ServerInstanceController? {
        controllers.first(where: { $0.id == id })
    }

    // MARK: App lifecycle

    /// Called from applicationWillTerminate — stops all managed servers.
    func stopAll() async {
        await withTaskGroup(of: Void.self) { group in
            for controller in controllers {
                group.addTask { await controller.stop() }
            }
        }
    }

    // MARK: Adopt already-running servers

    /// Check every stopped instance to see if a server is already running on
    /// its port (e.g. started by launchd, a shell script, or a previous
    /// LocalBar session). Runs concurrently across all instances.
    func adoptRunningServers() async {
        await withTaskGroup(of: Void.self) { group in
            for controller in controllers {
                group.addTask { await controller.adoptIfRunning() }
            }
        }
    }

    // MARK: Auto-start

    func startAutoLaunchInstances() async {
        for controller in controllers where controller.config.startOnAppLaunch {
            await controller.start()
        }
    }
}
