import SwiftUI

// Forces the SwiftUI Settings window to be user-resizable.
// The Scene-level .windowResizability modifier alone doesn't apply the resize
// chrome to the Settings window reliably; patching the NSWindow directly does.
private struct ResizableWindowHelper: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async { view.window?.styleMask.insert(.resizable) }
        return view
    }
    func updateNSView(_ nsView: NSView, context: Context) {}
}

struct SettingsView: View {
    @Environment(InstanceRegistry.self) private var registry

    var body: some View {
        TabView {
            ServersTab()
                .tabItem { Label("Servers", systemImage: "server.rack") }

            ProfilesTab()
                .tabItem { Label("Profiles", systemImage: "slider.horizontal.3") }

            GeneralTab()
                .tabItem { Label("General", systemImage: "gearshape") }
        }
        .frame(minWidth: 760, minHeight: 420)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(ResizableWindowHelper())
        .onAppear {
            // Show LocalBar in Cmd+Tab while Settings is open.
            NSApp.setActivationPolicy(.regular)
        }
        .onDisappear {
            // Return to menu-bar-only mode when Settings closes.
            NSApp.setActivationPolicy(.accessory)
        }
    }
}

// MARK: - Display helpers for the instance table

extension ServerInstanceController {
    /// The model shown in the UI: the running model if any, else the
    /// configured selection resolved against the last scanned model list.
    var displayedModel: ModelRef? {
        currentModel ?? availableModels.first { $0.key == config.selectedModelKey }
    }

    var sortName: String { config.name }

    var sortModel: String {
        displayedModel?.displayName ?? config.selectedModelKey ?? ""
    }

    var sortFormat: String {
        displayedModel?.metadata?.modelFormat.rawValue ?? ""
    }

    var sortParameters: String {
        displayedModel?.metadata?.parameterCount ?? ""
    }

    var quantizationLabel: String {
        displayedModel?.metadata?.quantization ?? ""
    }

    var capabilitiesLabel: String {
        guard let caps = displayedModel?.metadata?.capabilities, !caps.isEmpty else { return "" }
        return caps.map(\.rawValue).sorted().joined(separator: ", ")
    }

    var phaseLabel: String {
        switch phase {
        case .stopped:        return "Stopped"
        case .starting:       return "Starting…"
        case .running:        return "Running"
        case .stopping:       return "Stopping…"
        case .switchingModel: return "Switching…"
        case .error:          return "Error"
        }
    }
}

private extension ModelMetadata {
    /// One-line summary shown under model names in pickers.
    var summary: String {
        var parts: [String] = []
        if let parameterCount { parts.append(parameterCount) }
        if let quantization { parts.append(quantization) }
        parts.append(modelFormat.rawValue)
        if !capabilities.isEmpty {
            parts.append(capabilities.map(\.rawValue).sorted().joined(separator: ", "))
        }
        return parts.joined(separator: " · ")
    }
}

// MARK: - Servers tab

private struct ServersTab: View {
    @Environment(InstanceRegistry.self) private var registry

    @State private var selectedID: UUID?
    @State private var showingAddSheet = false
    @State private var sortOrder: [KeyPathComparator<ServerInstanceController>] = [
        KeyPathComparator(\ServerInstanceController.sortName)
    ]
    @State private var columnCustomization = TableColumnCustomization<ServerInstanceController>()
    @AppStorage("serversTableColumnCustomization") private var columnCustomizationData = Data()
    /// Set by the context menu Start action; triggers the async running-server check.
    @State private var checkingStartForController: ServerInstanceController?
    /// Set after the async check confirms a conflict; shows the confirmation alert.
    @State private var pendingStartController: ServerInstanceController?

    private var sortedControllers: [ServerInstanceController] {
        registry.controllers.sorted(using: sortOrder)
    }

    private var selectedController: ServerInstanceController? {
        selectedID.flatMap { registry.controller(for: $0) }
    }

