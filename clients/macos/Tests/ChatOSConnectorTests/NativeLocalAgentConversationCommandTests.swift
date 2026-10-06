@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentConversationCommandTests: XCTestCase {
    func testCreateConversationPersistsTypedProjectBinding() async throws {
        let host = ConversationCommandHostStub()
        let client = NativeLocalAgentConversationClient(host: host)
        let detail = try await client.create(
            ownerUserID: "user-1",
            conversationID: "conversation-project-1",
            title: "Project 1",
            resource: .init(kind: .project, resourceID: "project-1")
        )

        XCTAssertEqual(detail.conversation.resource?.kind, .project)
        XCTAssertEqual(detail.conversation.resource?.resourceID, "project-1")
        let command = try await host.lastCommandObject()
        guard case let .object(resource) = command["resource"] else {
            return XCTFail("Expected typed resource binding")
        }
        XCTAssertEqual(resource["kind"], .string("project"))
        XCTAssertEqual(resource["resource_id"], .string("project-1"))
    }

    func testStartTurnEncodesCompleteVersionedCommand() async throws {
        let host = ConversationCommandHostStub()
        let client = NativeLocalAgentConversationClient(host: host)
        let result = try await client.startTurn(.init(
            ownerUserID: "user-1",
            conversationID: "conversation-1",
            expectedConversationVersion: 3,
            turnID: "turn-1",
            messageID: "message-1",
            runID: "run-1",
            message: "Inspect the project",
            messageMetadata: .object(["source": .string("main_chat")]),
            modelConfigRef: "model-1",
            modelConfigRevision: "model-revision-1",
            capabilityPolicyRevision: "capability-revision-1",
            maxIterations: 12
        ))

        XCTAssertEqual(result.turn.runID, "run-1")
        let command = try await host.lastCommandObject()
        XCTAssertEqual(command["type"], .string("start_conversation_turn"))
        XCTAssertEqual(command["owner_user_id"], .string("user-1"))
        XCTAssertEqual(command["expected_conversation_version"], .number(3))
        XCTAssertEqual(command["model_config_revision"], .string("model-revision-1"))
        XCTAssertEqual(command["max_iterations"], .number(12))
    }

    func testCancelTurnUsesExpectedVersionsAndReason() async throws {
        let host = ConversationCommandHostStub()
        let client = NativeLocalAgentConversationClient(host: host)
        let result = try await client.cancelTurn(
            ownerUserID: "user-1",
            conversationID: "conversation-1",
            expectedConversationVersion: 4,
            turnID: "turn-1",
            expectedRunVersion: 7,
            reason: "user requested stop"
        )

        XCTAssertEqual(result.turn.status, "cancelled")
        let command = try await host.lastCommandObject()
        XCTAssertEqual(command["type"], .string("cancel_conversation_turn"))
        XCTAssertEqual(command["expected_run_version"], .number(7))
        XCTAssertEqual(command["reason"], .string("user requested stop"))
    }

    func testConversationEventFilterIgnoresOtherRunsAndFindsNewTurns() {
        let unrelated = LocalAgentEventRecord(
            cursor: 1,
            eventID: "event-unrelated",
            runID: "run-other",
            eventType: "run_succeeded",
            payload: .object(["conversation_id": .string("conversation-other")]),
            createdAtUnixMs: 1
        )
        XCTAssertFalse(NativeLocalAgentConversationService.eventsAffectConversation(
            [unrelated],
            conversationID: "conversation-1",
            knownRunIDs: ["run-1"]
        ))

        let knownRun = LocalAgentEventRecord(
            cursor: 2,
            eventID: "event-known",
            runID: "run-1",
            eventType: "run_succeeded",
            payload: nil,
            createdAtUnixMs: 2
        )
        XCTAssertTrue(NativeLocalAgentConversationService.eventsAffectConversation(
            [knownRun],
            conversationID: "conversation-1",
            knownRunIDs: ["run-1"]
        ))

        let newTurn = LocalAgentEventRecord(
            cursor: 3,
            eventID: "event-new-turn",
            runID: "run-2",
            eventType: "conversation_turn_started",
            payload: .object(["conversation_id": .string("conversation-1")]),
            createdAtUnixMs: 3
        )
        XCTAssertTrue(NativeLocalAgentConversationService.eventsAffectConversation(
            [newTurn],
            conversationID: "conversation-1",
            knownRunIDs: ["run-1"]
        ))
    }

    func testTaskGraphTerminalDisplaysTheTaskExecutionFinalContent() {
        let value = LocalAgentJSONValue.object([
            "type": .string("task_graph_terminal"),
            "status": .string("succeeded"),
            "tasks": .array([
                .object([
                    "title": .string("Inspect project"),
                    "status": .string("succeeded"),
                    "terminal_outcome": .object([
                        "content": .string("This is a local finance management application."),
                        "reasoning": .string("internal details must not be displayed"),
                    ]),
                ]),
            ]),
        ])

        XCTAssertEqual(
            value.conversationDisplayText,
            "This is a local finance management application."
        )
        XCTAssertFalse(value.conversationDisplayText.contains("internal details"))
    }

    func testTaskGraphTerminalFormatsMultipleTaskResultsWithTitles() {
        let value = LocalAgentJSONValue.object([
            "type": .string("task_graph_terminal"),
            "tasks": .array([
                .object([
                    "title": .string("Research"),
                    "status": .string("succeeded"),
                    "terminal_outcome": .object(["content": .string("Found the cause.")]),
                ]),
                .object([
                    "title": .string("Review"),
                    "status": .string("failed"),
                    "terminal_outcome": .object(["error": .string("Build failed.")]),
                ]),
            ]),
        ])

        XCTAssertEqual(
            value.conversationDisplayText,
            "### Research\n\nFound the cause.\n\n### Review\n\nBuild failed."
        )
    }

    func testTaskExecutionCallbackMetadataRestoresTaskReplyActions() throws {
        let message = LocalAgentConversationMessageRecord(
            messageID: "task_runner_callback::message-1::task-1::run-1",
            conversationID: "conversation-1",
            turnID: "turn-1",
            ordinal: 2,
            role: "assistant",
            content: .string("我已经处理完了。"),
            metadata: .object([
                "kind": .string("task_execution_callback"),
                "task_runner_async": .object([
                    "message_kind": .string("task_lifecycle_update"),
                    "event": .string("task.completed"),
                    "task_id": .string("task-1"),
                    "run_id": .string("run-1"),
                    "status": .string("succeeded"),
                    "source_session_id": .string("conversation-1"),
                    "source_turn_id": .string("turn-1"),
                    "source_user_message_id": .string("message-1"),
                ]),
            ]),
            createdAtUnixMs: 2
        )

        let reply = try XCTUnwrap(message.replyProjections(sourceUserMessageID: "message-1").first)
        XCTAssertEqual(reply.messageID, message.messageID)
        XCTAssertEqual(reply.text, "我已经处理完了。")
        XCTAssertEqual(reply.taskCallback?.taskID, "task-1")
        XCTAssertEqual(reply.taskCallback?.runID, "run-1")
        XCTAssertEqual(reply.taskCallback?.event, "task.completed")
        XCTAssertEqual(reply.taskCallback?.status, "completed")
        XCTAssertEqual(reply.taskCallback?.sourceSessionID, "conversation-1")
        XCTAssertEqual(reply.taskCallback?.sourceTurnID, "turn-1")
        XCTAssertEqual(reply.taskCallback?.sourceUserMessageID, "message-1")
    }

    func testLegacyTaskGraphTerminalExpandsIntoIndependentTaskCallbacks() {
        let message = LocalAgentConversationMessageRecord(
            messageID: "task-graph:graph-1:1",
            conversationID: "conversation-1",
            turnID: "turn-1",
            ordinal: 2,
            role: "assistant",
            content: .object([
                "type": .string("task_graph_terminal"),
                "tasks": .array([
                    .object([
                        "task_id": .string("task-1"),
                        "title": .string("Research"),
                        "status": .string("succeeded"),
                        "terminal_outcome": .object([
                            "content": .string("Found the cause."),
                            "reasoning": .string("must stay hidden"),
                        ]),
                    ]),
                    .object([
                        "task_id": .string("task-2"),
                        "title": .string("Review"),
                        "status": .string("failed"),
                        "terminal_outcome": .object([
                            "error": .string("Build failed."),
                        ]),
                    ]),
                ]),
            ]),
            metadata: .object(["kind": .string("task_graph_terminal")]),
            createdAtUnixMs: 2
        )

        let replies = message.replyProjections(sourceUserMessageID: "message-1")
        XCTAssertEqual(replies.count, 2)
        XCTAssertEqual(replies.map(\.text), ["Found the cause.", "Build failed."])
        XCTAssertEqual(replies.map(\.taskCallback?.taskID), ["task-1", "task-2"])
        XCTAssertEqual(replies.map(\.taskCallback?.event), ["task.completed", "task.failed"])
        XCTAssertEqual(replies.map(\.taskCallback?.status), ["completed", "failed"])
        XCTAssertTrue(replies.allSatisfy { !$0.text.contains("must stay hidden") })
        XCTAssertTrue(replies.allSatisfy {
            $0.taskCallback?.sourceUserMessageID == "message-1"
                && $0.taskCallback?.sourceTurnID == "turn-1"
        })
    }
}

