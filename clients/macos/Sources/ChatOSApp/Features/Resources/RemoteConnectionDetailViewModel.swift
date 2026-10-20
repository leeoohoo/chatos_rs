import ChatOSCore
import Foundation

@MainActor
final class RemoteConnectionDetailViewModel: ObservableObject {
    @Published private(set) var isTesting = false
    @Published private(set) var isDeleting = false
    @Published private(set) var notice: String?
    @Published private(set) var errorMessage: String?
    @Published var verificationPrompt: String?
    @Published var verificationCode = ""
    @Published private(set) var deletedConnectionID: String?

    let connectionID: String

    private let useCase: RemoteConnectionManagementUseCase
    private var operationState = RemoteConnectionOperationState()
    private var operationTask: Task<Void, Never>?

    init(
        connectionID: String,
        useCase: RemoteConnectionManagementUseCase
    ) {
        self.connectionID = connectionID
        self.useCase = useCase
    }

    var isBusy: Bool { isTesting || isDeleting }

    func testSavedConnection(code: String?) {
        let verificationCode = code?.trimmingCharacters(in: .whitespacesAndNewlines)
        if code != nil, verificationCode?.isEmpty != false {
            errorMessage = "请输入验证码。"
            return
        }
        guard let token = operationState.begin(.testSaved(connectionID: connectionID)) else {
            return
        }
        publishOperationState()
        notice = nil
        errorMessage = nil
        let useCase = useCase
        let connectionID = connectionID
        operationTask = Task { [weak self] in
            let result: Result<RemoteConnectionTestResult, Error>
            do {
                result = .success(try await useCase.testSaved(
                    connectionID: connectionID,
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
                self.notice = testResult.message?.remoteConnectionNonEmpty
                    ?? "连接测试成功。"
                self.verificationPrompt = nil
                self.verificationCode = ""
            case let .failure(challenge as RemoteVerificationChallenge):
                self.verificationPrompt = challenge.prompt
                self.verificationCode = ""
                if verificationCode != nil {
                    self.errorMessage = "验证码未通过，请重新输入。"
                }
            case let .failure(error):
                self.errorMessage = error.localizedDescription
            }
            self.finishOperation(token)
        }
    }

    func deleteConnection() {
        guard let token = operationState.begin(.delete(connectionID: connectionID)) else {
            return
        }
        publishOperationState()
        notice = nil
        errorMessage = nil
        let useCase = useCase
        let connectionID = connectionID
        operationTask = Task { [weak self] in
            let result: Result<Void, Error>
            do {
                result = .success(try await useCase.delete(connectionID: connectionID))
            } catch {
                result = .failure(error)
            }
            guard !Task.isCancelled,
                  let self,
                  self.operationState.accepts(token) else { return }
            switch result {
            case .success:
                self.deletedConnectionID = connectionID
            case let .failure(error):
                self.errorMessage = error.localizedDescription
            }
            self.finishOperation(token)
        }
    }

    func consumeDeletedConnection() {
        deletedConnectionID = nil
    }

    func deactivate() {
        operationTask?.cancel()
        operationTask = nil
        operationState.reset()
        publishOperationState()
    }

    private func finishOperation(_ token: RemoteConnectionOperationToken) {
        guard operationState.finish(token) else { return }
        operationTask = nil
        publishOperationState()
    }

    private func publishOperationState() {
        switch operationState.activeToken?.kind {
        case .testSaved:
            isTesting = true
            isDeleting = false
        case .delete:
            isTesting = false
            isDeleting = true
        default:
            isTesting = false
            isDeleting = false
        }
    }
}

private extension String {
    var remoteConnectionNonEmpty: String? {
        let value = trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }
}
