import ChatOSCore
import SwiftUI

struct RemoteConnectionDetailView: View {
    @EnvironmentObject private var model: AppModel
    let connectionID: String
    @StateObject private var viewModel: RemoteConnectionDetailViewModel

    @State private var showingEditor = false
    @State private var showingDeleteConfirmation = false
    @State private var selectedTab: RemoteConnectionWorkspaceTab = .initial

    init(
        connectionID: String,
        useCase: RemoteConnectionManagementUseCase
    ) {
        self.connectionID = connectionID
        _viewModel = StateObject(wrappedValue: RemoteConnectionDetailViewModel(
            connectionID: connectionID,
            useCase: useCase
        ))
    }

    var body: some View {
        Group {
            if let connection {
                VStack(spacing: 0) {
                    VStack(alignment: .leading, spacing: 14) {
                        connectionHeader(connection)
                        if let notice = viewModel.notice {
                            Label(localizedFeedback(notice), systemImage: "checkmark.circle.fill")
                                .foregroundStyle(.green)
                        }
                        if let errorMessage = viewModel.errorMessage {
                            Label(localizedFeedback(errorMessage), systemImage: "exclamationmark.triangle.fill")
                                .foregroundStyle(.orange)
                        }

                        Picker("远端工作区", selection: $selectedTab) {
                            ForEach(RemoteConnectionWorkspaceTab.allCases) { tab in
                                Label(
                                    tab.title(language: model.interfaceLanguage),
                                    systemImage: tab.systemImage
                                )
                                .tag(tab)
                            }
                        }
                        .pickerStyle(.segmented)
                        .frame(maxWidth: 360)
                    }
                    .padding(22)

                    Divider()

                    switch selectedTab {
                    case .terminal:
                        RemoteTerminalWorkspaceView(
                            viewModel: model.remoteConnectionWorkspaceStore
                                .terminalWorkspace(for: connection)
                        )
                        .id(connection.id)
                    case .files:
                        RemoteSFTPWorkspaceView(
                            viewModel: model.remoteConnectionWorkspaceStore
                                .fileWorkspace(for: connection.id)
                        )
                        .id(connection.id)
                    case .details:
                        ScrollView {
                            connectionDetails(connection)
                                .padding(22)
                        }
                    }
                }
                .navigationTitle(connection.name)
                .toolbar { toolbar(connection) }
            } else {
                ContentUnavailableView(
                    "远端连接不存在",
                    systemImage: "network.slash",
                    description: Text("它可能已经被删除，请刷新资源列表。")
                )
            }
        }
        .workspaceFill()
        .sheet(isPresented: $showingEditor) {
            RemoteConnectionEditorSheetHost(
                editingConnection: connection,
                connections: model.remoteConnections,
                useCase: model.remoteConnectionManagementUseCase,
                onSaved: model.registerRemoteConnection
            )
        }
        .sheet(isPresented: verificationPresented) { verificationSheet }
        .confirmationDialog(
            "删除远端连接？",
            isPresented: $showingDeleteConfirmation,
            titleVisibility: .visible
        ) {
            Button("删除", role: .destructive) {
                viewModel.deleteConnection()
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("只会删除 ChatOS 保存的连接配置，不会修改远端服务器。")
        }
        .onChange(of: viewModel.deletedConnectionID) { _, deletedConnectionID in
            guard let deletedConnectionID else { return }
            model.unregisterRemoteConnection(id: deletedConnectionID)
            viewModel.consumeDeletedConnection()
        }
        .onDisappear(perform: viewModel.deactivate)
    }

    private var connection: RemoteConnection? { model.remoteConnection(id: connectionID) }

    private func connectionHeader(_ connection: RemoteConnection) -> some View {
        HStack(spacing: 14) {
            Image(systemName: "network.badge.shield.half.filled")
                .appFont(.title)
                .foregroundStyle(Color.accentColor)
                .frame(width: 48, height: 48)
                .background(Color.accentColor.opacity(0.1), in: RoundedRectangle(cornerRadius: 12))
            VStack(alignment: .leading, spacing: 4) {
                Text(connection.name).appFont(.title2.weight(.semibold))
                Text("\(connection.username)@\(connection.host):\(connection.port)")
                    .appFont(.body)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            StatusCapsule(
                title: viewModel.isTesting
                    ? model.localized("测试中", english: "Testing")
                    : model.localized("已保存", english: "Saved"),
                color: viewModel.isTesting ? .orange : .green
            )
        }
    }

    private func connectionDetails(_ connection: RemoteConnection) -> some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(spacing: 10) {
                Button {
                    selectedTab = .terminal
                } label: {
                    Label(
                        model.localized("连接终端", english: "Connect Terminal"),
                        systemImage: "terminal"
                    )
                }
                .buttonStyle(.borderedProminent)

                Button {
                    selectedTab = .files
                } label: {
                    Label(
                        model.localized("打开 SFTP", english: "Open SFTP"),
                        systemImage: "externaldrive.connected.to.line.below"
                    )
                }
                .buttonStyle(.bordered)
            }

            Grid(alignment: .leading, horizontalSpacing: 26, verticalSpacing: 12) {
                detailRow(
                    model.localized("认证方式", english: "Authentication"),
                    authenticationLabel(connection.authenticationType)
                )
                detailRow(
                    model.localized("主机密钥", english: "Host Key"),
                    connection.hostKeyPolicy == .strict
                        ? model.localized("严格校验", english: "Strict Verification")
                        : model.localized("首次接受新密钥", english: "Accept New Key on First Connection")
                )
                detailRow(
                    model.localized("默认目录", english: "Default Folder"),
                    connection.defaultRemotePath ?? model.localized("登录目录", english: "Login Folder")
                )
                detailRow(
                    model.localized("执行位置", english: "Execution Location"),
                    model.localized("这台 Mac · ChatOS 客户端", english: "This Mac · ChatOS Client")
                )
                detailRow(
                    model.localized("跳板机", english: "Jump Host"),
                    connection.jumpEnabled
                        ? jumpLabel(connection)
                        : model.localized("未启用", english: "Disabled")
                )
            }
        }
        .padding(18)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14).stroke(AppPalette.border, lineWidth: 1)
        }
    }

    private func detailRow(_ title: String, _ value: String) -> some View {
        GridRow {
            Text(title).appFont(.caption.weight(.semibold)).foregroundStyle(.secondary)
            Text(value).appFont(.body).textSelection(.enabled)
        }
    }

    @ToolbarContentBuilder
    private func toolbar(_ connection: RemoteConnection) -> some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            Button {
                viewModel.testSavedConnection(code: nil)
            } label: {
                Label("测试连接", systemImage: "bolt.horizontal.circle")
            }
            .disabled(viewModel.isBusy)
            Button("编辑", systemImage: "pencil") { showingEditor = true }
                .disabled(viewModel.isBusy)
            Menu {
                Button("删除连接", systemImage: "trash", role: .destructive) {
                    showingDeleteConfirmation = true
                }
                .disabled(viewModel.isBusy)
            } label: {
                Image(systemName: "ellipsis.circle")
            }
        }
    }

    private var verificationPresented: Binding<Bool> {
        Binding(
            get: { viewModel.verificationPrompt != nil },
            set: { if !$0 { viewModel.verificationPrompt = nil } }
        )
    }

    private var verificationSheet: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("SSH 二次验证").appFont(.title3.weight(.semibold))
            Text(viewModel.verificationPrompt ?? model.localized("请输入验证码。", english: "Enter the verification code."))
                .appFont(.body).foregroundStyle(.secondary)
            TextField("验证码", text: $viewModel.verificationCode).textFieldStyle(.roundedBorder)
            HStack {
                Spacer()
                Button("取消") { viewModel.verificationPrompt = nil }
                Button("验证") {
                    viewModel.testSavedConnection(code: viewModel.verificationCode)
                }
                .buttonStyle(.borderedProminent)
                .disabled(
                    viewModel.isTesting
                        || viewModel.verificationCode
                            .trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                )
            }
        }
        .padding(22)
        .frame(width: 440)
    }

    private func authenticationLabel(_ type: RemoteAuthenticationType) -> String {
        switch type {
        case .privateKey: model.localized("私钥", english: "Private Key")
        case .privateKeyCertificate: model.localized("私钥 + 证书", english: "Private Key + Certificate")
        case .password: model.localized("密码", english: "Password")
        }
    }

    private func jumpLabel(_ connection: RemoteConnection) -> String {
        if let jumpID = connection.jumpConnectionID,
           let jump = model.remoteConnection(id: jumpID) {
            return "\(jump.name)（\(jump.username)@\(jump.host)）"
        }
        if let host = connection.jumpHost { return host }
        return model.localized("已启用", english: "Enabled")
    }

    private func localizedFeedback(_ message: String) -> String {
        switch message {
        case "连接测试成功。":
            model.localized("连接测试成功。", english: "Connection test succeeded.")
        case "请输入验证码。":
            model.localized("请输入验证码。", english: "Enter the verification code.")
        case "验证码未通过，请重新输入。":
            model.localized(
                "验证码未通过，请重新输入。",
                english: "The verification code was rejected. Try again."
            )
        default:
            message
        }
    }
}

enum RemoteConnectionWorkspaceTab: String, CaseIterable, Identifiable {
    case terminal = "远程终端"
    case files = "SFTP 文件"
    case details = "连接信息"

    var id: Self { self }

    static let initial: Self = .details

    func title(language: ChatOSLanguage) -> String {
        guard language == .english else { return rawValue }
        return switch self {
        case .terminal: "Remote Terminal"
        case .files: "SFTP Files"
        case .details: "Connection Details"
        }
    }

    var systemImage: String {
        switch self {
        case .terminal: "terminal"
        case .files: "externaldrive.connected.to.line.below"
        case .details: "info.circle"
        }
    }
}
