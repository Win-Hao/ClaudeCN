import AppKit
import Combine
import UserNotifications

enum PatchStatus: String {
    case notInstalled = "未检测到 Claude Desktop"
    case unpatched = "未汉化"
    case patched = "已汉化"
    case checking = "检测中..."
}

@MainActor
class AppState: ObservableObject {
    @Published var status: PatchStatus = .checking
    @Published var claudeVersion: String = ""
    @Published var isProcessing = false
    @Published var progressMessage = ""
    private var patcher = ClaudePatcher()
    private var updateTimer: Timer?

    var onStatusChange: (() -> Void)?
    var onClosePanel: (() -> Void)?
    var onShowPanel: (() -> Void)?

    init() {
        checkStatus()
        startUpdateMonitor()
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound]) { _, _ in }
    }

    func checkStatus() {
        let oldStatus = status
        status = .checking
        let claudePath = ClaudePatcher.claudeAppPath

        guard FileManager.default.fileExists(atPath: claudePath) else {
            status = .notInstalled
            claudeVersion = ""
            if status != oldStatus { onStatusChange?() }
            return
        }

        claudeVersion = patcher.getClaudeVersion() ?? "未知版本"
        status = patcher.isPatched() ? .patched : .unpatched
        if status != oldStatus { onStatusChange?() }
    }

    func applyPatch() async {
        guard !isProcessing else { return }
        isProcessing = true
        progressMessage = "正在退出 Claude Desktop..."

        onClosePanel?()

        do {
            try await Task.sleep(nanoseconds: 100_000_000)

            patcher.quitClaude()
            try await waitForClaudeToQuit()

            progressMessage = "正在备份并打补丁（需要输入密码）..."

            var p = patcher
            try await runInBackground { try p.applyPatch() }
            patcher = p

            progressMessage = "正在重启 Claude Desktop..."
            try await Task.sleep(nanoseconds: 500_000_000)
            patcher.launchClaude()
            try await Task.sleep(nanoseconds: 2_000_000_000)

            checkStatus()
            isProcessing = false
            progressMessage = ""

            if status == .patched {
                sendNotification("汉化完成", "Claude Desktop 已切换为中文界面。")
            } else {
                onShowPanel?()
                sendNotification("汉化异常", "补丁已应用但验证未通过，请尝试重新汉化。")
            }
        } catch {
            isProcessing = false
            progressMessage = ""
            checkStatus()
            onShowPanel?()
            sendNotification("汉化失败", error.localizedDescription)
        }
    }

    func removePatch() async {
        guard !isProcessing else { return }
        isProcessing = true
        progressMessage = "正在退出 Claude Desktop..."

        onClosePanel?()

        do {
            try await Task.sleep(nanoseconds: 100_000_000)

            patcher.quitClaude()
            try await waitForClaudeToQuit()

            progressMessage = "正在恢复原版..."
            let p = patcher
            try await runInBackground { try p.restore() }

            progressMessage = "正在重启 Claude Desktop..."
            patcher.launchClaude()
            try await Task.sleep(nanoseconds: 2_000_000_000)

            status = .unpatched
            isProcessing = false
            progressMessage = ""
            onStatusChange?()
            sendNotification("恢复完成", "Claude Desktop 已恢复为原版。")
        } catch {
            isProcessing = false
            progressMessage = ""
            checkStatus()
            onShowPanel?()
            sendNotification("恢复失败", error.localizedDescription)
        }
    }

    private func waitForClaudeToQuit() async throws {
        for _ in 0..<30 {
            try await Task.sleep(nanoseconds: 500_000_000)
            if !patcher.isClaudeRunning() { return }
        }
        NSRunningApplication.runningApplications(withBundleIdentifier: "com.anthropic.Claude")
            .forEach { $0.forceTerminate() }
        try await Task.sleep(nanoseconds: 1_000_000_000)
    }

    private func runInBackground(_ work: @escaping @Sendable () throws -> Void) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            DispatchQueue.global(qos: .userInitiated).async {
                do {
                    try work()
                    continuation.resume()
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }

    private func startUpdateMonitor() {
        updateTimer = Timer.scheduledTimer(withTimeInterval: 300, repeats: true) { [weak self] _ in
            Task { @MainActor in
                self?.checkStatus()
            }
        }
    }

    private func sendNotification(_ title: String, _ message: String) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = message
        content.sound = .default
        let request = UNNotificationRequest(identifier: UUID().uuidString, content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }
}
