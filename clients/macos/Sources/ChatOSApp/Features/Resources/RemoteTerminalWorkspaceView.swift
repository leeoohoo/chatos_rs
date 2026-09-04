import ChatOSCore
import SwiftUI

struct RemoteTerminalWorkspaceView: View {
    @EnvironmentObject private var model: AppModel
    @ObservedObject private var viewModel: RemoteTerminalWorkspaceViewModel

    init(viewModel: RemoteTerminalWorkspaceViewModel) {
        self.viewModel = viewModel
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                Label(
                    viewModel.terminal.workingDirectory,
                    systemImage: "network"
                )
                .appFont(.caption.monospaced())
                .lineLimit(1)
                .truncationMode(.middle)
                Spacer()
                StatusCapsule(
                    title: viewModel.connectionLabel(language: model.interfaceLanguage),
                    color: viewModel.connectionColor
                )
                Button("断开", systemImage: "stop.circle", action: viewModel.disconnect)
                    .labelStyle(.iconOnly)
                    .help("断开远程终端")
                    .disabled(!viewModel.isConnected)
            }
            .padding(.horizontal, 18)
            .frame(height: 48)
            .background(AppPalette.canvas)

            Divider()

            TerminalSessionView(
                terminal: viewModel.terminal,
                onSubmit: viewModel.submit,
                showsHeader: false
            )
        }
        .onAppear {
            viewModel.interfaceLanguage = model.interfaceLanguage
            viewModel.activate()
        }
        .onChange(of: model.interfaceLanguage) { _, language in
            viewModel.interfaceLanguage = language
        }
        .onDisappear(perform: viewModel.disconnect)
    }
}

@MainActor
final class RemoteTerminalWorkspaceViewModel: ObservableObject {
    enum ConnectionState {
        case ready
        case connected
        case disconnected
    }

    @Published private(set) var state: ConnectionState = .ready
    var interfaceLanguage: ChatOSLanguage = .simplifiedChinese
    let terminal: TerminalViewModel

    private let idleTimeout: Duration
    private var idleTask: Task<Void, Never>?

    init(
        connection: RemoteConnection,
        service: any RemoteTerminalCommandServicing,
        idleTimeout: Duration = .seconds(10 * 60)
    ) {
        self.idleTimeout = idleTimeout
        let initialDirectory = connection.defaultRemotePath?.remoteTerminalNonEmpty ?? "~"
        self.terminal = TerminalViewModel(
            workingDirectory: initialDirectory,
            executor: RemoteTerminalCommandExecutor(
                connectionID: connection.id,
                service: service
            )
        )
    }

    var isConnected: Bool { state == .connected }

    func connectionLabel(language: ChatOSLanguage) -> String {
        switch state {
        case .ready: language == .english ? "Connect on First Command" : "输入命令后连接"
        case .connected: language == .english ? "Connected" : "已连接"
        case .disconnected: language == .english ? "Disconnected" : "已断开"
        }
    }

    var connectionColor: Color {
        switch state {
        case .ready: .secondary
        case .connected: .green
        case .disconnected: .orange
        }
    }

    func activate() {
        if state == .disconnected { state = .ready }
    }

    func submit() {
        guard !terminal.command.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        if state == .disconnected {
            terminal.appendSystemLine(localized(
                "正在重新连接远端服务器…",
                english: "Reconnecting to the remote server…"
            ))
        }
        state = .connected
        terminal.submit()
        scheduleIdleDisconnect()
    }

    func disconnect() {
        idleTask?.cancel()
        idleTask = nil
        guard state == .connected else { return }
        state = .disconnected
        terminal.appendSystemLine(localized(
            "远程终端已断开。再次输入命令时会自动重新连接。",
            english: "The remote terminal was disconnected. It will reconnect when you enter another command."
        ))
    }

    private func scheduleIdleDisconnect() {
        idleTask?.cancel()
        let timeout = idleTimeout
        idleTask = Task { [weak self] in
            try? await Task.sleep(for: timeout)
            guard !Task.isCancelled else { return }
            self?.disconnectAfterIdle()
        }
    }

    private func disconnectAfterIdle() {
        guard state == .connected, !terminal.isRunning else {
            if terminal.isRunning { scheduleIdleDisconnect() }
            return
        }
        state = .disconnected
        terminal.appendSystemLine(localized(
            "远程终端已因 10 分钟无操作自动断开。",
            english: "The remote terminal disconnected after 10 minutes of inactivity."
        ))
    }

    private func localized(_ chinese: String, english: String) -> String {
        interfaceLanguage == .english ? english : chinese
    }
}

private struct RemoteTerminalCommandExecutor: TerminalCommandExecuting {
    let connectionID: String
    let service: any RemoteTerminalCommandServicing

    func execute(command: String, workingDirectory: String) async -> TerminalCommandResult {
        do {
            let result = try await service.executeRemoteCommand(
                connectionID: connectionID,
                command: command,
                workingDirectory: workingDirectory
            )
            return .init(
                output: result.output,
                error: result.error,
                exitCode: result.exitCode,
                workingDirectory: result.workingDirectory
            )
        } catch {
            return .init(output: "", error: error.localizedDescription, exitCode: -1)
        }
    }
}

private extension String {
    var remoteTerminalNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
