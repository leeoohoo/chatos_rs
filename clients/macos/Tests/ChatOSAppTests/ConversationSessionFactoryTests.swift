import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

@MainActor
final class ConversationSessionFactoryTests: XCTestCase {
    func testFactoryPreservesInitialPresentationState() {
        let factory = ConversationSessionFactory(historyStore: ConversationHistoryStore())
        let turn = ConversationTurn(
            id: "turn-1",
            sessionID: "session-1",
            sequence: 1,
            revision: 1,
            userMessage: ChatMessage(
                id: "message-1",
                role: .user,
                text: "你好",
                createdAt: Date(timeIntervalSince1970: 1)
            ),
            status: .completed,
            startedAt: Date(timeIntervalSince1970: 1)
        )

        let viewModel = factory.makeSession(
            sessionID: "session-1",
            initialTurns: [turn],
            localAgentRuntimeReady: false
        )

        XCTAssertEqual(viewModel.sessionID, "session-1")
        XCTAssertEqual(viewModel.turns.map(\.id), [turn.id])
        XCTAssertFalse(viewModel.isLocalAgentRuntimeReady)
        XCTAssertNil(viewModel.sendUseCase)
        XCTAssertNil(viewModel.realtimeService)
    }

    func testFactoryEvaluatesRuntimeReadinessForEachSession() {
        let factory = ConversationSessionFactory(historyStore: ConversationHistoryStore())

        let unavailable = factory.makeSession(
            sessionID: "session-unavailable",
            localAgentRuntimeReady: false
        )
        let ready = factory.makeSession(
            sessionID: "session-ready",
            localAgentRuntimeReady: true
        )

        XCTAssertFalse(unavailable.isLocalAgentRuntimeReady)
        XCTAssertTrue(ready.isLocalAgentRuntimeReady)
    }
}
