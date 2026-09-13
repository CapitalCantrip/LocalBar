import Foundation
import Observation
import UserNotifications

/// Owns the state machine, Process handle, and lifecycle for one server instance.
/// All state (phase, process, model) lives here — never in drivers or config.
@Observable @MainActor
final class ServerInstanceController: Identifiable {

    // MARK: Identity

    let id: UUID

    // MARK: Observed state

    private(set) var config: ServerInstanceConfig
    private(set) var phase: InstancePhase = .stopped(.neverStarted)
    private(set) var currentModel: ModelRef?
    private(set) var availableModels: [ModelRef] = []
    private(set) var contextUsage: ContextUsage?
    private(set) var lastError: InstanceError?
    /// Non-nil during .switchingModel — used to show ETA countdown in UI.
    private(set) var switchStartedAt: Date?

    // MARK: Private

    private let driver: any ServerDriver
    private var process: Process?
    /// Called by InstanceRegistry after any config mutation so the registry can
    /// persist the updated config list. Set once at creation time.
    var onConfigChanged: ((ServerInstanceConfig) -> Void)?
    /// Called after a successful restart-required model switch with the model key
    /// and measured duration. Registry uses this to persist EWMA timing samples.
    var onModelSwitchTiming: ((String, TimeInterval) -> Void)?
    /// Returns the active NamedProfile for this instance, or nil if none is set.
    /// Set by InstanceRegistry at controller creation time.
    var activeProfileProvider: (() -> NamedProfile?)? = nil
    /// PID of an externally-managed server that LocalBar adopted rather than spawned.
    /// Used to send shutdown signals when `process` is nil.
    private var adoptedPID: pid_t?
    private var healthPollTask: Task<Void, Never>?
    private var startupTimeoutTask: Task<Void, Never>?
    private var contextPollTask: Task<Void, Never>?
    /// Last captured stderr from the child process (tail, for error reporting).
    /// Written once by a `Task.detached` after the process exits, read on
    /// MainActor in `handleUnexpectedTermination`. No actual data race —
    /// the write always precedes the read because both are triggered by
    /// process termination (pipe closes first, then terminationHandler fires).
    nonisolated(unsafe) private var capturedStderr = ""

    private var startupTimeout: TimeInterval {
        if let v = config.advancedFlags["startupTimeoutSeconds"], case .int(let s) = v {
            return TimeInterval(s)
        }
        return 120 // D5: 120 s default
    }

    // MARK: Init

    init(config: ServerInstanceConfig) {
        self.id = config.id
        self.config = config
        self.driver = DriverRegistry.driver(for: config.type)
    }

    // MARK: Public API

    func start() async {
        guard case .stopped = phase else { return }
        await performStart()
    }

    func stop() async {
        guard phase.isRunning || phase == .starting else { return }
        await performStop(reason: .userStopped)
    }

    func restart() async {
        await stop()
        await start()
    }

    func switchModel(to model: ModelRef) async {
        guard phase.isRunning else { return }

        switch driver.modelSwitchBehavior {
        case .apiCall:
            await performAPIModelSwitch(to: model)
        case .restartRequired:
            await performRestartModelSwitch(to: model)
        }
    }

    func retryFromError() async {
        guard case .error = phase else { return }
        lastError = nil
        await performStart()
    }

    /// D4: one-click rollback shown in error state after a failed restart-required switch.
    func rollbackToPreviousModel() async {
        guard case .error(let err) = phase,
              let previousKey = err.previousModelKey else { return }
        config.selectedModelKey = previousKey
        lastError = nil
        await performStart()
    }

