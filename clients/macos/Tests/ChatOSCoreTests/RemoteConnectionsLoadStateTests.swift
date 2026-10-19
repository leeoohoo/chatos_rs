import Foundation
import Testing
@testable import ChatOSCore

@Suite("Remote connections load")
struct RemoteConnectionsLoadStateTests {
    @Test
    func newerRequestAndAccountChangeRejectOlderToken() {
        var state = RemoteConnectionsLoadState()
        let old = state.begin(ownerUserID: "owner-1", accountGeneration: 1)
        let current = state.begin(ownerUserID: "owner-1", accountGeneration: 1)

        #expect(!state.accepts(
            old,
            ownerUserID: "owner-1",
            accountGeneration: 1
        ))
        #expect(state.accepts(
            current,
            ownerUserID: "owner-1",
            accountGeneration: 1
        ))
        #expect(!state.accepts(
            current,
            ownerUserID: "owner-2",
            accountGeneration: 2
        ))
    }

    @Test
    func resetRejectsActiveRequest() {
        var state = RemoteConnectionsLoadState()
        let token = state.begin(ownerUserID: "owner-1", accountGeneration: 1)

        state.reset()

        #expect(!state.accepts(
            token,
            ownerUserID: "owner-1",
            accountGeneration: 1
        ))
    }

    @Test
    func useCaseSortsConnectionsByDisplayName() async throws {
        let service = RemoteConnectionListStub(connections: [
            connection(id: "2", name: "Zulu"),
            connection(id: "1", name: "Alpha"),
        ])
        let useCase = RemoteConnectionListUseCase(service: service)

        let connections = try await useCase.load()

        #expect(connections.map(\.id) == ["1", "2"])
    }

    private func connection(id: String, name: String) -> RemoteConnection {
        RemoteConnection(
            id: id,
            name: name,
            host: "\(id).example.com",
            port: 22,
            username: "user",
            authenticationType: .privateKey,
            hasPassword: false,
            hasPrivateKeyPath: true,
            hasCertificatePath: false,
            defaultRemotePath: nil,
            hostKeyPolicy: .strict,
            localConnectorDeviceID: "device-1",
            localConnectorWorkspaceID: "workspace-1",
            jumpEnabled: false,
            jumpConnectionID: nil,
            jumpHost: nil,
            jumpPort: nil,
            jumpUsername: nil,
            hasJumpPrivateKeyPath: false,
            hasJumpCertificatePath: false,
            hasJumpPassword: false,
            lastActiveAt: nil
        )
    }
}

private actor RemoteConnectionListStub: RemoteConnectionListing {
    let connections: [RemoteConnection]

    init(connections: [RemoteConnection]) {
        self.connections = connections
    }

    func listConnections() async throws -> [RemoteConnection] {
        connections
    }
}
