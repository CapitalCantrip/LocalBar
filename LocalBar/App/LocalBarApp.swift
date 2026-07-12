import SwiftUI

@main
struct LocalBarApp: App {
    @State private var registry = InstanceRegistry()
    // Guard so adoptRunningServers() runs exactly once at launch, not every
    // time the MenuBarExtra popover opens (the .task re-fires on each appear).
    @State private var hasAdoptedOnLaunch = false

    var body: some Scene {
        MenuBarExtra {
            MenuBarView()
                .environment(registry)
                .task {
                    guard !hasAdoptedOnLaunch else { return }
                    hasAdoptedOnLaunch = true
                    await registry.bootstrap()
                    await registry.adoptRunningServers()
                }
        } label: {
            MenuBarIconView(registry: registry)
        }
        .menuBarExtraStyle(.window)

        Settings {
            SettingsView()
                .environment(registry)
        }
        .windowResizability(.contentMinSize)
    }
}
