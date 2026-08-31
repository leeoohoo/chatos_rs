import ChatOSCore
import SwiftUI

struct LocalConnectorTerminalView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel
    @State private var command = "pwd"
    @State private var selectedWorkspaceID = ""

    var body: some View {
        VStack(spacing: 0) {
            terminal
            Divider()
            history
                .frame(minHeight: 190, idealHeight: 230)
        }
        .onAppear(perform: selectDefaultWorkspace)
        .onChange(of: viewModel.status?.workspaces) { selectDefaultWorkspace() }
    }

    private var terminal: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Picker("工作区", selection: $selectedWorkspaceID) {
                    ForEach(workspaces) { workspace in
                        Text(workspace.alias).tag(workspace.id)
                    }
                }
                .frame(maxWidth: 260)
                Spacer()
                if let workspace = selectedWorkspace {
                    Text(workspace.absoluteRoot)
                        .appFont(.caption.monospaced())
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .textSelection(.enabled)
                }
            }
            .padding(14)
            .background(.bar)

            ScrollView {
                Text(outputText)
                    .appFont(.system(size: 13, design: .monospaced))
                    .foregroundStyle(outputColor)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .topLeading)
                    .padding(18)
            }
            .background(Color(nsColor: .textBackgroundColor))

            HStack(alignment: .center, spacing: 10) {
                Text("❯")
                    .appFont(.system(.body, design: .monospaced).weight(.bold))
                    .foregroundStyle(.green)
                TextField("输入命令", text: $command)
                    .textFieldStyle(.plain)
                    .appFont(.system(.body, design: .monospaced))
                    .onSubmit(run)
                Button("执行", systemImage: "play.fill", action: run)
                    .buttonStyle(.borderedProminent)
                    .disabled(selectedWorkspace == nil || viewModel.isPerformingAction)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .background(.bar)
        }
    }

    private var history: some View {
        VStack(spacing: 0) {
            HStack {
                Label("执行历史", systemImage: "clock.arrow.circlepath")
                    .appFont(.headline)
                Text("\(viewModel.commandHistory.count)")
                    .appFont(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                Spacer()
                Button("清空", role: .destructive) { viewModel.clearCommandHistory() }
                    .disabled(viewModel.commandHistory.isEmpty)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            List(viewModel.commandHistory) { entry in
                HStack(alignment: .top, spacing: 12) {
                    Image(systemName: historyIcon(entry.status))
                        .foregroundStyle(historyColor(entry.status))
                        .frame(width: 18)
                    VStack(alignment: .leading, spacing: 4) {
                        Text(entry.display)
                            .appFont(.system(.callout, design: .monospaced))
                            .lineLimit(2)
                            .textSelection(.enabled)
                        HStack(spacing: 8) {
                            Text(entry.workspaceAlias ?? entry.cwd ?? model.localized("本机", english: "Local"))
                            Text(entry.startedAt)
                            if let exitCode = entry.exitCode {
                                Text("exit \(exitCode)")
                            }
                        }
                        .appFont(.caption2)
                        .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text(displayHistoryStatus(entry.status))
                        .appFont(.caption.weight(.medium))
                        .foregroundStyle(historyColor(entry.status))
                }
                .padding(.vertical, 3)
            }
            .listStyle(.inset)
        }
    }

    private var workspaces: [LocalConnectorWorkspace] {
        viewModel.status?.workspaces ?? []
    }

    private var selectedWorkspace: LocalConnectorWorkspace? {
        workspaces.first(where: { $0.id == selectedWorkspaceID })
    }

    private var outputText: String {
        guard let result = viewModel.terminalResult else {
            return model.localized(
                "ChatOS 本机终端\n命令会经过与任务执行相同的权限与审批链路。",
                english: "ChatOS Local Terminal\nCommands use the same permission and approval flow as task execution."
            )
        }
        let commandLine = ([result.command] + result.args).joined(separator: " ")
        let output = [result.stdout, result.stderr, result.error]
            .compactMap { $0?.trimmingCharacters(in: .newlines) }
            .filter { !$0.isEmpty }
            .joined(separator: "\n")
        let displayedOutput = output.isEmpty
            ? model.localized("（没有输出）", english: "(No output)")
            : output
        return model.localized(
            "$ \(commandLine)\n\(displayedOutput)\n\n进程退出：\(result.exitCode.map(String.init) ?? "—")",
            english: "$ \(commandLine)\n\(displayedOutput)\n\nProcess exited: \(result.exitCode.map(String.init) ?? "—")"
        )
    }

    private var outputColor: Color {
        viewModel.terminalResult?.success == false ? .primary : .primary
    }

    private func selectDefaultWorkspace() {
        guard !workspaces.contains(where: { $0.id == selectedWorkspaceID }) else { return }
        selectedWorkspaceID = viewModel.status?.defaultWorkspaceID ?? workspaces.first?.id ?? ""
    }

    private func run() {
        guard let workspace = selectedWorkspace else { return }
        viewModel.runTerminal(
            commandLine: command,
            workspaceID: workspace.id,
            cwd: workspace.absoluteRoot
        )
    }

    private func historyIcon(_ status: String) -> String {
        status.lowercased().contains("success") || status.lowercased().contains("completed")
            ? "checkmark.circle.fill" : "xmark.circle.fill"
    }

    private func historyColor(_ status: String) -> Color {
        status.lowercased().contains("success") || status.lowercased().contains("completed")
            ? .green : .orange
    }

    private func displayHistoryStatus(_ status: String) -> String {
        switch status.lowercased() {
        case "success", "completed", "succeeded": model.localized("已完成", english: "Completed")
        case "running", "processing": model.localized("执行中", english: "Running")
        case "failed", "error": model.localized("失败", english: "Failed")
        case "cancelled", "canceled": model.localized("已取消", english: "Cancelled")
        default: status
        }
    }
}
