@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentConversationCommandTests: XCTestCase {
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
}

private actor ConversationCommandHostStub: LocalAgentHostClientServicing {
    private var commands: [Data] = []

    func start(ownerUserID: String) async throws {}

    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let value = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        let commandType = value?["type"] as? String
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