    /// Check whether a server is already running on this instance's port
    /// and, if so, adopt it without spawning a new process. Called at app
    /// launch so externally-started servers (launchd, scripts, etc.) are
    /// surfaced in LocalBar automatically, and by adoptExternalAsNewInstance
    /// when the user explicitly clicks Adopt on a port-conflict error.
    ///
    /// Falls back to lsof when the health endpoint is unreachable — this
    /// handles servers that were orphaned mid-startup or that don't expose
    /// the exact health path the driver probes (e.g. an mlx-lm server still
    /// loading its model weights). beginHealthPoll() will confirm health on
    /// the next cycle regardless.
    func adoptIfRunning() async {
        guard case .stopped = phase else { return }

        let health = await driver.healthCheck(config: config)
        let pid: pid_t?

        if health == .healthy {
            pid = await findListeningPID(port: config.port)
        } else {
            // Health endpoint unresponsive — confirm something is actually
            // listening via lsof before proceeding.
            guard let found = await findListeningPID(port: config.port), found > 0 else {
                // Nothing on the port — clear stale wasRunningWhenQuit flag.
                if config.wasRunningWhenQuit {
                    config.wasRunningWhenQuit = false
                    onConfigChanged?(config)
                }
                return
            }
            pid = found
        }

        adoptedPID = pid
        await refreshModels()
        // Try to identify what model is actually loaded by querying /v1/models.
        // This matters for externally-started servers where the loaded model
        // may differ from config.selectedModelKey.
        let detected = await detectRunningModel()
        currentModel = detected != nil ? detected : await resolvedModel()
        transition(to: .running)
        beginHealthPoll()
        beginContextPoll()
    }

    func refreshModels() async {
        do {
            availableModels = try await driver.listModels(config: config)
        } catch {
            // Non-fatal — stale model list stays visible.
        }
    }

    /// Estimate the memory footprint of starting this instance with its current
    /// model and params. For Ollama, may make a short /api/show request if the
    /// model's architecture fields are not yet populated.
    func estimatedMemoryFootprint() async -> MemoryFootprintEstimate {
        guard let model = currentModel ?? availableModels.first(where: { $0.key == config.selectedModelKey }) else {
            return MemoryFootprintEstimate()
        }

        var metadata = model.metadata ?? ModelMetadata()

        // For Ollama: fetch architecture fields from /api/show if missing.
        if config.type == .ollama,
           metadata.numHiddenLayers == nil,
           let ollamaDriver = driver as? OllamaDriver {
            if let fetched = await ollamaDriver.fetchModelInfo(modelKey: model.key, config: config) {
                metadata.numHiddenLayers = fetched.numHiddenLayers
                metadata.numKVHeads      = fetched.numKVHeads
                metadata.headDim         = fetched.headDim
            }
        }

        let params = resolvedParams()

        // Context length: active param value, or driver default (2048 for Ollama, nil for mlx-lm).
        let contextLength: Int
        if let v = params[.contextLength], case .int(let n) = v {
            contextLength = n
        } else {
            contextLength = config.type == .ollama ? 2048 : 4096
        }

        // KV cache bits: mlx-lm --kv-cache-bits flag; Ollama always bf16.
        let kvCacheBits: Int
        if config.type == .mlxLM,
           let v = config.advancedFlags["--kv-cache-bits"],
           case .int(let bits) = v {
            kvCacheBits = bits
        } else {
            kvCacheBits = 16
        }

        return MemoryFootprintEstimate(
            weightBytes:  metadata.estimatedWeightBytes(directSizeBytes: model.sizeBytes),
            kvCacheBytes: metadata.estimatedKVCacheBytes(contextLength: contextLength, kvCacheBits: kvCacheBits)
        )
    }

    func updateConfig(_ newConfig: ServerInstanceConfig) {
        let modelKeyChanged = newConfig.selectedModelKey != config.selectedModelKey
        let ollamaParamsChanged = newConfig.type == .ollama
            && newConfig.instanceParams != config.instanceParams
        config = newConfig
        onConfigChanged?(config)

        // For Ollama: regenerate the managed Modelfile when the model selection,
        // any sampling param, or the active profile changes (profile changes alter
        // the resolved params even when instanceParams hasn't changed).
        let profileChanged = newConfig.activeProfileID != config.activeProfileID
        if modelKeyChanged || ollamaParamsChanged || (newConfig.type == .ollama && profileChanged) {
            Task { await ensureManagedModelIfNeeded() }
        }
    }

    /// Remove the LocalBar-managed Ollama model tag (if any). Called before
    /// the instance is removed from the registry so we don't leave orphaned
    /// `localbar/` entries in `ollama list`.
    func cleanupManagedResources() async {
        guard config.type == .ollama, let tag = config.managedModelTag else { return }
        await driver.removeManagedModel(tag: tag, executablePath: config.executablePath)
        config.managedModelTag = nil
        onConfigChanged?(config)
    }

    /// Called by the registry when the user chooses "Adopt" on a port-conflict error
    /// and we create a new instance instead. Resets this controller back to .stopped
    /// so the user's configured instance (name, model key, params) is untouched.
    func revertToStopped() {
        cancelPolling()
        adoptedPID = nil
        transition(to: .stopped(.neverStarted))
    }

