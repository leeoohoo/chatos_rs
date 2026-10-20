import Foundation
import Testing
@testable import ChatOSCore

@Suite("Remote terminal lifecycle")
struct RemoteTerminalUseCaseTests {
    @Test
    func reconnectAndResetRejectOldConnectAndSessionTokens() {
        var state = RemoteTerminalLifecycleState()
        let oldConnect = state.beginConnect(connectionID: "c")
        let newConnect = state.beginConnect(connectionID: "c")

        #expect(!state.acceptsConnect(oldConnect))
        #expect(state.acceptsConnect(newConnect))

        let session = state.activate(newConnect)
        #expect(session != nil)
        #expect(state.acceptsSession(session!))

        state.reset()
        #expect(!state.acceptsSession(session!))
    }

    @Test
    func useCaseRoutesConnectionAndVerificationCode() async throws {
        let provider = RemoteTerminalProviderStub()
        let useCase = RemoteTerminalUseCase(provider: provider)

        let session = try await useCase.connect(
            connectionID: "connection-1",
            verificationCode: "123456"
        )

        #expect(await provider.requests == [.init(
            connectionID: "connection-1",
            verificationCode: "123456"
        )])
        #expect(session.isRunning)
    }
}

private struct RemoteTerminalProviderRequest: Sendable, Equatable {
    let connectionID: String
    let verificationCode: String?
}

private actor RemoteTerminalProviderStub: RemoteTerminalSessionProviding {
    private(set) var requests: [RemoteTerminalProviderRequest] = []

    func openRemoteTerminalSession(
        connectionID: String,
        verificationCode: String?
    ) async throws -> any RemoteTerminalSessionPort {
        requests.append(.init(
            connectionID: connectionID,
            verificationCode: verificationCode
        ))
        return RemoteTerminalSessionStub()
    }
}

private final class RemoteTerminalSessionStub: RemoteTerminalSessionPort, @unchecked Sendable {
    var isRunning: Bool { true }
    func events() -> AsyncStream<RemoteTerminalEvent> { .init { $0.finish() } }
    func start(columns: Int, rows: Int) {}
    func send(_ data: Data) {}
    func resize(columns: Int, rows: Int) {}
    func interrupt() {}
    func close() {}
}