    var body: some View {
        HSplitView {
            // ── Left: sortable instance table ──────────────────────────
            VStack(spacing: 0) {
                Table(
                    sortedControllers,
                    selection: $selectedID,
                    sortOrder: $sortOrder,
                    columnCustomization: $columnCustomization
                ) {
                    TableColumn("Name", value: \.sortName) { controller in
                        HStack(spacing: 6) {
                            PhaseIndicatorView(phase: controller.phase)
                            Text(controller.config.name)
                        }
                    }
                    .customizationID("name")

                    TableColumn("Model", value: \.sortModel) { controller in
                        Text(controller.sortModel.isEmpty ? "—" : controller.sortModel)
                            .truncationMode(.middle)
                    }
                    .customizationID("model")

                    TableColumn("Type", value: \.sortFormat) { controller in
                        Text(controller.sortFormat.isEmpty ? "—" : controller.sortFormat)
                    }
                    .width(min: 50, ideal: 70)
                    .customizationID("type")

                    TableColumn("Params", value: \.sortParameters) { controller in
                        Text(controller.sortParameters.isEmpty ? "—" : controller.sortParameters)
                    }
                    .width(min: 55, ideal: 70)
                    .customizationID("parameters")

                    TableColumn("Quant") { controller in
                        Text(controller.quantizationLabel.isEmpty ? "—" : controller.quantizationLabel)
                    }
                    .width(min: 55, ideal: 70)
                    .customizationID("quantization")

                    TableColumn("Status") { controller in
                        Text(controller.phaseLabel)
                            .foregroundStyle(statusColor(for: controller.phase))
                    }
                    .width(min: 65, ideal: 80)
                    .customizationID("status")

                    TableColumn("Server") { controller in
                        Text(controller.config.type.rawValue)
                    }
                    .width(min: 50, ideal: 70)
                    .customizationID("server")
                    .defaultVisibility(.hidden)

                    TableColumn("Capabilities") { controller in
                        Text(controller.capabilitiesLabel.isEmpty ? "—" : controller.capabilitiesLabel)
                            .lineLimit(1)
                    }
                    .customizationID("capabilities")
                    .defaultVisibility(.hidden)

                    TableColumn("Port") { controller in
                        Text(String(controller.config.port))
                            .monospacedDigit()
                    }
                    .width(min: 44, ideal: 56)
                    .customizationID("port")
                    .defaultVisibility(.hidden)
                }
                .contextMenu(forSelectionType: UUID.self) { ids in
                    if let id = ids.first,
                       let controller = registry.controller(for: id) {
                        instanceContextMenu(for: controller, id: id)
                    }
                }

                Divider()

                HStack(spacing: 0) {
                    Button(action: { showingAddSheet = true }) {
                        Image(systemName: "plus").frame(width: 24, height: 22)
                    }
                    .buttonStyle(.borderless)

                    Divider().frame(height: 16)

                    Button(action: removeSelected) {
                        Image(systemName: "minus").frame(width: 24, height: 22)
                    }
                    .buttonStyle(.borderless)
                    .disabled(selectedID == nil)

                    Spacer()
                }
                .padding(.horizontal, 4)
                .padding(.vertical, 2)
            }
            .frame(minWidth: 440)

            // ── Right: detail panel ────────────────────────────────────
            // Always keep the pane in the hierarchy so HSplitView stays
            // two-column. Swap content based on selection.
            Group {
                if let controller = selectedController {
                    InstanceDetailPanel(controller: controller)
                        .id(controller.id)  // force recreation on selection change
                } else {
                    VStack(spacing: 12) {
                        Image(systemName: "cpu")
                            .font(.system(size: 36))
                            .foregroundStyle(.tertiary)
                        Text("Select an instance")
                            .foregroundStyle(.secondary)
                        Button("Add Instance") { showingAddSheet = true }
                            .buttonStyle(.borderedProminent)
                            .controlSize(.small)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .frame(minWidth: 280, maxWidth: 340)
        }
        .task(id: checkingStartForController?.id) {
            guard let c = checkingStartForController else { return }
            checkingStartForController = nil
            if await registry.hasAnyRunningIncludingExternal(excludingPort: c.config.port) {
                pendingStartController = c
            } else {
                await c.start()
            }
        }
        .alert(
            "Another server is running",
            isPresented: Binding(
                get: { pendingStartController != nil },
                set: { if !$0 { pendingStartController = nil } }
            )
        ) {
            Button("Start Anyway") {
                if let c = pendingStartController { Task { await c.start() } }
                pendingStartController = nil
            }
            Button("Cancel", role: .cancel) { pendingStartController = nil }
        } message: {
            if let c = pendingStartController {
                let managedNames = registry.controllers
                    .filter { $0.phase.isRunning && $0.id != c.id }
                    .map(\.config.name)
                let description = managedNames.isEmpty ? "an external server" : managedNames.joined(separator: ", ")
                Text("Starting \"\(c.config.name)\" while \(description) is running may exhaust unified memory. Continue?")
            }
        }
        .task {
            restoreColumnCustomization()
            for controller in registry.controllers {
                await controller.refreshModels()
            }
        }
        .onChange(of: columnCustomization) { _, newValue in
            persistColumnCustomization(newValue)
        }
        .sheet(isPresented: $showingAddSheet) {
            AddInstanceSheet()
                .environment(registry)
        }
    }

    private func removeSelected() {
        guard let id = selectedID else { return }
        registry.removeInstance(id: id)
        selectedID = nil
    }

    @MainActor @ViewBuilder
    private func instanceContextMenu(for controller: ServerInstanceController, id: UUID) -> some View {
        if controller.phase.isRunning {
            Button("Stop")    { Task { await controller.stop() } }
            Button("Restart") { Task { await controller.restart() } }
            Divider()
        } else if !controller.phase.isTransitioning {
            // stopped or error — start after async running-server check
            Button("Start") { checkingStartForController = controller }
            Divider()
        }
        Button("Remove", role: .destructive) {
            registry.removeInstance(id: id)
            if selectedID == id { selectedID = nil }
        }
    }

    private func statusColor(for phase: InstancePhase) -> Color {
        switch phase.iconState {
        case .on:            return .green
        case .off:           return .secondary
        case .error:         return .red
        case .transitioning: return .yellow
        }
    }

    // MARK: Column customization persistence

    private func restoreColumnCustomization() {
        guard !columnCustomizationData.isEmpty,
              let restored = try? JSONDecoder().decode(
                TableColumnCustomization<ServerInstanceController>.self,
                from: columnCustomizationData
              ) else { return }
        columnCustomization = restored
    }

    private func persistColumnCustomization(_ value: TableColumnCustomization<ServerInstanceController>) {
        columnCustomizationData = (try? JSONEncoder().encode(value)) ?? Data()
    }
}

// MARK: - Instance detail panel (inline right pane)

private struct InstanceDetailPanel: View {
    let controller: ServerInstanceController

    @Environment(InstanceRegistry.self) private var registry

    @State private var editedName = ""
    @State private var editedModelKey: String = ""  // "" = nil sentinel
    @State private var editedProfileID: UUID? = nil
    @State private var portOverride: String = ""
    @State private var showingConcurrentWarning = false
    @State private var memoryWarning: MemoryFootprintWarning? = nil

    // Param panel state — string drafts for TextField controls, bool for toggles.
    @State private var paramDrafts: [CanonicalParam: String] = [:]
    @State private var boolDrafts: [CanonicalParam: Bool] = [:]
    @State private var systemPromptDraft: String = ""

    private var models: [ModelRef] { controller.availableModels }
    private var driver: any ServerDriver { DriverRegistry.driver(for: controller.config.type) }

    /// Profiles compatible with this instance's server type.
    private var compatibleProfiles: [NamedProfile] {
        registry.profiles.filter { $0.serverType == nil || $0.serverType == controller.config.type }
    }

    /// The active profile, if any.
    private var activeProfile: NamedProfile? {
        registry.profile(for: controller.config.activeProfileID)
    }

    var body: some View {
        VStack(spacing: 0) {
            // Panel header
            HStack(spacing: 8) {
                PhaseIndicatorView(phase: controller.phase)
                Text(controller.config.name)
                    .fontWeight(.medium)
                    .lineLimit(1)
                Spacer()
                Text(controller.phaseLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    controlsSection
                    configSection
                    modelSection
                    profileSection
                    parametersSection
                }
                .padding(12)
            }
        } // end outer VStack
        .alert("Another server is running", isPresented: $showingConcurrentWarning) {
            Button("Start Anyway") {
                Task {
                    // Use retryFromError if we're in an error state (e.g. after
                    // a port-conflict → port-change flow), otherwise start fresh.
                    if case .error = controller.phase {
                        await controller.retryFromError()
                    } else {
                        await controller.start()
                    }
                }
            }
            Button("Cancel", role: .cancel) { }
        } message: {
            let managedNames = registry.controllers
                .filter { $0.phase.isRunning && $0.id != controller.id }
                .map(\.config.name)
            let description = managedNames.isEmpty ? "an external server" : managedNames.joined(separator: ", ")
            Text("Starting \"\(controller.config.name)\" while \(description) is running may exhaust unified memory. Continue?")
        }
        .alert("High memory usage estimated", isPresented: Binding(
            get: { memoryWarning != nil },
            set: { if !$0 { memoryWarning = nil } }
        )) {
            Button("Start Anyway", role: .destructive) {
                memoryWarning = nil
                Task { await controller.start() }
            }
            Button("Cancel", role: .cancel) { memoryWarning = nil }
        } message: {
            if let w = memoryWarning {
                Text(w.message(formatGB: formatGB))
            }
        }
        .onAppear {
            editedName = controller.config.name
            editedModelKey = controller.config.selectedModelKey ?? ""
            editedProfileID = controller.config.activeProfileID
            portOverride = String(controller.config.port)
            initParamDrafts()
        }
        .onChange(of: controller.config.activeProfileID) { _, newID in
            editedProfileID = newID
        }
        .onChange(of: editedName) { _, newValue in
            let trimmed = newValue.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty, trimmed != controller.config.name else { return }
            var updated = controller.config
            updated.name = trimmed
            controller.updateConfig(updated)
        }
        .onChange(of: editedModelKey) { _, newValue in
            let key: String? = newValue.isEmpty ? nil : newValue
            guard key != controller.config.selectedModelKey else { return }
            var updated = controller.config
            updated.selectedModelKey = key
            controller.updateConfig(updated)
        }
        .onChange(of: editedProfileID) { _, newID in
            guard newID != controller.config.activeProfileID else { return }
            var updated = controller.config
            updated.activeProfileID = newID
            controller.updateConfig(updated)
        }
    }

    // MARK: Body sections (split out to keep body type-checkable)

    @ViewBuilder private var controlsSection: some View {
        GroupBox("Controls") {
            HStack(spacing: 12) {
                controlButtons
                Spacer()
            }
            if case .error(let err) = controller.phase {
                Divider()
                VStack(alignment: .leading, spacing: 6) {
                    Text(err.message)
                        .font(.callout)
                        .foregroundStyle(.red)
                    if err.previousModelKey != nil {
                        Button("Restart with previous model") {
                            Task { await controller.rollbackToPreviousModel() }
                        }
                    }
                }
            }
            if let usage = controller.contextUsage {
                Divider()
                LabeledContent("Context") {
                    Text("\(usage.usedTokens) / \(usage.maxTokens) tokens")
                }
            }
        }
    }

    @ViewBuilder private var configSection: some View {
        GroupBox("Configuration") {
            LabeledContent("Name") {
                TextField("Instance name", text: $editedName)
                    .textFieldStyle(.roundedBorder)
                    .frame(maxWidth: 220)
            }
            LabeledContent("Type") { Text(controller.config.type.rawValue) }
            LabeledContent("Host") { Text(controller.config.host).monospacedDigit() }
            LabeledContent("Port") {
                if controller.phase.isTransitioning || controller.phase.isRunning {
                    Text(String(controller.config.port))
                        .monospacedDigit()
                        .foregroundStyle(.secondary)
                } else {
                    TextField("", text: $portOverride)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 80)
                        .monospacedDigit()
                        .onSubmit { commitPortOverride() }
                }
            }
            LabeledContent("Executable") {
                Text(controller.config.executablePath)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .font(.callout)
            }
        }
    }

    @ViewBuilder private var modelSection: some View {
        if !models.isEmpty {
            GroupBox("Model") {
                Picker("", selection: $editedModelKey) {
                    Text("None").tag("")
                    ForEach(models) { model in
                        VStack(alignment: .leading, spacing: 2) {
                            Text(model.displayName)
                            if let meta = model.metadata {
                                Text(meta.summary)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .tag(model.key)
                    }
                }
                .labelsHidden()
                .pickerStyle(.radioGroup)
            }
        } else {
            GroupBox("Model") {
                Button("Scan for models") {
                    Task { await controller.refreshModels() }
                }
            }
        }
    }

    @ViewBuilder private var profileSection: some View {
        GroupBox("Profile") {
            Picker("Active Profile", selection: $editedProfileID) {
                Text("None (auto-memory)").tag(Optional<UUID>.none)
                if !compatibleProfiles.isEmpty {
                    Divider()
                    ForEach(compatibleProfiles) { profile in
                        Text(profile.name).tag(Optional(profile.id))
                    }
                }
            }
            .labelsHidden()
            if activeProfile != nil {
                Text("Parameters from the active profile override instance settings.")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            } else if compatibleProfiles.isEmpty {
                Text("No profiles yet — create one in the Profiles tab.")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    @ViewBuilder private var parametersSection: some View {
        GroupBox("Parameters") {
            let schema = driver.paramSchema
            if schema.isEmpty {
                Text("No configurable parameters for this server type.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(schema) { descriptor in
                    paramRow(for: descriptor)
                    if descriptor.id != schema.last?.id { Divider() }
                }
            }
            if controller.config.type == .ollama {
                Divider()
                systemPromptSection
            }
        }
    }

    @ViewBuilder private var systemPromptSection: some View {
        let profilePrompt = activeProfile?.params.systemPrompt
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("System Prompt")
                    .font(.callout)
                Spacer()
                if profilePrompt != nil {
                    Text("Profile")
                        .font(.caption2)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 2)
                        .background(.purple.opacity(0.12))
                        .foregroundStyle(.purple)
                        .clipShape(Capsule())
                } else {
                    Text("Baked into Modelfile")
                        .font(.caption2)
                        .padding(.horizontal, 5)
                        .padding(.vertical, 2)
                        .background(.orange.opacity(0.15))
                        .foregroundStyle(.orange)
                        .clipShape(Capsule())
                }
            }
            if let locked = profilePrompt {
                Text(locked.isEmpty ? "(empty)" : locked)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(6)
                    .background(Color.secondary.opacity(0.07))
                    .clipShape(RoundedRectangle(cornerRadius: 6))
            } else {
                TextEditor(text: $systemPromptDraft)
                    .font(.callout)
                    .frame(minHeight: 80, maxHeight: 160)
                    .overlay(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(Color.secondary.opacity(0.3), lineWidth: 1)
                    )
                HStack {
                    if systemPromptDraft.isEmpty {
                        Text("Leave blank to use the model's built-in default.")
                            .font(.caption)
                            .foregroundStyle(.tertiary)
                    }
                    Spacer()
                    let stored = controller.config.instanceParams.systemPrompt ?? ""
                    if systemPromptDraft != stored {
                        Button("Save") { commitSystemPrompt() }
                            .font(.caption)
                            .buttonStyle(.borderless)
                            .foregroundStyle(Color.accentColor)
                    }
                }
            }
        }
        .padding(.top, 4)
    }

    // MARK: Param panel helpers

    /// Build a single row for a CanonicalParam descriptor.
    @ViewBuilder
    private func paramRow(for descriptor: ParamDescriptor) -> some View {
        let isServerSide = descriptor.application == .serverSideDefault
        let profileValue = activeProfile?.params.values[descriptor.param]
        HStack(alignment: .top, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(humanName(descriptor.param))
                        .font(.callout)
                    if isServerSide {
                        Text("Modelfile")
                            .font(.caption2)
                            .padding(.horizontal, 5)
                            .padding(.vertical, 2)
                            .background(.orange.opacity(0.15))
                            .foregroundStyle(.orange)
                            .clipShape(Capsule())
                    }
                    if profileValue != nil {
                        Text("Profile")
                            .font(.caption2)
                            .padding(.horizontal, 5)
                            .padding(.vertical, 2)
                            .background(.purple.opacity(0.12))
                            .foregroundStyle(.purple)
                            .clipShape(Capsule())
                    }
                }
                if let note = descriptor.note {
                    Text(note)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
            Spacer()
            if let profileValue {
                // Profile is overriding this param — show locked value.
                Text(paramValueString(profileValue))
                    .font(.callout)
                    .monospacedDigit()
                    .foregroundStyle(.secondary)
                    .frame(minWidth: 60, alignment: .trailing)
            } else {
                paramControl(for: descriptor)
            }
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder
    private func paramControl(for descriptor: ParamDescriptor) -> some View {
        let defaultText = descriptor.defaultValue.map { paramValueString($0) }

        switch descriptor.valueType {
        case .bool:
            Toggle("", isOn: Binding(
                get: { boolDrafts[descriptor.param] ?? false },
                set: { newVal in
                    boolDrafts[descriptor.param] = newVal
                    commitBool(descriptor.param, value: newVal)
                }
            ))
            .labelsHidden()
            .frame(width: 44)

        case .double(let range):
            if let range {
                VStack(alignment: .trailing, spacing: 2) {
                    HStack(spacing: 6) {
                        Slider(
                            value: Binding(
                                get: { Double(paramDrafts[descriptor.param] ?? "") ?? range.lowerBound },
                                set: { paramDrafts[descriptor.param] = String(format: "%.3g", $0) }
                            ),
                            in: range,
                            onEditingChanged: { editing in
                                if !editing { commitText(descriptor) }
                            }
                        )
                        .frame(width: 100)
                        TextField(defaultText ?? "default", text: paramBinding(descriptor))
                            .textFieldStyle(.roundedBorder)
                            .frame(width: 60)
                            .multilineTextAlignment(.trailing)
                            .onSubmit { commitText(descriptor) }
                    }
                    clearButton(for: descriptor)
                }
            } else {
                VStack(alignment: .trailing, spacing: 2) {
                    TextField(defaultText ?? "default", text: paramBinding(descriptor))
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 80)
                        .multilineTextAlignment(.trailing)
                        .onSubmit { commitText(descriptor) }
                    clearButton(for: descriptor)
                }
            }

        case .int(let range):
            // Use Stepper only for small ranges (≤ 100 values). Avoid .count on large
            // ranges like 0...Int.max which overflows.
            if let range, range.upperBound - range.lowerBound <= 100 {
                VStack(alignment: .trailing, spacing: 2) {
                    Stepper(
                        value: Binding(
                            get: { Int(paramDrafts[descriptor.param] ?? "") ?? defaultIntValue(descriptor, fallback: range.lowerBound) },
                            set: { newVal in
                                paramDrafts[descriptor.param] = String(newVal)
                                commitText(descriptor)
                            }
                        ),
                        in: range
                    ) {
                        Text(paramDrafts[descriptor.param] ?? defaultText ?? "default")
                            .frame(width: 50, alignment: .trailing)
                            .monospacedDigit()
                    }
                    clearButton(for: descriptor)
                }
            } else {
                VStack(alignment: .trailing, spacing: 2) {
                    TextField(defaultText ?? "default", text: paramBinding(descriptor))
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 80)
                        .multilineTextAlignment(.trailing)
                        .monospacedDigit()
                        .onSubmit { commitText(descriptor) }
                    clearButton(for: descriptor)
                }
            }

        case .string:
            VStack(alignment: .trailing, spacing: 2) {
                TextField(defaultText ?? "default", text: paramBinding(descriptor))
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 100)
                    .onSubmit { commitText(descriptor) }
                clearButton(for: descriptor)
            }
        }
    }

    @ViewBuilder
    private func clearButton(for descriptor: ParamDescriptor) -> some View {
        if paramDrafts[descriptor.param] != nil {
            Button("Reset") {
                paramDrafts.removeValue(forKey: descriptor.param)
                var updated = controller.config
                updated.instanceParams.values.removeValue(forKey: descriptor.param)
                controller.updateConfig(updated)
            }
            .font(.caption2)
            .foregroundStyle(.secondary)
        }
    }

    private func defaultIntValue(_ descriptor: ParamDescriptor, fallback: Int) -> Int {
        if case .int(let i) = descriptor.defaultValue { return i }
        return fallback
    }

    private func paramBinding(_ descriptor: ParamDescriptor) -> Binding<String> {
        Binding(
            get: { paramDrafts[descriptor.param] ?? "" },
            set: { newVal in
                paramDrafts[descriptor.param] = newVal.isEmpty ? nil : newVal
                commitText(descriptor)   // save on every change; commitText skips unparseable partials
            }
        )
    }

    private func commitText(_ descriptor: ParamDescriptor) {
        guard let raw = paramDrafts[descriptor.param], !raw.isEmpty else {
            var updated = controller.config
            updated.instanceParams.values.removeValue(forKey: descriptor.param)
            controller.updateConfig(updated)
            return
        }
        var value: ParamValue?
        switch descriptor.valueType {
        case .double:   if let d = Double(raw) { value = .double(d) }
        case .int:      if let i = Int(raw)    { value = .int(i)    }
        case .string:   value = .string(raw)
        case .bool:     break
        }
        guard let value else { return }
        var updated = controller.config
        updated.instanceParams.values[descriptor.param] = value
        controller.updateConfig(updated)
    }

    private func commitBool(_ param: CanonicalParam, value: Bool) {
        var updated = controller.config
        updated.instanceParams.values[param] = .bool(value)
        controller.updateConfig(updated)
    }

    private func commitSystemPrompt() {
        var updated = controller.config
        updated.instanceParams.systemPrompt = systemPromptDraft.isEmpty ? nil : systemPromptDraft
        controller.updateConfig(updated)
    }

    private func initParamDrafts() {
        paramDrafts = [:]
        boolDrafts = [:]
        for descriptor in driver.paramSchema {
            if let val = controller.config.instanceParams.values[descriptor.param] {
                switch val {
                case .double(let d): paramDrafts[descriptor.param] = String(format: "%.3g", d)
                case .int(let i):    paramDrafts[descriptor.param] = String(i)
                case .string(let s): paramDrafts[descriptor.param] = s
                case .bool(let b):   boolDrafts[descriptor.param] = b
                }
            } else if case .bool(let b) = descriptor.defaultValue {
                boolDrafts[descriptor.param] = b
            }
        }
        systemPromptDraft = controller.config.instanceParams.systemPrompt ?? ""
    }

    private func humanName(_ param: CanonicalParam) -> String {
        switch param {
        case .contextLength:  return "Context Length"
        case .temperature:    return "Temperature"
        case .maxTokens:      return "Max Tokens"
        case .topK:           return "Top K"
        case .repeatPenalty:  return "Repeat Penalty"
        case .presencePenalty: return "Presence Penalty"
        case .topP:           return "Top P"
        case .minP:           return "Min P"
        case .seed:           return "Seed"
        case .systemPrompt:   return "System Prompt"
        }
    }

    private func paramValueString(_ value: ParamValue) -> String {
        switch value {
        case .double(let d): return String(format: "%.3g", d)
        case .int(let i):    return String(i)
        case .string(let s): return s
        case .bool(let b):   return b ? "On" : "Off"
        }
    }

    /// Commit a port edit made while the server is stopped. Called on submit/blur.
    private func commitPortOverride() {
        guard let port = Int(portOverride),
              (1...65_535).contains(port),
              port != controller.config.port else {
            portOverride = String(controller.config.port) // revert invalid
            return
        }
        var updated = controller.config
        updated.port = port
        controller.updateConfig(updated)
    }

    private func applyPortOverrideAndRetry() {
        // Apply the port change first (if valid and different).
        var excludePort = controller.config.port
        if let port = Int(portOverride), (1...65_535).contains(port), port != controller.config.port {
            var updated = controller.config
            updated.port = port
            controller.updateConfig(updated)
            excludePort = port
        }
        // Run the concurrent-server check against the NEW port before retrying.
        // Without this, changing port sidesteps the warning about the unmanaged
        // server that's still consuming memory on a different port.
        Task {
            if await registry.hasAnyRunningIncludingExternal(excludingPort: excludePort) {
                showingConcurrentWarning = true
            } else {
                await controller.retryFromError()
            }
        }
    }

    private func handleStart() {
        Task {
            if await registry.hasAnyRunningIncludingExternal(excludingPort: controller.config.port) {
                showingConcurrentWarning = true
                return
            }
            let totalRAM = systemTotalMemoryBytes()
            if totalRAM > 0 {
                let estimate = await controller.estimatedMemoryFootprint()
                if estimate.totalBytes > 0, estimate.totalBytes > Int64(Double(totalRAM) * 0.70) {
                    memoryWarning = MemoryFootprintWarning(
                        weightBytes:  estimate.weightBytes,
                        kvCacheBytes: estimate.kvCacheBytes,
                        totalBytes:   estimate.totalBytes,
                        totalRAM:     totalRAM
                    )
                    return
                }
            }
            await controller.start()
        }
    }

    private func formatGB(_ bytes: Int64) -> String {
        String(format: "%.1f GB", Double(bytes) / 1_073_741_824.0)
    }

    @ViewBuilder
    private var controlButtons: some View {
        switch controller.phase {
        case .stopped:
            Button("Start") { handleStart() }
                .buttonStyle(.borderedProminent)
        case .running:
            Button("Stop") { Task { await controller.stop() } }
            Button("Restart") { Task { await controller.restart() } }
        case .starting, .stopping, .switchingModel:
            ProgressView().controlSize(.small)
        case .error(let err):
            if case .portConflict = err.kind {
                Button("Adopt") { Task { await registry.adoptExternalAsNewInstance(from: controller) } }
                    .buttonStyle(.borderedProminent)
                HStack(spacing: 4) {
                    Text("or port")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                    TextField("", text: $portOverride)
                        .frame(width: 52)
                        .multilineTextAlignment(.trailing)
                    Button("Retry") { applyPortOverrideAndRetry() }
                }
            } else {
                Button("Retry") { Task { await controller.retryFromError() } }
                    .buttonStyle(.borderedProminent)
                    .tint(.red)
            }
        }
    }
}

// MARK: - Add instance sheet

private struct AddInstanceSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(InstanceRegistry.self) private var registry

    // Server type + executable
    @State private var serverType = ServerType.mlxLM
    @State private var executablePath = ""
    @State private var isDetectingExecutable = false

    // Model discovery — persisted so the user doesn't have to reselect each time.
    @AppStorage("mlxModelSearchPath") private var modelSearchPath = ProcessInfo.processInfo.environment["HF_HOME"]
        ?? (NSHomeDirectory() + "/.cache/huggingface/hub")
    @State private var scannedModels: [ModelRef] = []
    @State private var isScanningModels = false
    @State private var scanError: String?
    @State private var selectedModelKey: String?

    // Port + name
    @State private var port = "8080"
    @State private var name = ""
    @State private var lastAutoName = ""

    private var executableFound: Bool {
        FileManager.default.isExecutableFile(atPath: executablePath)
    }

    private var portValue: Int? { Int(port) }

    /// True when another instance is already configured on this port.
    /// Advisory only — not a blocker. Multiple instances may share a port
    /// as long as only one runs at a time.
    private var portAlreadyConfigured: Bool {
        guard let p = portValue else { return false }
        return registry.controllers.contains { $0.config.port == p }
    }

    private var selectedModel: ModelRef? {
        scannedModels.first { $0.key == selectedModelKey }
    }

    private var canAdd: Bool {
        guard !name.trimmingCharacters(in: .whitespaces).isEmpty,
              !executablePath.isEmpty,
              let p = portValue, (1...65_535).contains(p) else { return false }
        // mlx-lm cannot launch without a model.
        if serverType == .mlxLM && selectedModelKey == nil { return false }
        return true
    }

    var body: some View {
        VStack(spacing: 0) {
            Form {
                // 1. Server type
                Picker("Server Type", selection: $serverType) {
                    ForEach(ServerType.allCases, id: \.self) { type in
                        Text(type.rawValue).tag(type)
                    }
                }

                // 2. Executable path
                LabeledContent("Executable Path") {
                    HStack(spacing: 6) {
                        TextField("", text: $executablePath, prompt: Text(serverType == .ollama ? "/opt/homebrew/bin/ollama" : "/usr/local/bin/uvx  or  /path/to/venv/python3"))
                            .labelsHidden()
                            .truncationMode(.middle)
                        executableStatusIcon
                    }
                }

                // 3. Model search path (mlx-lm only — Ollama manages its own models)
                if serverType == .mlxLM {
                    LabeledContent("Model Search Path") {
                        HStack(spacing: 6) {
                            TextField("", text: $modelSearchPath)
                                .labelsHidden()
                                .truncationMode(.middle)
                            Button(action: pickModelFolder) {
                                Image(systemName: "folder")
                            }
                            .help("Browse for folder")
                            Button("Scan") { scanModels() }
                                .disabled(isScanningModels)
                        }
                    }
                }

                // 4. Model
                Section("Model") {
                    if isScanningModels {
                        HStack(spacing: 8) {
                            ProgressView().controlSize(.small)
                            Text("Scanning for models…").foregroundStyle(.secondary)
                        }
                    } else if scannedModels.isEmpty {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(scanError ?? emptyModelHint)
                                .foregroundStyle(.secondary)
                                .font(.callout)
                            if serverType == .ollama {
                                Button("Fetch Models") { scanModels() }
                            }
                        }
                    } else {
                        List(scannedModels, id: \.key, selection: $selectedModelKey) { model in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(model.displayName)
                                if let metadata = model.metadata {
                                    Text(metadata.summary)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .tag(model.key)
                        }
                        .frame(minHeight: 180, maxHeight: 320)
                    }
                }

                // 5. Port
                LabeledContent("Port") {
                    VStack(alignment: .trailing, spacing: 2) {
                        TextField("", text: $port)
                            .labelsHidden()
                            .frame(width: 80)
                            .multilineTextAlignment(.trailing)
                        if portAlreadyConfigured {
                            Text("Another instance uses port \(port) — only one can run at a time")
                                .font(.caption)
                                .foregroundStyle(.orange)
                        } else if portValue == nil {
                            Text("Enter a valid port number")
                                .font(.caption)
                                .foregroundStyle(.red)
                        }
                    }
                }

                // 6. Name
                TextField("Name", text: $name, prompt: Text("Auto-generated from model"))
            }
            .formStyle(.grouped)

            Divider()

            HStack {
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button("Add") { addInstance(); dismiss() }
                    .keyboardShortcut(.defaultAction)
                    .disabled(!canAdd)
                    .buttonStyle(.borderedProminent)
            }
            .padding(16)
        }
        .frame(width: 520)
        .onAppear {
            port = String(suggestFreePort(startingAt: defaultPort))
            detectExecutable()
            if serverType == .mlxLM { scanModels() }
        }
        .onChange(of: serverType) { _, newType in
            port = String(suggestFreePort(startingAt: newType == .ollama ? 11_434 : 8_080))
            scannedModels = []
            selectedModelKey = nil
            scanError = nil
            executablePath = ""
            detectExecutable()
            if newType == .mlxLM { scanModels() }
        }
        .onChange(of: selectedModelKey) { _, _ in
            guard let model = selectedModel else { return }
            // Only overwrite the name if the user hasn't customized it.
            if name.isEmpty || name == lastAutoName {
                name = model.displayName
                lastAutoName = model.displayName
            }
        }
    }

    // MARK: Subviews

    @ViewBuilder
    private var executableStatusIcon: some View {
        if isDetectingExecutable {
            ProgressView()
                .controlSize(.small)
        } else if executableFound {
            Image(systemName: "checkmark.circle.fill")
                .foregroundStyle(.green)
                .help("Executable found")
        } else {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.red)
                .help("No executable at this path")
        }
    }

    private var emptyModelHint: String {
        serverType == .ollama
            ? "Ollama manages its own models. Start the Ollama server and fetch the model list, or add the instance and pick a model later."
            : "No models found. Check the search path and scan again."
    }

    // MARK: Logic

    private func pickModelFolder() {
        let panel = NSOpenPanel()
        panel.canChooseFiles = false
        panel.canChooseDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Choose"
        panel.message = "Select the folder containing your MLX models"
        // Pre-open at the current path if it exists.
        if !modelSearchPath.isEmpty {
            panel.directoryURL = URL(fileURLWithPath: modelSearchPath)
        }
        if panel.runModal() == .OK, let url = panel.url {
            modelSearchPath = url.path
            scanModels()
        }
    }

    private var defaultPort: Int {
        serverType == .ollama ? 11_434 : 8_080
    }

    private func suggestFreePort(startingAt base: Int) -> Int {
        let used = Set(registry.controllers.map(\.config.port))
        var candidate = base
        while used.contains(candidate) { candidate += 1 }
        return candidate
    }

    private func detectExecutable() {
        isDetectingExecutable = true
        let type = serverType
        Task {
            let detected: String? = (type == .ollama)
                ? await PathScanner.detectOllama()
                : await PathScanner.detectMLXExecutable()
            guard type == serverType else { return } // type changed mid-flight
            if let detected, executablePath.isEmpty || !executableFound {
                executablePath = detected
            }
            isDetectingExecutable = false
        }
    }

    private func scanModels() {
        isScanningModels = true
        scanError = nil
        let type = serverType
        var tempConfig = ServerInstanceConfig(
            name: "scan",
            type: type,
            port: portValue ?? defaultPort,
            executablePath: executablePath
        )
        if type == .mlxLM {
            tempConfig.modelSearchPaths = [modelSearchPath]
        }
        let config = tempConfig
        Task {
            defer { isScanningModels = false }
            do {
                let models = try await DriverRegistry.driver(for: type).listModels(config: config)
                guard type == serverType else { return }
                scannedModels = models.sorted {
                    $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending
                }
                if models.isEmpty { scanError = nil }
            } catch {
                guard type == serverType else { return }
                scannedModels = []
                scanError = type == .ollama
                    ? "Could not fetch models — is the Ollama server running?"
                    : "Scan failed: \(error.localizedDescription)"
            }
        }
    }

    private func addInstance() {
        var config = ServerInstanceConfig(
            name: name.trimmingCharacters(in: .whitespaces),
            type: serverType,
            port: portValue ?? defaultPort,
            executablePath: executablePath
        )
        if serverType == .mlxLM {
            config.modelSearchPaths = [modelSearchPath]
        }
        config.selectedModelKey = selectedModelKey
        registry.addInstance(config: config, initialModels: scannedModels)
    }
}

// MARK: - General tab

private struct GeneralTab: View {
    @AppStorage("localbar.notificationsEnabled") private var notificationsEnabled = true

