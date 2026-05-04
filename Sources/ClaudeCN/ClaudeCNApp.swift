import SwiftUI

@main
struct ClaudeCNApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var delegate
    @StateObject private var appState = AppState()

    var body: some Scene {
        MenuBarExtra {
            MenuBarView()
                .environmentObject(appState)
        } label: {
            Image(systemName: appState.status == .patched ? "globe.asia.australia.fill" : "globe.asia.australia")
            Text("CN")
        }
        .menuBarExtraStyle(.window)
    }
}
