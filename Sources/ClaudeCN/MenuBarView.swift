import SwiftUI

struct MenuBarView: View {
    @EnvironmentObject var appState: AppState

    var body: some View {
        VStack(spacing: 0) {
            headerSection
            Divider()
            statusSection
            Divider()
            actionSection
            Divider()
            footerSection
        }
        .frame(width: 280)
    }

    private var headerSection: some View {
        HStack {
            Image(systemName: "globe.asia.australia.fill")
                .font(.title2)
                .foregroundColor(.accentColor)
            VStack(alignment: .leading, spacing: 2) {
                Text("Claude 汉化助手")
                    .font(.headline)
                Text("v1.0.0")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            Spacer()
        }
        .padding(12)
    }

    private var statusSection: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("状态")
                    .foregroundColor(.secondary)
                Spacer()
                statusBadge
            }
            if !appState.claudeVersion.isEmpty {
                HStack {
                    Text("版本")
                        .foregroundColor(.secondary)
                    Spacer()
                    Text(appState.claudeVersion)
                        .font(.caption)
                }
            }
        }
        .padding(12)
    }

    private var statusBadge: some View {
        HStack(spacing: 4) {
            Circle()
                .fill(statusColor)
                .frame(width: 8, height: 8)
            Text(appState.status.rawValue)
                .font(.caption)
        }
    }

    private var statusColor: Color {
        switch appState.status {
        case .patched: return .green
        case .unpatched: return .orange
        case .notInstalled: return .red
        case .checking: return .gray
        }
    }

    private var actionSection: some View {
        VStack(spacing: 8) {
            if appState.isProcessing {
                HStack(spacing: 8) {
                    ProgressView()
                        .scaleEffect(0.7)
                    Text(appState.progressMessage)
                        .font(.caption)
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 4)
            } else {
                switch appState.status {
                case .unpatched:
                    Button(action: { Task { await appState.applyPatch() } }) {
                        Label("一键汉化", systemImage: "arrow.down.circle.fill")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)

                case .patched:
                    Button(action: { Task { await appState.applyPatch() } }) {
                        Label("重新汉化", systemImage: "arrow.clockwise.circle.fill")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.large)

                    Button(action: { Task { await appState.removePatch() } }) {
                        Label("恢复原版", systemImage: "arrow.uturn.backward.circle")
                            .frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                    .controlSize(.regular)

                case .notInstalled:
                    Text("请先安装 Claude Desktop")
                        .font(.caption)
                        .foregroundColor(.secondary)

                case .checking:
                    ProgressView()
                }
            }
        }
        .padding(12)
    }

    private var footerSection: some View {
        HStack {
            Button("刷新状态") {
                appState.checkStatus()
            }
            .font(.caption)
            Spacer()
            Button("退出") {
                NSApplication.shared.terminate(nil)
            }
            .font(.caption)
        }
        .padding(12)
    }
}
