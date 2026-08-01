import Foundation
import Observation

/// Owns the set of ServerInstanceControllers, provides the aggregate icon state,
/// and mediates persistence.
@Observable @MainActor
final class InstanceRegistry {

    private(set) var controllers: [ServerInstanceController] = []
    private(set) var profiles: [NamedProfile] = []
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
            // Try both /health (mlx-lm) and / (Ollama) so either server type is detected.
            let probes = [
                "http://127.0.0.1:\(port)/health",
                "http://127.0.0.1:\(port)/",
            ]
            for urlString in probes {
                guard let url = URL(string: urlString) else { continue }
                if let (_, resp) = try? await URLSession(configuration: .ephemeral).data(from: url),
                   (resp as? HTTPURLResponse)?.statusCode == 200 {
                    return true
                }
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
            profiles = (try? await persistence.loadProfiles()) ?? []
            let configs = try await persistence.loadInstances()
            for config in configs {
                let controller = makeController(config: config)
                controllers.append(controller)
                // Only fire-and-forget start() when wasRunningWhenQuit is false.
                // If wasRunningWhenQuit is true, adoptRunningServers() (called right
                // after bootstrap) will reconnect silently instead — avoids a race
                // between start() and adoptIfRunning() on the same controller.
                if config.startOnAppLaunch && !config.wasRunningWhenQuit {
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

    // MARK: Managing profiles

    func addProfile(_ profile: NamedProfile) {
        profiles.append(profile)
        persistProfiles()
    }

    func updateProfile(_ profile: NamedProfile) {
        guard let idx = profiles.firstIndex(where: { $0.id == profile.id }) else { return }
        profiles[idx] = profile
        persistProfiles()
    }

    func removeProfile(id: UUID) {
        profiles.removeAll { $0.id == id }
        // Clear the profile from any instances that had it active.
        for controller in controllers where controller.config.activeProfileID == id {
            var cfg = controller.config
            cfg.activeProfileID = nil
            controller.updateConfig(cfg)
        }
        persistProfiles()
    }

    func profile(for id: UUID?) -> NamedProfile? {
        guard let id else { return nil }
        return profiles.first { $0.id == id }
    }

    private func persistProfiles() {
        let p = profiles
        Task { try? await persistence.saveProfiles(p) }
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
        controller.onModelSwitchTiming = { [weak self] modelKey, duration in
            self?.recordModelSwitchTiming(modelKey: modelKey, duration: duration)
        }
        controller.activeProfileProvider = { [weak self, weak controller] in
            self?.profile(for: controller?.config.activeProfileID)
        }
        return controller
    }

    /// Persist a restart-duration sample for EWMA adaptive timing estimates.
    private func recordModelSwitchTiming(modelKey: String, duration: TimeInterval) {
        Task {
            var store = (try? await persistence.loadModelMemory()) ?? [:]
            var entry = store[modelKey] ?? ModelMemory(
                modelKey: modelKey,
                lastUsedParams: ParamValues(),
                lastUsedAt: Date()
            )
            entry.recordRestartDuration(duration)
            try? await persistence.upsertModelMemory(entry, into: &store)
        }
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

    // MARK: Adopt as new instance

    /// Called when the user clicks "Adopt" on a port-conflict error.
    /// Creates a fresh persistent instance to track the external server and
    /// reverts the errored controller to .stopped — preserving the user's
    /// original config (name, model selection, params) completely untouched.
    func adoptExternalAsNewInstance(from erroredController: ServerInstanceController) async {
        guard case .error(let err) = erroredController.phase,
              case .portConflict(let port, _) = err.kind else { return }

        let base = erroredController.config
        var adoptConfig = ServerInstanceConfig(
            name: "External @ \(port)",
            type: base.type,
            port: base.port,
            executablePath: base.executablePath
        )
        adoptConfig.host = base.host
        adoptConfig.modelSearchPaths = base.modelSearchPaths

        let adoptController = makeController(config: adoptConfig)
        controllers.append(adoptController)

        // Connect the new controller to the already-running server.
        await adoptController.adoptIfRunning()

        // Persist the detected model key so the instance can be restarted later
        // without losing the model selection.
        if let detectedKey = adoptController.currentModel?.key {
            var updated = adoptController.config
            updated.selectedModelKey = detectedKey
            adoptController.updateConfig(updated)
        }

        // Leave the original controller in .stopped — the user's config is intact.
        erroredController.revertToStopped()

        persistInstances()
    }

    // MARK: App lifecycle

    /// Stops all managed servers. Called by the "Stop All" button in the menu bar.
    func stopAll() async {
        await withTaskGroup(of: Void.self) { group in
            for controller in controllers {
                group.addTask { await controller.stop() }
            }
        }
    }

    // MARK: Adopt already-running servers

    /// At app launch, reconnect to servers that are already healthy on their
    /// configured port for two categories of instance:
    ///   • `startOnAppLaunch` — the user wants LocalBar to manage these automatically.
    ///   • `wasRunningWhenQuit` — the server was running when LocalBar last quit
    ///     (quit-without-stop), so reconnecting silently is the right default.
    ///
    /// All other instances are left stopped. If something external happens to be
    /// on their port, the user will see a portConflict error when they explicitly
    /// click Start — giving them the choice to Adopt or change the port.
    func adoptRunningServers() async {
        await withTaskGroup(of: Void.self) { group in
            for controller in controllers
                where controller.config.startOnAppLaunch || controller.config.wasRunningWhenQuit {
                group.addTask { await controller.adoptIfRunning() }
            }
        }
    }

}
