import ChatOSConnector
import ChatOSCore
import Foundation

enum RemoteJumpMode: String, CaseIterable, Identifiable {
    case existing = "已有连接"
    case manual = "手动配置"

    var id: Self { self }

    func title(language: ChatOSLanguage) -> String {
        guard language == .english else { return rawValue }
        return switch self {
        case .existing: "Existing Connection"
        case .manual: "Manual Configuration"
        }
    }
}

@MainActor
final class RemoteConnectionEditorViewModel: ObservableObject {
    @Published var name = ""
    @Published var host = ""
    @Published var port = "22"
    @Published var username = ""
    @Published var authenticationType: RemoteAuthenticationType = .privateKey
    @Published var password = ""
    @Published var privateKeyPath = ""
    @Published var certificatePath = ""
    @Published var defaultRemotePath = ""
    @Published var hostKeyPolicy: RemoteHostKeyPolicy = .strict
    @Published var jumpEnabled = false
    @Published var jumpMode: RemoteJumpMode = .manual
    @Published var jumpConnectionID = ""
    @Published var jumpHost = ""
    @Published var jumpPort = "22"
    @Published var jumpUsername = ""
    @Published var jumpPrivateKeyPath = ""
    @Published var jumpCertificatePath = ""
    @Published var jumpPassword = ""
    @Published private(set) var isSaving = false
    @Published private(set) var isTesting = false
    @Published private(set) var successMessage: String?
    @Published var errorMessage: String?
    @Published var verificationPrompt: String?
    @Published var verificationCode = ""

    let editingConnection: RemoteConnection?
    let connections: [RemoteConnection]

    private let service: any RemoteConnectionServicing

    init(
        editingConnection: RemoteConnection?,
        connections: [RemoteConnection],
        service: any RemoteConnectionServicing
    ) {
        self.editingConnection = editingConnection
        self.connections = connections
        self.service = service

        if let connection = editingConnection {
            name = connection.name
            host = connection.host
            port = String(connection.port)
            username = connection.username
            authenticationType = connection.authenticationType
            defaultRemotePath = connection.defaultRemotePath ?? ""
            hostKeyPolicy = connection.hostKeyPolicy
            jumpEnabled = connection.jumpEnabled
            jumpMode = connection.jumpConnectionID == nil ? .manual : .existing
            jumpConnectionID = connection.jumpConnectionID ?? ""
            jumpHost = connection.jumpHost ?? ""
            jumpPort = String(connection.jumpPort ?? 22)
            jumpUsername = connection.jumpUsername ?? ""
        } else {
            if !connections.isEmpty { jumpMode = .existing }
        }
    }

    var title: String { editingConnection == nil ? "新建远端连接" : "编辑远端连接" }

    var availableJumpConnections: [RemoteConnection] {
        connections.filter { $0.id != editingConnection?.id }
    }

    var isBusy: Bool { isSaving || isTesting }

    func testConnection() async {
        errorMessage = nil
        successMessage = nil
        do {
            let draft = try buildDraft()
            isTesting = true
            let result = try await service.testDraft(draft, verificationCode: nil)
            successMessage = result.message?.nonEmpty ?? "连接测试成功。"
        } catch let challenge as RemoteVerificationChallenge {
            verificationPrompt = challenge.prompt
            verificationCode = ""
        } catch {
            errorMessage = error.localizedDescription
        }
        isTesting = false
    }

    func submitVerification() async {
        let code = verificationCode.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty else {
            errorMessage = "请输入验证码。"
            return
        }
        errorMessage = nil
        do {
            let draft = try buildDraft()
            isTesting = true
            let result = try await service.testDraft(draft, verificationCode: code)
            successMessage = result.message?.nonEmpty ?? "二次验证通过，连接测试成功。"
            verificationPrompt = nil
            verificationCode = ""
        } catch let challenge as RemoteVerificationChallenge {
            verificationPrompt = challenge.prompt
            errorMessage = "验证码未通过，请重新输入。"
        } catch {
            errorMessage = error.localizedDescription
        }
        isTesting = false
    }

    func save() async -> RemoteConnection? {
        errorMessage = nil
        successMessage = nil
        do {
            let draft = try buildDraft()
            isSaving = true
            let connection: RemoteConnection
            if let editingConnection {
                connection = try await service.updateConnection(id: editingConnection.id, draft: draft)
            } else {
                connection = try await service.createConnection(draft)
            }
            isSaving = false
            return connection
        } catch {
            errorMessage = error.localizedDescription
            isSaving = false
            return nil
        }
    }

