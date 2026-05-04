import AppKit
import Combine
import SwiftUI

@MainActor
class AppDelegate: NSObject, NSApplicationDelegate {
    private var statusItem: NSStatusItem!
    private var popover: NSPopover!
    private let appState = AppState()
    private var eventMonitor: Any?
    private var spinnerTimer: Timer?
    private var spinnerFrame = 0
    private var cancellables = Set<AnyCancellable>()

    private static let spinnerSymbols = [
        "circle.dotted",
        "circle.bottomhalf.filled",
        "circle.lefthalf.filled",
        "circle.tophalf.filled",
        "circle.righthalf.filled",
    ]

    func applicationDidFinishLaunching(_ notification: Notification) {
        let bundleID = Bundle.main.bundleIdentifier ?? ""
        let running = NSRunningApplication.runningApplications(withBundleIdentifier: bundleID)
        if running.count > 1 {
            NSApp.terminate(nil)
        }

        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        updateStatusIcon()

        if let button = statusItem.button {
            button.action = #selector(togglePopover)
            button.target = self
        }

        popover = NSPopover()
        popover.contentSize = NSSize(width: 280, height: 10)
        popover.behavior = .transient
        popover.contentViewController = NSHostingController(
            rootView: MenuBarView()
                .environmentObject(appState)
        )

        appState.onStatusChange = { [weak self] in
            self?.updateStatusIcon()
        }

        appState.onClosePanel = { [weak self] in
            self?.closePopover()
        }

        appState.onShowPanel = { [weak self] in
            self?.showPopover()
        }

        appState.$isProcessing
            .receive(on: RunLoop.main)
            .sink { [weak self] processing in
                if processing {
                    self?.startSpinner()
                } else {
                    self?.stopSpinner()
                }
            }
            .store(in: &cancellables)

        eventMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { [weak self] _ in
            guard let self, self.popover.isShown, !self.appState.isProcessing else { return }
            self.closePopover()
        }
    }

    deinit {
        if let monitor = eventMonitor {
            NSEvent.removeMonitor(monitor)
        }
    }

    private func updateStatusIcon() {
        guard let button = statusItem?.button else { return }
        let imageName = appState.status == .patched
            ? "globe.asia.australia.fill"
            : "globe.asia.australia"
        button.image = NSImage(systemSymbolName: imageName, accessibilityDescription: "ClaudeCN")
        button.title = " CN"
    }

    private func startSpinner() {
        spinnerFrame = 0
        tickSpinner()
        spinnerTimer = Timer.scheduledTimer(withTimeInterval: 0.3, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.tickSpinner() }
        }
    }

    private func tickSpinner() {
        guard let button = statusItem?.button else { return }
        let symbol = Self.spinnerSymbols[spinnerFrame % Self.spinnerSymbols.count]
        button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: "处理中")
        button.title = " \(appState.progressMessage)"
        spinnerFrame += 1
    }

    private func stopSpinner() {
        spinnerTimer?.invalidate()
        spinnerTimer = nil
        updateStatusIcon()
    }

    @objc private func togglePopover() {
        if popover.isShown {
            closePopover()
        } else {
            showPopover()
        }
    }

    private func showPopover() {
        guard let button = statusItem.button else { return }
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        popover.contentViewController?.view.window?.makeKey()
    }

    private func closePopover() {
        popover.performClose(nil)
    }
}
