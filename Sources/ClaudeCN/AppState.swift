import Foundation
import Combine

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
    @Published var showAlert = false
    @Published var alertTitle = ""
    @Published var alertMessage = ""

    private var patcher = ClaudePatcher()
    private var updateTimer: Timer?

    init() {
        checkStatus()
        startUpdateMonitor()
    }

    func checkStatus() {
        status = .checking
        let claudePath = ClaudePatcher.claudeAppPath

        guard FileManager.default.fileExists(atPath: claudePath) else {
            status = .notInstalled
            claudeVersion = ""
            return
        }

        claudeVersion = patcher.getClaudeVersion() ?? "未知版本"
        status = patcher.isPatched() ? .patched : .unpatched
    }

    func applyPatch() async {
        isProcessing = true
        progressMessage = "正在退出 Claude Desktop..."

        do {
            try await Task.sleep(nanoseconds: 100_000_000)

            patcher.quitClaude()
            try await Task.sleep(nanoseconds: 2_000_000_000)

            progressMessage = "正在备份并打补丁（需要输入密码）..."

            var p = patcher
            try await runInBackground { try p.applyPatch() }
            patcher = p

            progressMessage = "正在设置语言并重启..."
            try patcher.writeLocaleConfig()
            try await Task.sleep(nanoseconds: 500_000_000)
            patcher.launchClaude()
            try await Task.sleep(nanoseconds: 2_000_000_000)

            status = .patched
            isProcessing = false
            progressMessage = ""
            showSuccess("汉化完成", "Claude Desktop 已切换为中文界面。")
        } catch {
            showError("汉化失败", error.localizedDescription)
            isProcessing = false
            progressMessage = ""
        }
    }

    func removePatch() async {
        isProcessing = true
        progressMessage = "正在退出 Claude Desktop..."

        do {
            try await Task.sleep(nanoseconds: 100_000_000)

            patcher.quitClaude()
            try await Task.sleep(nanoseconds: 2_000_000_000)

            progressMessage = "正在恢复原版..."
            let p = patcher
            try await runInBackground { try p.restore() }

            progressMessage = "正在重启 Claude Desktop..."
            patcher.launchClaude()
            try await Task.sleep(nanoseconds: 2_000_000_000)

            status = .unpatched
            isProcessing = false
            progressMessage = ""
            showSuccess("恢复完成", "Claude Desktop 已恢复为原版。")
        } catch {
            showError("恢复失败", error.localizedDescription)
            isProcessing = false
            progressMessage = ""
        }
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

    private func showSuccess(_ title: String, _ message: String) {
        alertTitle = title
        alertMessage = message
        showAlert = true
    }

    private func showError(_ title: String, _ message: String) {
        alertTitle = title
        alertMessage = message
        showAlert = true
    }
}
