import ChatOSAgentRuntime
@testable import ChatOSApp
import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class TeamTodoRunPresentationTests: XCTestCase {
    func testExtractsCommittedPathsOnceFromSuccessfulReceipts() throws {
        let run = try makeRun(
            receipts: [
                "write": .init("""
                    {"result":{"committed_paths":["Sources/B.swift","Sources/A.swift"]}}
                    """),
                "duplicate": .init(#"{"committed_paths":["Sources/A.swift"]}"#),
                "failed": .failure(#"{"committed_paths":["Sources/ShouldNotAppear.swift"]}"#),
            ]
        )

        let presentation = TeamTodoRunPresentation(run: run)

        XCTAssertEqual(presentation.runID, run.id)
        XCTAssertEqual(presentation.receiptCount, 3)
        XCTAssertEqual(presentation.committedPaths, ["Sources/A.swift", "Sources/B.swift"])
    }

    func testKeepsOnlyNewestRunForEachTodo() throws {
        let newest = try makeRun(receipts: ["new": .init(#"{"committed_paths":["new.swift"]}"#)])
        let older = try makeRun(receipts: ["old": .init(#"{"committed_paths":["old.swift"]}"#)])
        let deliveries = [
            newest.id: makeDelivery(run: newest, todoID: "todo-1"),
            older.id: makeDelivery(run: older, todoID: "todo-1"),
        ]

        let presentations = TeamTodoRunPresentation.presentationsByTodoID(
            runs: [newest, older],
            deliveriesByRunID: deliveries
        )

        XCTAssertEqual(presentations["todo-1"]?.runID, newest.id)
        XCTAssertEqual(presentations["todo-1"]?.committedPaths, ["new.swift"])
    }

    private func makeRun(
        receipts: [String: AgentToolOutcome]
    ) throws -> LocalAgentGroupChatRun {
        let runID = UUID()
        let context = try LocalAgentChatRunContext(
            ownerUserID: "owner-1",
            projectID: "project-1",
            roomID: "room-1",
            agentID: "agent-1",
            deliveryID: "delivery-\(runID.uuidString.lowercased())",
            triggerMessageID: "message-1",
            rootMessageID: "message-1",
            runID: runID.uuidString.lowercased(),
            hopCount: 0
        )
        var checkpoint = AgentRunCheckpoint(
            scope: LocalAgentGroupChatRun.runtimeScope(for: context),
            messages: [.init(role: .system, content: "test")]
        )
        checkpoint.id = runID
        checkpoint.status = .completed
        checkpoint.receipts = receipts
        return try LocalAgentGroupChatRun(
            id: runID,
            context: context,
            modelConfigID: "model-1",
            policy: .init(),
            checkpoint: checkpoint,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 2
        )
    }

    private func makeDelivery(
        run: LocalAgentGroupChatRun,
        todoID: String
    ) -> ProjectAgentDelivery {
        ProjectAgentDelivery(
            id: run.context.deliveryID,
            ownerUserID: run.context.ownerUserID,
            roomID: run.context.roomID,
            messageID: run.context.triggerMessageID,
            rootMessageID: run.context.rootMessageID,
            targetAgentID: run.context.agentID,
            triggerKind: .todo,
            status: .completed,
            attempt: 1,
            hopCount: 0,
            deduplicationKey: "todo:\(todoID)",
            createdAtUnixMs: 1
        )
    }
}
