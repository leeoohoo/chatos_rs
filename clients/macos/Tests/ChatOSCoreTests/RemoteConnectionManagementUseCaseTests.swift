import Foundation
import Testing
@testable import ChatOSCore

@Suite("Remote connection management")
struct RemoteConnectionManagementUseCaseTests {
    @Test
    func operationStateIsSingleFlightAndRejectsResetToken() {
        var state = RemoteConnectionOperationState()
        let active = state.begin(.testDraft)

        #expect(active != nil)
        #expect(state.begin(.save) == nil)
        #expect(state.accepts(active!))

        state.reset()

        #expect(!state.accepts(active!))
        #expect(state.begin(.save) != nil)
    }

    @Test
    func useCaseRoutesCreateUpdateTestAndDelete() async throws {
        let service = RemoteConnectionManagementStub()
        let useCase = RemoteConnectionManagementUseCase(service: service)
        let draft = makeDraft(host: "server.example.com")

        let created = try await useCase.save(editingConnectionID: nil, draft: draft)
        let updated = try await useCase.save(editingConnectionID: "saved", draft: draft)
        _ = try await useCase.testDraft(draft, verificationCode: "123456")
        _ = try await useCase.testSaved(connectionID: "saved", verificationCode: nil)
        try await useCase.delete(connectionID: "saved")

        #expect(created.id == "created")
        #expect(updated.id == "saved")
        #expect(await service.calls == [
            .create,
            .update("saved"),
            .testDraft("123456"),
            .testSaved("saved", nil),
            .delete("saved"),
        ])
    }

    private func makeDraft(host: String) -> RemoteConnectionDraft {
        RemoteConnectionDraft(
            name: "Server",
            host: host,
            port: 22,
            username: "user",
            authenticationType: .privateKey,
            password: nil,
            privateKeyPath: "/key",
            certificatePath: nil,
            defaultRemotePath: nil,
            hostKeyPolicy: .strict,
            localConnectorDeviceID: "",
            localConnectorWorkspaceID: "",
            jumpEnabled: false,
            jumpConnectionID: nil,
            jumpHost: nil,
            jumpPort: nil,
            jumpUsername: nil,
            jumpPrivateKeyPath: nil,
            jumpCertificatePath: nil,
            jumpPassword: nil
        )
    }
}

private enum RemoteConnectionManagementCall: Sendable, Equatable {
    case create
    case update(String)
    case testDraft(String?)
    case testSaved(String, String?)
    case delete(String)
}

private actor RemoteConnectionManagementStub: RemoteConnectionServicing {
    private(set) var calls: [RemoteConnectionManagementCall] = []

    func listConnections() async throws -> [RemoteConnection] { [] }
    func createConnection(_ draft: RemoteConnectionDraft) async throws -> RemoteConnection {
        calls.append(.create)
        return connection(id: "created", draft: draft)
    }
    func updateConnection(
        id: String,
        draft: RemoteConnectionDraft
    ) async throws -> RemoteConnection {
        calls.append(.update(id))
        return connection(id: id, draft: draft)
    }
    func deleteConnection(id: String) async throws {
        calls.append(.delete(id))
    }
    func testDraft(
        _ draft: RemoteConnectionDraft,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        calls.append(.testDraft(verificationCode))
        return .init(success: true, message: nil)
    }
    func testSaved(
        id: String,
        verificationCode: String?
    ) async throws -> RemoteConnectionTestResult {
        calls.append(.testSaved(id, verificationCode))
        return .init(success: true, message: nil)
    }

    private func connection(
        id: String,
        draft: RemoteConnectionDraft
    ) -> RemoteConnection {
        RemoteConnection(
            id: id,
            name: draft.name ?? draft.host,
            host: draft.host,
            port: draft.port,
            username: draft.username,
            authenticationType: draft.authenticationType,
            hasPassword: draft.password != nil,
            hasPrivateKeyPath: draft.privateKeyPath != nil,
            hasCertificatePath: draft.certificatePath != nil,
            defaultRemotePath: draft.defaultRemotePath,
            hostKeyPolicy: draft.hostKeyPolicy,
            localConnectorDeviceID: "device",
            localConnectorWorkspaceID: "workspace",
            jumpEnabled: draft.jumpEnabled,
            jumpConnectionID: draft.jumpConnectionID,
            jumpHost: draft.jumpHost,
            jumpPort: draft.jumpPort,
            jumpUsername: draft.jumpUsername,
            hasJumpPrivateKeyPath: draft.jumpPrivateKeyPath != nil,
            hasJumpCertificatePath: draft.jumpCertificatePath != nil,
            hasJumpPassword: draft.jumpPassword != nil,
            lastActiveAt: nil
        )
    }
}