    private var appVersion: String {
        let version = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "—"
        let build   = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "—"
        return "\(version) (\(build))"
    }

    var body: some View {
        Form {
            Section("Notifications") {
                Toggle("System notifications", isOn: $notificationsEnabled)
            }

            Section("About") {
                LabeledContent("Version", value: appVersion)

                LabeledContent("Licence") {
                    VStack(alignment: .trailing, spacing: 4) {
                        Text("LocalBar is dual-licensed.")
                            .fontWeight(.medium)
                        Text("Free for personal and open-source use under the GNU General Public Licence v3.0. A commercial licence is required for for-profit organisational deployment or commercial distribution.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.trailing)
                        HStack(spacing: 12) {
                            Link("Full licence terms",
                                 destination: URL(string: "https://github.com/CapitalCantrip/localbar/blob/main/LICENSE")!)
                                .font(.caption)
                            Link("Commercial licensing",
                                 destination: URL(string: "mailto:capitalcantrip@gmail.com")!)
                                .font(.caption)
                        }
                    }
                }

                LabeledContent("Copyright") {
                    Text("© 2026 CapitalCantrip. All rights reserved.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
        .padding()
    }
}

// MARK: - Profiles tab

private struct ProfilesTab: View {
    @Environment(InstanceRegistry.self) private var registry
    @State private var selectedID: UUID?

    private var selectedProfile: NamedProfile? {
        selectedID.flatMap { id in registry.profiles.first { $0.id == id } }
    }

    var body: some View {
        HSplitView {
            // ── Left: profile list ──────────────────────────
            VStack(spacing: 0) {
                List(registry.profiles, id: \.id, selection: $selectedID) { profile in
                    ProfileRowView(profile: profile)
                }
                Divider()
                HStack {
                    Button {
                        let profile = NamedProfile(name: "New Profile", params: ParamValues())
                        registry.addProfile(profile)
                        selectedID = profile.id
                    } label: {
                        Image(systemName: "plus")
                    }
                    .buttonStyle(.borderless)
                    .padding(6)
                    Spacer()
                }
            }
            .frame(minWidth: 180, idealWidth: 200)

            // ── Right: detail ───────────────────────────────
            if let profile = selectedProfile {
                ProfileDetailPanel(profile: profile)
                    .id(profile.id)
            } else {
                VStack(spacing: 8) {
                    Image(systemName: "slider.horizontal.3")
                        .font(.largeTitle)
                        .foregroundStyle(.tertiary)
                    Text("No profile selected")
                        .foregroundStyle(.secondary)
                    Text("Create a profile to save named parameter presets\nyou can apply to any server instance.")
                        .font(.caption)
                        .foregroundStyle(.tertiary)
                        .multilineTextAlignment(.center)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .onChange(of: registry.profiles) { _, newProfiles in
            // If the selected profile was deleted, clear selection.
            if let id = selectedID, !newProfiles.contains(where: { $0.id == id }) {
                selectedID = nil
            }
        }
    }
}

private struct ProfileRowView: View {
    let profile: NamedProfile

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(profile.name)
                .lineLimit(1)
            HStack(spacing: 6) {
                if let type = profile.serverType {
                    Text(type.rawValue)
                        .font(.caption2)
                        .padding(.horizontal, 4)
                        .padding(.vertical, 1)
                        .background(Color.secondary.opacity(0.15))
                        .clipShape(Capsule())
                } else {
                    Text("Any")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                Text(profile.modifiedAt.formatted(date: .abbreviated, time: .omitted))
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(.vertical, 2)
    }
}

// MARK: - Profile detail panel

private struct ProfileDetailPanel: View {
    let profile: NamedProfile
    @Environment(InstanceRegistry.self) private var registry

    @State private var editedName: String = ""
    @State private var editedServerType: ServerType? = nil
    @State private var paramDrafts: [CanonicalParam: String] = [:]
    @State private var boolDrafts: [CanonicalParam: Bool] = [:]
    @State private var systemPromptDraft: String = ""

    private var paramSchema: [ParamDescriptor] {
        if let type = editedServerType {
            return DriverRegistry.driver(for: type).paramSchema
        }
        // Union of all drivers, deduplicated, sorted by raw value for stable order.
        var seen = Set<CanonicalParam>()
        var result: [ParamDescriptor] = []
        for type in ServerType.allCases {
            for d in DriverRegistry.driver(for: type).paramSchema where seen.insert(d.param).inserted {
                result.append(d)
            }
        }
        return result.sorted { $0.param.rawValue < $1.param.rawValue }
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                Text(profile.name)
                    .fontWeight(.medium)
                    .lineLimit(1)
                Spacer()
                Button("Delete", role: .destructive) {
                    registry.removeProfile(id: profile.id)
                }
                .foregroundStyle(.red)
                .buttonStyle(.borderless)
                .font(.caption)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    GroupBox("Profile") {
                        LabeledContent("Name") {
                            TextField("Profile name", text: $editedName)
                                .textFieldStyle(.roundedBorder)
                                .frame(maxWidth: 220)
                        }
                        LabeledContent("Compatible with") {
                            Picker("", selection: $editedServerType) {
                                Text("Any server").tag(Optional<ServerType>.none)
                                ForEach(ServerType.allCases, id: \.self) { type in
                                    Text(type.rawValue).tag(Optional(type))
                                }
                            }
                            .frame(width: 160)
                            .labelsHidden()
                        }
                    }

                    GroupBox("Parameters") {
                        if paramSchema.isEmpty {
                            Text("No configurable parameters.")
                                .font(.callout)
                                .foregroundStyle(.secondary)
                        } else {
                            ForEach(paramSchema) { descriptor in
                                profileParamRow(for: descriptor)
                                if descriptor.id != paramSchema.last?.id { Divider() }
                            }
                        }
                        Divider()
                        profileSystemPromptSection
                    }
                }
                .padding(12)
            }
        }
        .onAppear { initDrafts() }
        .onChange(of: profile.id) { initDrafts() }
        .onChange(of: editedName) { _, newName in
            let trimmed = newName.trimmingCharacters(in: .whitespaces)
            guard !trimmed.isEmpty, trimmed != profile.name else { return }
            var updated = profile; updated.name = trimmed; updated.modifiedAt = Date()
            registry.updateProfile(updated)
        }
        .onChange(of: editedServerType) { _, newType in
            guard newType != profile.serverType else { return }
            var updated = profile; updated.serverType = newType; updated.modifiedAt = Date()
            registry.updateProfile(updated)
        }
    }

    // MARK: Param row

    @ViewBuilder
    private func profileParamRow(for descriptor: ParamDescriptor) -> some View {
        HStack(alignment: .top, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(profileHumanName(descriptor.param))
                    .font(.callout)
                if let note = descriptor.note {
                    Text(note)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }
            Spacer()
            profileParamControl(for: descriptor)
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder
    private func profileParamControl(for descriptor: ParamDescriptor) -> some View {
        let defaultText = descriptor.defaultValue.map { profileValueString($0) }

        switch descriptor.valueType {
        case .bool:
            Toggle("", isOn: Binding(
                get: { boolDrafts[descriptor.param] ?? false },
                set: { newVal in
                    boolDrafts[descriptor.param] = newVal
                    profileCommitBool(descriptor.param, value: newVal)
                }
            ))
            .labelsHidden()
            .frame(width: 44)

        case .double(let range):
            if let range {
                VStack(alignment: .trailing, spacing: 2) {
                    HStack(spacing: 6) {
                        Slider(
                            value: Binding(
                                get: { Double(paramDrafts[descriptor.param] ?? "") ?? range.lowerBound },
                                set: { paramDrafts[descriptor.param] = String(format: "%.3g", $0) }
                            ),
                            in: range,
                            onEditingChanged: { editing in if !editing { profileCommitText(descriptor) } }
                        )
                        .frame(width: 100)
                        TextField(defaultText ?? "default", text: profileParamBinding(descriptor))
                            .textFieldStyle(.roundedBorder)
                            .frame(width: 60)
                            .multilineTextAlignment(.trailing)
                            .onSubmit { profileCommitText(descriptor) }
                    }
                    profileClearButton(for: descriptor)
                }
            } else {
                VStack(alignment: .trailing, spacing: 2) {
                    TextField(defaultText ?? "default", text: profileParamBinding(descriptor))
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 80)
                        .multilineTextAlignment(.trailing)
                        .onSubmit { profileCommitText(descriptor) }
                    profileClearButton(for: descriptor)
                }
            }

        case .int(let range):
            if let range, range.upperBound - range.lowerBound <= 100 {
                VStack(alignment: .trailing, spacing: 2) {
                    Stepper(
                        value: Binding(
                            get: { Int(paramDrafts[descriptor.param] ?? "") ?? range.lowerBound },
                            set: { newVal in
                                paramDrafts[descriptor.param] = String(newVal)
                                profileCommitText(descriptor)
                            }
                        ),
                        in: range
                    ) {
                        Text(paramDrafts[descriptor.param] ?? defaultText ?? "default")
                            .frame(width: 50, alignment: .trailing)
                            .monospacedDigit()
                    }
                    profileClearButton(for: descriptor)
                }
            } else {
                VStack(alignment: .trailing, spacing: 2) {
                    TextField(defaultText ?? "default", text: profileParamBinding(descriptor))
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 80)
                        .multilineTextAlignment(.trailing)
                        .monospacedDigit()
                        .onSubmit { profileCommitText(descriptor) }
                    profileClearButton(for: descriptor)
                }
            }

        case .string:
            VStack(alignment: .trailing, spacing: 2) {
                TextField(defaultText ?? "default", text: profileParamBinding(descriptor))
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 100)
                    .onSubmit { profileCommitText(descriptor) }
                profileClearButton(for: descriptor)
            }
        }
    }

    @ViewBuilder
    private func profileClearButton(for descriptor: ParamDescriptor) -> some View {
        if paramDrafts[descriptor.param] != nil {
            Button("Reset") {
                paramDrafts.removeValue(forKey: descriptor.param)
                var updated = profile
                updated.params.values.removeValue(forKey: descriptor.param)
                updated.modifiedAt = Date()
                registry.updateProfile(updated)
            }
            .font(.caption2)
            .foregroundStyle(.secondary)
        }
    }

    @ViewBuilder
    private var profileSystemPromptSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("System Prompt")
                .font(.callout)
            TextEditor(text: $systemPromptDraft)
                .font(.callout)
                .frame(minHeight: 80, maxHeight: 160)
                .overlay(
                    RoundedRectangle(cornerRadius: 6)
                        .stroke(Color.secondary.opacity(0.3), lineWidth: 1)
                )
            HStack {
                Text("Applied by Ollama via Modelfile; stored for other server types.")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                Spacer()
                let stored = profile.params.systemPrompt ?? ""
                if systemPromptDraft != stored {
                    Button("Save") { profileCommitSystemPrompt() }
                        .font(.caption)
                        .buttonStyle(.borderless)
                        .foregroundStyle(Color.accentColor)
                }
            }
        }
        .padding(.top, 4)
    }

    // MARK: Helpers

    private func profileParamBinding(_ descriptor: ParamDescriptor) -> Binding<String> {
        Binding(
            get: { paramDrafts[descriptor.param] ?? "" },
            set: { newVal in
                paramDrafts[descriptor.param] = newVal.isEmpty ? nil : newVal
                profileCommitText(descriptor)
            }
        )
    }

    private func profileCommitText(_ descriptor: ParamDescriptor) {
        guard let raw = paramDrafts[descriptor.param], !raw.isEmpty else {
            var updated = profile
            updated.params.values.removeValue(forKey: descriptor.param)
            updated.modifiedAt = Date()
            registry.updateProfile(updated)
            return
        }
        var value: ParamValue?
        switch descriptor.valueType {
        case .double:   if let d = Double(raw) { value = .double(d) }
        case .int:      if let i = Int(raw)    { value = .int(i) }
        case .string:   value = .string(raw)
        case .bool:     break
        }
        guard let value else { return }
        var updated = profile
        updated.params.values[descriptor.param] = value
        updated.modifiedAt = Date()
        registry.updateProfile(updated)
    }

    private func profileCommitBool(_ param: CanonicalParam, value: Bool) {
        var updated = profile
        updated.params.values[param] = .bool(value)
        updated.modifiedAt = Date()
        registry.updateProfile(updated)
    }

    private func profileCommitSystemPrompt() {
        var updated = profile
        updated.params.systemPrompt = systemPromptDraft.isEmpty ? nil : systemPromptDraft
        updated.modifiedAt = Date()
        registry.updateProfile(updated)
    }

    private func initDrafts() {
        editedName = profile.name
        editedServerType = profile.serverType
        paramDrafts = [:]
        boolDrafts = [:]
        for descriptor in paramSchema {
            if let val = profile.params.values[descriptor.param] {
                switch val {
                case .double(let d): paramDrafts[descriptor.param] = String(format: "%.3g", d)
                case .int(let i):    paramDrafts[descriptor.param] = String(i)
                case .string(let s): paramDrafts[descriptor.param] = s
                case .bool(let b):   boolDrafts[descriptor.param] = b
                }
            } else if case .bool(let b) = descriptor.defaultValue {
                boolDrafts[descriptor.param] = b
            }
        }
        systemPromptDraft = profile.params.systemPrompt ?? ""
    }

    private func profileHumanName(_ param: CanonicalParam) -> String {
        switch param {
        case .contextLength:   return "Context Length"
        case .temperature:     return "Temperature"
        case .maxTokens:       return "Max Tokens"
        case .topK:            return "Top K"
        case .repeatPenalty:   return "Repeat Penalty"
        case .presencePenalty: return "Presence Penalty"
        case .topP:            return "Top P"
        case .minP:            return "Min P"
        case .seed:            return "Seed"
        case .systemPrompt:    return "System Prompt"
        }
    }

    private func profileValueString(_ value: ParamValue) -> String {
        switch value {
        case .double(let d): return String(format: "%.3g", d)
        case .int(let i):    return String(i)
        case .string(let s): return s
        case .bool(let b):   return b ? "On" : "Off"
        }
    }
}
