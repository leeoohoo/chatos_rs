import ChatOSCore
import SwiftUI

struct RemoteConnectionDetailView: View {
    @EnvironmentObject private var model: AppModel
    let connectionID: String

    @State private var showingEditor = false
    @State private var showingDeleteConfirmation = false
    @State private var isTesting = false
    @State private var notice: String?
    @State private var errorMessage: String?
    @State private var verificationPrompt: String?
    @State private var verificationCode = ""
    @State private var selectedTab: RemoteConnectionWorkspaceTab = .terminal

    var body: some View {
        Group {
            if let connection {
                VStack(spacing: 0) {
                    VStack(alignment: .leading, spacing: 14) {
                        connectionHeader(connection)
                        if let notice {
                            Label(notice, systemImage: "checkmark.circle.fill")
                                .foregroundStyle(.green)
                        }
                        if let errorMessage {
                            Label(errorMessage, systemImage: "exclamationmark.triangle.fill")
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
                            connection: connection,
                            service: model.remoteConnectionService
                        )
                    case .files:
                        RemoteSFTPWorkspaceView(
                            connectionID: connection.id,
                            service: model.remoteFileService
                        )
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
                service: model.remoteConnectionService,
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
                Task {
                    do { try await model.deleteRemoteConnection(id: connectionID) }
                    catch { errorMessage = error.localizedDescription }
                }
            }
            Button("取消", role: .cancel) {}
        } message: {
            Text("只会删除 ChatOS 保存的连接配置，不会修改远端服务器。")
        }
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
                title: isTesting
                    ? model.localized("测试中", english: "Testing")
                    : model.localized("已保存", english: "Saved"),
                color: isTesting ? .orange : .green
            )
        }
    }

    private func connectionDetails(_ connection: RemoteConnection) -> some View {
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
                Task { await testSavedConnection(code: nil) }
            } label: {
                Label("测试连接", systemImage: "bolt.horizontal.circle")
            }
            .disabled(isTesting)
            Button("编辑", systemImage: "pencil") { showingEditor = true }
            Menu {
                Button("删除连接", systemImage: "trash", role: .destructive) {
                    showingDeleteConfirmation = true
                }
            } label: {
                Image(systemName: "ellipsis.circle")
            }
        }
    }

    private func testSavedConnection(code: String?) async {
        isTesting = true
        notice = nil
        errorMessage = nil
        do {
            let result = try await model.remoteConnectionService.testSaved(
                id: connectionID,
                verificationCode: code
            )
            notice = result.message?.nonEmpty
                ?? model.localized("连接测试成功。", english: "Connection test succeeded.")
            verificationPrompt = nil
            verificationCode = ""
        } catch let challenge as RemoteVerificationChallenge {
            verificationPrompt = challenge.prompt
            verificationCode = ""
        } catch {
            errorMessage = error.localizedDescription
        }
        isTesting = false
    }

    private var verificationPresented: Binding<Bool> {
        Binding(
            get: { verificationPrompt != nil },
            set: { if !$0 { verificationPrompt = nil } }
        )
    }

    private var verificationSheet: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("SSH 二次验证").appFont(.title3.weight(.semibold))
            Text(verificationPrompt ?? model.localized("请输入验证码。", english: "Enter the verification code."))
                .appFont(.body).foregroundStyle(.secondary)
            TextField("验证码", text: $verificationCode).textFieldStyle(.roundedBorder)
            HStack {
                Spacer()
                Button("取消") { verificationPrompt = nil }
                Button("验证") {
                    let code = verificationCode
                    Task { await testSavedConnection(code: code) }
                }
                .buttonStyle(.borderedProminent)
                .disabled(verificationCode.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
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
}

private enum RemoteConnectionWorkspaceTab: String, CaseIterable, Identifiable {
    case terminal = "远程终端"
    case files = "SFTP 文件"
    case details = "连接信息"

    var id: Self { self }

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

private extension String {
    var nonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
