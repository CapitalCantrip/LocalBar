import SwiftUI

/// Root view shown when the user opens the menu bar extra.
struct MenuBarView: View {
    @Environment(InstanceRegistry.self) private var registry
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if registry.controllers.isEmpty {
                EmptyStateView()
            } else {
                ForEach(registry.controllers) { controller in
                    InstanceRowView(controller: controller)
                    Divider()
                }
            }

            Divider().padding(.vertical, 4)

            if registry.hasAnyRunning {
                Button("Stop All") {
                    Task { await registry.stopAll() }
                }
                .foregroundStyle(.red)

                Divider().padding(.vertical, 2)
            }

            Button("Settings…") {
                NSApp.activate(ignoringOtherApps: true)
                openSettings()
            }
            .keyboardShortcut(",", modifiers: .command)

            Divider().padding(.vertical, 2)

            Button("Quit LocalBar") {
                NSApp.terminate(nil)
            }
            .keyboardShortcut("q", modifiers: .command)
        }
        .padding(8)
        .frame(minWidth: 280)
    }
}

// MARK: - Empty state

private struct EmptyStateView: View {
    var body: some View {
        VStack(spacing: 8) {
            Text("No servers configured")
                .font(.headline)
            Text("Open Settings to add a server.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding()
    }
}

// MARK: - Instance row

struct InstanceRowView: View {
    let controller: ServerInstanceController
    @Environment(InstanceRegistry.self) private var registry

    // Async-safe start: set a new UUID to trigger .task, which runs the
    // concurrent-server check and memory warning before calling controller.start().
    @State private var startTrigger: UUID? = nil
    @State private var showConcurrentWarning = false
    @State private var concurrentWarningNames: [String] = []
    @State private var memoryWarning: MemoryWarningInfo? = nil

    /// Captured footprint shown in the E1 modal. Separate from MemoryFootprintEstimate
    /// so the alert can be driven by a single optional.
    struct MemoryWarningInfo {
        var weightBytes: Int64?
        var kvCacheBytes: Int64?
        var totalBytes: Int64
        var totalRAM: Int64
    }

    var body: some View {
        HStack {
            PhaseIndicatorView(phase: controller.phase)

            VStack(alignment: .leading, spacing: 2) {
                Text(controller.config.name)
                    .font(.headline)
                if let model = controller.currentModel {
                    Text(modelLabel(for: model))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                if let error = controller.phase.error {
                    Text(errorSummary(error))
                        .font(.caption)
                        .foregroundStyle(.red)
                }
            }

            Spacer()

            instanceActions
        }
        .padding(.vertical, 6)
        .task(id: startTrigger) {
            guard startTrigger != nil else { return }
            defer { startTrigger = nil }
            let port = controller.config.port

            // 1. Concurrent-server check.
            let runningManaged = registry.controllers.filter {
                $0.id != controller.id && $0.phase.isRunning
            }
            let hasConcurrent: Bool
            if !runningManaged.isEmpty {
                hasConcurrent = true
            } else {
                hasConcurrent = await registry.hasAnyRunningIncludingExternal(excludingPort: port)
            }
            if hasConcurrent {
                let managedNames = runningManaged.map(\.config.name)
                concurrentWarningNames = managedNames.isEmpty ? ["an external server"] : managedNames
                showConcurrentWarning = true
                return
            }

            // 2. Memory footprint check (>70% of total RAM → E1 modal).
            let totalRAM = systemTotalMemoryBytes()
            if totalRAM > 0 {
                let estimate = await controller.estimatedMemoryFootprint()
                if estimate.totalBytes > 0 {
                    let threshold = Int64(Double(totalRAM) * 0.70)
                    if estimate.totalBytes > threshold {
                        memoryWarning = MemoryWarningInfo(
                            weightBytes:  estimate.weightBytes,
                            kvCacheBytes: estimate.kvCacheBytes,
                            totalBytes:   estimate.totalBytes,
                            totalRAM:     totalRAM
                        )
                        return
                    }
                }
            }

            await controller.start()
        }
        .alert("Another server is already running", isPresented: $showConcurrentWarning) {
            Button("Start Anyway", role: .destructive) {
                Task { await controller.start() }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            let names = concurrentWarningNames.joined(separator: ", ")
            Text("\(names) is already running. Starting another model may exhaust unified memory on Apple Silicon.")
        }
        .alert("High memory usage estimated", isPresented: Binding(
            get: { memoryWarning != nil },
            set: { if !$0 { memoryWarning = nil } }
        )) {
            Button("Start Anyway", role: .destructive) {
                memoryWarning = nil
                Task { await controller.start() }
            }
            Button("Cancel", role: .cancel) {
                memoryWarning = nil
            }
        } message: {
            if let w = memoryWarning {
                Text(memoryWarningMessage(w))
            }
        }
    }

    private func memoryWarningMessage(_ w: MemoryWarningInfo) -> String {
        var lines: [String] = []
        if let wb = w.weightBytes {
            lines.append("Model weights:   \(formatGB(wb))")
        }
        if let kv = w.kvCacheBytes {
            lines.append("KV cache:        \(formatGB(kv))")
        }
        let pct = Int((Double(w.totalBytes) / Double(w.totalRAM)) * 100)
        lines.append("Estimated total: \(formatGB(w.totalBytes)) (\(pct)% of \(formatGB(w.totalRAM)) RAM)")
        lines.append("")
        lines.append("This may cause system instability or swapping.")
        return lines.joined(separator: "\n")
    }

    private func formatGB(_ bytes: Int64) -> String {
        let gb = Double(bytes) / 1_073_741_824.0
        return String(format: "%.1f GB", gb)
    }

    private func errorSummary(_ error: InstanceError) -> String {
        switch error.kind {
        case .portConflict(let port, let occupant):
            if let occupant { return "Port conflict · \(port) (in use by \(occupant))" }
            return "Port conflict · \(port)"
        case .healthCheckFailed:
            return "Health check timed out"
        case .crashed(let code):
            if let code { return "Crashed (exit \(code))" }
            return "Crashed unexpectedly"
        case .launchFailed:
            return "Launch failed"
        case .shutdownTimedOut:
            return "Shutdown timed out"
        }
    }

    /// "Qwen2.5 7B" — appends the parsed parameter count when it isn't
    /// already part of the display name.
    private func modelLabel(for model: ModelRef) -> String {
        guard let parameterCount = model.metadata?.parameterCount,
              !model.displayName.localizedCaseInsensitiveContains(parameterCount) else {
            return model.displayName
        }
        return "\(model.displayName) \(parameterCount)"
    }

    @ViewBuilder
    private var instanceActions: some View {
        switch controller.phase {
        case .stopped:
            Button("Start") { startTrigger = UUID() }
        case .running:
            Button("Stop") { Task { await controller.stop() } }
        case .starting, .stopping, .switchingModel:
            ProgressView().scaleEffect(0.6)
        case .error:
            Button("Retry") { Task { await controller.retryFromError() } }
                .foregroundStyle(.red)
        }
    }
}

// MARK: - Phase indicator dot

struct PhaseIndicatorView: View {
    let phase: InstancePhase

    var body: some View {
        Circle()
            .fill(dotColor)
            .frame(width: 8, height: 8)
    }

    private var dotColor: Color {
        switch phase.iconState {
        case .on:            return .green
        case .off:           return .secondary
        case .error:         return .red
        case .transitioning: return .yellow
        }
    }
}

// MARK: - Menu bar icon

struct MenuBarIconView: View {
    let registry: InstanceRegistry

    var body: some View {
        Image(systemName: iconName)
            .symbolVariant(symbolVariant)
    }

    private var iconName: String {
        switch registry.aggregateIconState {
        case .on:            return "brain.head.profile.fill"
        case .off:           return "brain.head.profile"
        case .error:         return "exclamationmark.triangle.fill"
        case .transitioning: return "arrow.trianglehead.2.clockwise.rotate.90"
        }
    }

    private var symbolVariant: SymbolVariants {
        // brain.head.profile already has an explicit .fill variant above;
        // the .fill SymbolVariant would double-apply and may not resolve.
        .none
    }
}