    /// Pre-populate the model list from an already-completed scan (e.g. at add-time),
    /// so metadata columns fill immediately without waiting for a background refresh.
    func seedModels(_ models: [ModelRef]) {
        availableModels = models
    }

    // MARK: Private — start

    private func performStart() async {
        transition(to: .starting)

        if let err = await preFlightChecks() {
            transition(to: .error(err))
            return
        }

        let params = resolvedParams()
        let model: ModelRef? = await resolvedModel()

        let plan: LaunchPlan
        do {
            plan = try driver.makeLaunchPlan(config: config, model: model, params: params)
        } catch {
            transition(to: .error(InstanceError(kind: .launchFailed, message: error.localizedDescription)))
            return
        }

        do {
            process = try spawnProcess(plan: plan)
        } catch {
            transition(to: .error(InstanceError(kind: .launchFailed, message: "Failed to spawn process: \(error.localizedDescription)")))
            return
        }

        currentModel = model
        let timeout = startupTimeout
        startupTimeoutTask = Task {
            try? await Task.sleep(for: .seconds(timeout))
            guard !Task.isCancelled else { return }
            await self.handleStartupTimeout()
        }
        await awaitHealthy()
    }

    /// Run pre-flight checks before spawning: port-conflict detection and config validation.
    /// Returns a ready-to-use InstanceError on failure, nil on success.
    private func preFlightChecks() async -> InstanceError? {
        // If a server is already healthy on this port, surface the conflict
        // explicitly so the user can choose to Adopt rather than spawn blindly.
        if await driver.healthCheck(config: config) == .healthy {
            let occupant = await detectOccupantModelName() ?? "an external server"
            return InstanceError(kind: .portConflict(port: config.port, occupiedBy: occupant),
                message: "Port \(config.port) is already in use by \(occupant). Adopt the running server or stop it and retry.")
        }

        // Config validation — abort on any blocker.
        let issues = await driver.validate(config: config)
        if let blocker = issues.first(where: { $0.severity == .blocker }) {
            return InstanceError(kind: .launchFailed, message: blocker.message ?? "Validation failed.")
        }

        // lsof check — catches orphaned / still-starting / unresponsive servers
        // that failed the health probe above but are still holding the port.
        if let pid = await findListeningPID(port: config.port), pid > 0 {
            let occupant = await detectOccupantModelName() ?? "PID \(pid)"
            return InstanceError(kind: .portConflict(port: config.port, occupiedBy: occupant),
                message: "Port \(config.port) is already in use by \(occupant). Adopt the running server or change the port.")
        }

        return nil
    }

    /// Configure and launch the server process for `plan`. Sets up stderr capture
    /// and the termination handler. Clears any prior adoption state.
    private func spawnProcess(plan: LaunchPlan) throws -> Process {
        let proc = Process()
        proc.executableURL = plan.executableURL
        proc.arguments = plan.arguments
        proc.environment = mergedEnvironment(plan.environment)
        if let cwd = plan.workingDirectory { proc.currentDirectoryURL = cwd }

        // Fresh spawn — clear any previous adoption state and stderr buffer.
        adoptedPID = nil
        capturedStderr = ""
        let stderrPipe = Pipe()
        proc.standardError = stderrPipe
        Task.detached { [weak self] in
            let data = stderrPipe.fileHandleForReading.readDataToEndOfFile()
            let text = String(data: data, encoding: .utf8) ?? ""
            // Keep the last 800 chars so we don't OOM on chatty servers.
            self?.capturedStderr = text.count > 800 ? String(text.suffix(800)) : text
        }
        proc.terminationHandler = { [weak self] process in
            Task { @MainActor [weak self] in
                self?.handleUnexpectedTermination(exitCode: process.terminationStatus)
            }
        }
        try proc.run()
        return proc
    }

    private func awaitHealthy() async {
        while true {
            guard phase == .starting else { return }
            let status = await driver.healthCheck(config: config)
            // Re-check phase: the startup timeout task may have fired while the
            // health check was in-flight, killing the process and transitioning
            // to .error. Without this guard we'd incorrectly override .error with
            // .running using a stale result from the now-dead process.
            guard phase == .starting else { return }
            if status == .healthy {
                startupTimeoutTask?.cancel()
                startupTimeoutTask = nil
                transition(to: .running)
                beginHealthPoll()
                beginContextPoll()
                await refreshModels()
                await notify(title: "LocalBar", body: "\(config.name) is running")
                return
            }
            try? await Task.sleep(for: .seconds(2))
        }
    }

