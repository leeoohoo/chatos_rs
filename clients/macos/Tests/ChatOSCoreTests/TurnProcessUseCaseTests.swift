import Foundation
import Testing
@testable import ChatOSCore

@Suite("TurnProcessUseCase")
struct TurnProcessUseCaseTests {
    @Test
    func forwardsTypedConversationIdentityAndReturnsNodes() async throws {
        let service = TurnProcessServiceSpy()
        let useCase = TurnProcessUseCase(service: service)

        let nodes = try await useCase.load(
            sessionID: "session-1",
            turnID: "turn-1"
        )

        #expect(nodes.map(\.id) == ["node-1"])
        #expect(await service.requests == [
            .init(sessionID: "session-1", turnID: "turn-1"),
        ])
    }
}

private actor TurnProcessServiceSpy: TurnProcessServicing {
    struct Request: Sendable, Equatable {
        let sessionID: String
        let turnID: String
    }

    private(set) var requests: [Request] = []

    func fetchProcessNodes(
        sessionID: String,
        turnID: String
    ) async throws -> [TurnProcessNode] {
        requests.append(.init(sessionID: sessionID, turnID: turnID))
        return [TurnProcessNode(
            id: "node-1",
            title: "Tool",
            detail: nil,
            status: .completed,
            kind: .tool,
            timestamp: Date(timeIntervalSince1970: 1)
        )]
    }
}
