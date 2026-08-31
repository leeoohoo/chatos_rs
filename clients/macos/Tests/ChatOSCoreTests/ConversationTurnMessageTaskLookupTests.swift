import Foundation
import XCTest
@testable import ChatOSCore

final class ConversationTurnMessageTaskLookupTests: XCTestCase {
    func testProjectExecutionGroupIsUsedForLegacyGraphLookup() {
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
            projectExecutionContext: ProjectExecutionContext(
                projectID: "project-1",
                requirementID: "requirement-1",
                executionGroupID: "execution-group-1"
            ),
            status: .completed,
            startedAt: .distantPast
        )

        XCTAssertEqual(
            turn.resolvedMessageTaskLookup.sourceUserMessageID,
            "execution-group-1"
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
            projectExecutionContext: ProjectExecutionContext(executionGroupID: "group-1"),
            status: .completed,
            startedAt: .distantPast
        )

        XCTAssertEqual(turn.resolvedMessageTaskLookup.sourceUserMessageID, "source-1")
    }
}