    // MARK: Private — stop

    private func performStop(reason: InstancePhase.StopReason) async {
        transition(to: .stopping)
        cancelPolling()

        guard let proc = process else {
            // No spawned process — server was adopted from external management.
            await stopAdoptedProcess()
            adoptedPID = nil
            transition(to: .stopped(reason))
            return
        }

        let plan = driver.makeShutdownPlan(config: config)
        await sendGracefulShutdown(plan.gracefulRequest, to: proc)
        if await waitForProcessExit(proc: proc, gracePeriod: plan.gracePeriod) {
            process = nil
            transition(to: .stopped(reason))
            if reason == .userStopped {
                await notify(title: "LocalBar", body: "\(config.name) stopped")
            }
        } else {
            process = nil
            transition(to: .error(InstanceError(kind: .shutdownTimedOut, message: "Server did not stop after SIGKILL. PID: \(proc.processIdentifier)")))
        }
    }

    /// Signal or HTTP-request graceful shutdown to a spawned process.
    private func sendGracefulShutdown(_ graceful: ShutdownPlan.GracefulShutdown?, to proc: Process) async {
        switch graceful {
        case .none: break
        case .signal(let sig):
            kill(proc.processIdentifier, sig)
        case .httpRequest(let path, let method):
            let url = URL(string: "http://\(config.host):\(config.port)\(path)")!
            var req = URLRequest(url: url)
            req.httpMethod = method
            _ = try? await URLSession(configuration: .ephemeral).data(for: req)
        }
    }

    /// Wait grace period, escalate to SIGTERM, then SIGKILL.
    /// Returns true if the process exited, false if SIGKILL also timed out.
    private func waitForProcessExit(proc: Process, gracePeriod: TimeInterval) async -> Bool {
        let deadline = Date().addingTimeInterval(gracePeriod)
        while proc.isRunning, Date() < deadline {
            try? await Task.sleep(for: .milliseconds(200))
        }
        if proc.isRunning {
            proc.terminate()
            let sigtermDeadline = Date().addingTimeInterval(gracePeriod * 2)
            while proc.isRunning, Date() < sigtermDeadline {
                try? await Task.sleep(for: .milliseconds(200))
            }
        }
        if proc.isRunning {
            kill(proc.processIdentifier, SIGKILL)
            try? await Task.sleep(for: .seconds(1))
        }
        return !proc.isRunning
    }

    /// Stop an externally-adopted server by PID (no spawned Process handle).
    private func stopAdoptedProcess() async {
        let pid: pid_t?
        if let known = adoptedPID { pid = known } else { pid = await findListeningPID(port: config.port) }
        guard let pid, pid > 0 else { return }
        let plan = driver.makeShutdownPlan(config: config)
        if case .signal(let sig) = plan.gracefulRequest { kill(pid, sig) }
        let deadline = Date().addingTimeInterval(plan.gracePeriod)
        while isExternalProcessRunning(pid: pid), Date() < deadline {
            try? await Task.sleep(for: .milliseconds(200))
        }
        if isExternalProcessRunning(pid: pid) { kill(pid, SIGTERM) }
        try? await Task.sleep(for: .milliseconds(500))
        if isExternalProcessRunning(pid: pid) { kill(pid, SIGKILL) }
    }

    // MARK: Private — managed model

    /// For Ollama: create/update the managed Modelfile for the currently
    /// selected model. Updates `config.managedModelTag` and persists.
    /// No-op for mlx-lm (driver returns nil).
    private func ensureManagedModelIfNeeded() async {
        guard let key = config.selectedModelKey else { return }
        let params = resolvedParams()
        let instanceId = config.id.uuidString
        if let tag = await driver.createManagedModel(
            baseTag: key,
            instanceId: instanceId,
            params: params,
            executablePath: config.executablePath
        ) {
            config.managedModelTag = tag
            onConfigChanged?(config)
        }
    }

    // MARK: Private — model switch