private actor ConversationCommandHostStub: LocalAgentHostClientServicing {
    private var commands: [Data] = []

    func start(ownerUserID: String) async throws {}

    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let value = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        let commandType = value?["type"] as? String
        if commandType == "create_conversation" {
            let resource = value?["resource"] ?? NSNull()
            let response: [String: Any] = [
                "type": "conversation",
                "conversation": [
                    "conversation": [
                        "conversation_id": "conversation-project-1",
                        "owner_user_id": "user-1",
                        "title": "Project 1",
                        "resource": resource,
                        "version": 1,
                        "created_at_unix_ms": 1,
                        "updated_at_unix_ms": 1,
                    ],
                    "turns": [],
                    "messages": [],
                    "attachments": [],
                ],
            ]
            return try JSONSerialization.data(withJSONObject: response)
        }
        let cancelled = commandType == "cancel_conversation_turn"
        let responseType = commandType == "start_conversation_turn"
            ? "conversation_turn_started"
            : "conversation_turn_updated"
        let message: Any = cancelled ? NSNull() : [
            "message_id": "message-1",
            "conversation_id": "conversation-1",
            "turn_id": "turn-1",
            "ordinal": 1,
            "role": "user",
            "content": ["text": "Inspect the project"],
            "metadata": [:],
            "created_at_unix_ms": 1,
        ]
        let response: [String: Any] = [
            "type": responseType,
            "result": [
                "conversation": [
                    "conversation_id": "conversation-1",
                    "owner_user_id": "user-1",
                    "title": "Local conversation",
                    "version": cancelled ? 5 : 4,
                    "created_at_unix_ms": 1,
                    "updated_at_unix_ms": 2,
                ],
                "turn": [
                    "turn_id": "turn-1",
                    "conversation_id": "conversation-1",
                    "user_message_id": "message-1",
                    "run_id": "run-1",
                    "status": cancelled ? "cancelled" : "running",
                    "created_at_unix_ms": 1,
                    "updated_at_unix_ms": 2,
                ],
                "message": message,
                "attachments": [],
            ],
        ]
        return try JSONSerialization.data(withJSONObject: response)
    }

    func lastCommandObject() throws -> [String: LocalAgentJSONValue] {
        guard let command = commands.last else {
            throw CocoaError(.fileNoSuchFile)
        }
        let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: command)
        guard case let .object(object) = value else {
            throw CocoaError(.fileReadCorruptFile)
        }
        return object
    }
}
