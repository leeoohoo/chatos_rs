import Foundation
import XCTest
@testable import ChatOSCore

final class ConversationTurnMessageTaskLookupTests: XCTestCase {
    func testUserMessageIsDefaultGraphLookup() {
        let turn = ConversationTurn(
            id: "turn-1",
            sessionID: "conversation-1",
            sequence: 1,
            revision: 1,
            userMessage: ChatMessage(
                id: "message-1",
                role: .user,
                text: "执行九个任务",
                createdAt: .distantPast
            ),
            status: .completed,
            startedAt: .distantPast
        )

        XCTAssertEqual(
            turn.resolvedMessageTaskLookup.sourceUserMessageID,
            "message-1"
        )
    }

    func testExplicitTaskRunnerSourceAlwaysWins() {
        let turn = ConversationTurn(
            id: "turn-1",
            sessionID: "conversation-1",
            sequence: 1,
            revision: 1,
            userMessage: ChatMessage(
                id: "message-1",
                role: .user,
                text: "执行任务",
                createdAt: .distantPast
            ),
            messageTaskLookup: MessageTaskLookup(sourceUserMessageID: "source-1"),
            status: .completed,
            startedAt: .distantPast
        )

        XCTAssertEqual(turn.resolvedMessageTaskLookup.sourceUserMessageID, "source-1")
    }
}