    private func performAPIModelSwitch(to model: ModelRef) async {
        // Set the key first so ensureManagedModelIfNeeded builds the Modelfile
        // for the *new* model, not the one that was previously selected.
        config.selectedModelKey = model.key
        await ensureManagedModelIfNeeded()

        let params = resolvedParams()
        do {
            try await driver.switchModel(to: model, params: params, config: config)
            currentModel = model
            onConfigChanged?(config)
        } catch {
            // Stay in .running, surface a non-fatal error banner (handled by UI observing lastError).
            lastError = InstanceError(kind: .launchFailed, message: "Model switch failed: \(error.localizedDescription)")
        }
    }

    private func performRestartModelSwitch(to model: ModelRef) async {
        let previousKey = currentModel?.key
        let attemptedKey = model.key

        switchStartedAt = Date()
        transition(to: .switchingModel)
        cancelPolling()

        await performStop(reason: .exitedCleanly)

        config.selectedModelKey = model.key
        await performStart()

        if phase.isRunning {
            if let start = switchStartedAt {
                let duration = Date().timeIntervalSince(start)
                onModelSwitchTiming?(model.key, duration)
            }
            switchStartedAt = nil
        } else if case .error(var err) = phase {
            // D4: attach rollback keys so UI can show "Restart with previous model".
            err = InstanceError(kind: err.kind, message: err.message, lastAttemptedModelKey: attemptedKey, previousModelKey: previousKey)
            transition(to: .error(err))
            switchStartedAt = nil
        }
    }

    // MARK: Private — health polling

