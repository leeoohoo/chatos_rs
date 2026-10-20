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
    @Published private(set) var savedConnection: RemoteConnection?

    let editingConnection: RemoteConnection?
    let connections: [RemoteConnection]

    private let useCase: RemoteConnectionManagementUseCase
    private var operationState = RemoteConnectionOperationState()
    private var operationTask: Task<Void, Never>?

    init(
        editingConnection: RemoteConnection?,
        connections: [RemoteConnection],
        useCase: RemoteConnectionManagementUseCase
    ) {
        self.editingConnection = editingConnection
        self.connections = connections
        self.useCase = useCase

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

    func testConnection() {
        errorMessage = nil
        successMessage = nil
        do {
            let draft = try buildDraft()
            beginTest(draft: draft, verificationCode: nil)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func submitVerification() {
        let code = verificationCode.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty else {
            errorMessage = "请输入验证码。"
            return
        }
        errorMessage = nil
        do {
            let draft = try buildDraft()
            beginTest(draft: draft, verificationCode: code)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func save() {
        errorMessage = nil
        successMessage = nil
        do {
            let draft = try buildDraft()
            guard let token = operationState.begin(.save) else { return }
            publishOperationState()
            let useCase = useCase
            let editingConnectionID = editingConnection?.id
            operationTask = Task { [weak self] in
                let result: Result<RemoteConnection, Error>
                do {
                    result = .success(try await useCase.save(
                        editingConnectionID: editingConnectionID,
                        draft: draft
                    ))
                } catch {
                    result = .failure(error)
                }
                guard !Task.isCancelled,
                      let self,
                      self.operationState.accepts(token) else { return }
                switch result {
                case let .success(connection):
                    self.savedConnection = connection
                case let .failure(error):
                    self.errorMessage = error.localizedDescription
                }
                self.finishOperation(token)
            }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func consumeSavedConnection() {
        savedConnection = nil
    }

    func deactivate() {
        operationTask?.cancel()
        operationTask = nil
        operationState.reset()
        publishOperationState()
    }

    private func beginTest(
        draft: RemoteConnectionDraft,
        verificationCode: String?
    ) {
        guard let token = operationState.begin(.testDraft) else { return }
        publishOperationState()
        let useCase = useCase
        operationTask = Task { [weak self] in
            let result: Result<RemoteConnectionTestResult, Error>
            do {
                result = .success(try await useCase.testDraft(
                    draft,
                    verificationCode: verificationCode
                ))
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.operationState.accepts(token) else { return }
            switch result {
            case let .success(testResult):
                self.successMessage = testResult.message?.nonEmpty
                    ?? (verificationCode == nil
                        ? "连接测试成功。"
                        : "二次验证通过，连接测试成功。")
                self.verificationPrompt = nil
                self.verificationCode = ""
            case let .failure(challenge as RemoteVerificationChallenge):
                self.verificationPrompt = challenge.prompt
                if verificationCode != nil {
                    self.errorMessage = "验证码未通过，请重新输入。"
                }
                self.verificationCode = ""
            case let .failure(error):
                self.errorMessage = error.localizedDescription
            }
            self.finishOperation(token)
        }
    }

    private func finishOperation(_ token: RemoteConnectionOperationToken) {
        guard operationState.finish(token) else { return }
        operationTask = nil
        publishOperationState()
    }

    private func publishOperationState() {
        let kind = operationState.activeToken?.kind
        isSaving = kind == .save
        isTesting = kind == .testDraft
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
            localConnectorDeviceID: "",
            localConnectorWorkspaceID: "",
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