    private func buildDraft() throws -> RemoteConnectionDraft {
        let host = host.trimmingCharacters(in: .whitespacesAndNewlines)
        let username = username.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !host.isEmpty else { throw FormError("请输入远端主机地址。") }
        guard !username.isEmpty else { throw FormError("请输入登录用户名。") }
        guard let port = Int(port), (1...65_535).contains(port) else {
            throw FormError("端口必须在 1 到 65535 之间。")
        }

        let password = password.nonEmpty
        let privateKeyPath = privateKeyPath.nonEmpty
        let certificatePath = certificatePath.nonEmpty
        switch authenticationType {
        case .password where password == nil && editingConnection?.hasPassword != true:
            throw FormError("请输入登录密码。")
        case .privateKey where privateKeyPath == nil && editingConnection?.hasPrivateKeyPath != true:
            throw FormError("请选择或填写私钥路径。")
        case .privateKeyCertificate
            where privateKeyPath == nil && editingConnection?.hasPrivateKeyPath != true:
            throw FormError("请选择或填写私钥路径。")
        case .privateKeyCertificate
            where certificatePath == nil && editingConnection?.hasCertificatePath != true:
            throw FormError("请选择或填写证书路径。")
        default:
            break
        }

        var resolvedJumpConnectionID: String?
        var resolvedJumpHost: String?
        var resolvedJumpPort: Int?
        var resolvedJumpUsername: String?
        var resolvedJumpPrivateKeyPath: String?
        var resolvedJumpCertificatePath: String?
        var resolvedJumpPassword: String?

        if jumpEnabled {
            switch jumpMode {
            case .existing:
                guard let selected = availableJumpConnections.first(where: { $0.id == jumpConnectionID }) else {
                    throw FormError("请选择一条已有连接作为跳板机。")
                }
                resolvedJumpConnectionID = selected.id
                resolvedJumpHost = selected.host
                resolvedJumpPort = selected.port
                resolvedJumpUsername = selected.username
            case .manual:
                let host = jumpHost.trimmingCharacters(in: .whitespacesAndNewlines)
                let username = jumpUsername.trimmingCharacters(in: .whitespacesAndNewlines)
                guard !host.isEmpty, !username.isEmpty else {
                    throw FormError("启用跳板机后，需要填写跳板机地址和用户名。")
                }
                guard let port = Int(jumpPort), (1...65_535).contains(port) else {
                    throw FormError("跳板机端口必须在 1 到 65535 之间。")
                }
                if jumpCertificatePath.nonEmpty != nil && jumpPrivateKeyPath.nonEmpty == nil {
                    throw FormError("跳板机证书必须与私钥一起使用。")
                }
                resolvedJumpHost = host
                resolvedJumpPort = port
                resolvedJumpUsername = username
                resolvedJumpPrivateKeyPath = jumpPrivateKeyPath.nonEmpty
                resolvedJumpCertificatePath = jumpCertificatePath.nonEmpty
                resolvedJumpPassword = jumpPassword.nonEmpty
            }
        }

        return RemoteConnectionDraft(
            name: name.nonEmpty,
            host: host,
            port: port,
            username: username,
            authenticationType: authenticationType,
            password: authenticationType == .password ? password : nil,
            privateKeyPath: authenticationType == .password ? nil : privateKeyPath,
            certificatePath: authenticationType == .privateKeyCertificate ? certificatePath : nil,
            defaultRemotePath: defaultRemotePath.nonEmpty,
            hostKeyPolicy: hostKeyPolicy,
            localConnectorDeviceID: NativeRemoteConnectionService.nativeDeviceID,
            localConnectorWorkspaceID: NativeRemoteConnectionService.nativeWorkspaceID,
            jumpEnabled: jumpEnabled,
            jumpConnectionID: resolvedJumpConnectionID,
            jumpHost: resolvedJumpHost,
            jumpPort: resolvedJumpPort,
            jumpUsername: resolvedJumpUsername,
            jumpPrivateKeyPath: resolvedJumpPrivateKeyPath,
            jumpCertificatePath: resolvedJumpCertificatePath,
            jumpPassword: resolvedJumpPassword,
            localCredentialReferenceID: editingConnection?.id
        )
    }
}

private struct FormError: LocalizedError {
    var message: String
    init(_ message: String) { self.message = message }
    var errorDescription: String? { message }
}

private extension String {
    var nonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