    private func beginHealthPoll() {
        healthPollTask = Task {
            var consecutiveFailures = 0
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(5))
                guard !Task.isCancelled, phase.isRunning else { break }

                let status = await driver.healthCheck(config: config)
                if status == .healthy {
                    consecutiveFailures = 0
                } else {
                    consecutiveFailures += 1
                    if consecutiveFailures >= 3 {
                        await handleHealthCheckFailure()
                        break
                    }
                }
            }
        }
    }

    private func handleHealthCheckFailure() async {
        cancelPolling()
        await performStop(reason: .exitedCleanly)
        transition(to: .error(InstanceError(kind: .healthCheckFailed, message: "\(config.name) stopped responding to health checks.")))
        await notify(title: "LocalBar — Error", body: "\(config.name) became unresponsive.")
    }

    private func handleUnexpectedTermination(exitCode: Int32) {
        // A graceful stop we initiated — performStop handles the transition.
        if case .stopping = phase { return }
        startupTimeoutTask?.cancel()
        startupTimeoutTask = nil
        cancelPolling()
        process = nil
        // Include the last stderr lines so the user knows WHY it crashed
        // (e.g. "No module named mlx_lm", "Model not found", etc.)
        let stderrHint = capturedStderr
            .split(separator: "\n")
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { !$0.isEmpty }
            .suffix(4)
            .joined(separator: "\n")
        let detail = stderrHint.isEmpty ? "" : "\n\n\(stderrHint)"
        let err = InstanceError(kind: .crashed(exitCode: exitCode), message: "\(config.name) exited (code \(exitCode)).\(detail)")
        transition(to: .error(err))
        Task {
            await notify(title: "LocalBar — Crashed", body: "\(config.name) crashed (exit \(exitCode)).")
        }
    }

    private func handleStartupTimeout() async {
        guard phase == .starting else { return }
        let timeout = startupTimeout
        cancelPolling()
        await performStop(reason: .exitedCleanly)
        transition(to: .error(InstanceError(kind: .healthCheckFailed, message: "Server did not become healthy within \(Int(timeout)) s. If loading a large model, increase the startup timeout in advanced settings.")))
    }

    // MARK: Private — context polling

    private func beginContextPoll() {
        contextPollTask = Task {
            while !Task.isCancelled, phase.isRunning {
                if let usage = try? await driver.contextUsage(config: config) {
                    contextUsage = usage
                }
                try? await Task.sleep(for: .seconds(30))
            }
        }
    }

    // MARK: Private — helpers

    private func cancelPolling() {
        healthPollTask?.cancel()
        healthPollTask = nil
        startupTimeoutTask?.cancel()
        startupTimeoutTask = nil
        contextPollTask?.cancel()
        contextPollTask = nil
    }

    private func transition(to newPhase: InstancePhase) {
        phase = newPhase
        if case .error(let err) = newPhase { lastError = err }

        // Keep wasRunningWhenQuit in sync so a restart can reconnect orphans.
        switch newPhase {
        case .running:
            if !config.wasRunningWhenQuit {
                config.wasRunningWhenQuit = true
                onConfigChanged?(config)
            }
        case .stopped, .error:
            if config.wasRunningWhenQuit {
                config.wasRunningWhenQuit = false
                onConfigChanged?(config)
            }
        default: break
        }
    }

    private func resolvedParams() -> ParamValues {
        // Resolution order: driver defaults < instance overrides < active profile (highest).
        var resolved = ParamValues()
        for descriptor in driver.paramSchema {
            if let def = descriptor.defaultValue { resolved.values[descriptor.param] = def }
        }
        resolved.merge(from: config.instanceParams)
        if let profile = activeProfileProvider?() { resolved.merge(from: profile.params) }
        return resolved
    }

    /// The fully-resolved effective params — useful for read-only display in UI
    /// (shows what will actually be sent at launch/switch, including defaults).
    func effectiveParams() -> ParamValues { resolvedParams() }

    private func resolvedModel() async -> ModelRef? {
        guard let key = config.selectedModelKey else { return nil }
        if availableModels.isEmpty { await refreshModels() }
        return availableModels.first(where: { $0.key == key })
    }

    /// Returns a human-readable name for whatever model is currently loaded on
    /// this port, without needing availableModels to be populated. Used to
    /// populate the port-conflict error message before a full model scan.
    private func detectOccupantModelName() async -> String? {
        guard let url = URL(string: "http://\(config.host):\(config.port)/v1/models") else { return nil }
        do {
            let (data, _) = try await URLSession(configuration: .ephemeral).data(from: url)
            guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let list = json["data"] as? [[String: Any]],
                  let firstID = list.first?["id"] as? String else { return nil }
            return URL(fileURLWithPath: firstID).lastPathComponent
        } catch { return nil }
    }

    /// Query the running server's /v1/models endpoint to find what model is
    /// actually loaded, then match it against our scanned availableModels list.
    /// The model ID returned by mlx-lm is the filesystem path passed at launch.
    private func detectRunningModel() async -> ModelRef? {
        guard let url = URL(string: "http://\(config.host):\(config.port)/v1/models") else { return nil }
        do {
            let (data, _) = try await URLSession(configuration: .ephemeral).data(from: url)
            // Response shape: {"object":"list","data":[{"id":"<path>","object":"model",...}]}
            guard let json = try JSONSerialization.jsonObject(with: data) as? [String: Any],
                  let list = json["data"] as? [[String: Any]],
                  let firstID = list.first?["id"] as? String else { return nil }
            // Match by filesystem path (the model location stored in ModelRef).
            return availableModels.first {
                if case .filesystem(let path) = $0.location {
                    return path == firstID || firstID.hasPrefix(path) || path.hasPrefix(firstID)
                }
                return false
            }
        } catch {
            return nil
        }
    }

    private func mergedEnvironment(_ extra: [String: String]) -> [String: String] {
        var env = ProcessInfo.processInfo.environment
        env.merge(extra) { _, new in new }
        return env
    }

    /// Returns the PID of the process listening on the given TCP port, or nil.
    /// Runs `lsof` on a background queue to avoid blocking MainActor.
    private func findListeningPID(port: Int) async -> pid_t? {
        await withCheckedContinuation { (continuation: CheckedContinuation<pid_t?, Never>) in
            DispatchQueue.global(qos: .userInitiated).async {
                let proc = Process()
                proc.executableURL = URL(fileURLWithPath: "/usr/sbin/lsof")
                proc.arguments = ["-t", "-i", ":\(port)", "-sTCP:LISTEN"]
                let pipe = Pipe()
                proc.standardOutput = pipe
                proc.standardError = Pipe()
                try? proc.run()
                proc.waitUntilExit()
                let data = pipe.fileHandleForReading.readDataToEndOfFile()
                let str = String(data: data, encoding: .utf8)?
                    .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                // lsof -t may return multiple PIDs (one per line); take the first.
                let first = str.split(separator: "\n").first.map(String.init) ?? str
                continuation.resume(returning: pid_t(first))
            }
        }
    }

    /// Sends signal 0 to check process existence without actually signalling it.
    private func isExternalProcessRunning(pid: pid_t) -> Bool {
        kill(pid, 0) == 0
    }

    private func notify(title: String, body: String) async {
        guard UserDefaults.standard.object(forKey: "localbar.notificationsEnabled") as? Bool ?? true else { return }
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
        try? await UNUserNotificationCenter.current().add(request)
    }
}

