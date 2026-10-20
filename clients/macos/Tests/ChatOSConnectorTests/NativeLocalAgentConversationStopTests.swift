@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentConversationStopTests: XCTestCase {
    func testFailedTurnWithReviewRunCanBeCancelledUsingItsVersion() async throws {
        let host = ConversationStopHost(turnStatus: "failed", runStatus: "needs_review")
        let service = try await service(host)
        try await service.stopTurn(conversationID: "conversation-1", turnID: "turn-1")

        let commands = await host.commands
        XCTAssertEqual(commands.map { $0["type"] }, [
            .string("get_conversation_context"), .string("get_run"),
            .string("cancel_conversation_turn"),
        ])
        XCTAssertEqual(commands.last?["turn_id"], .string("turn-1"))
        XCTAssertEqual(commands.last?["expected_run_version"], .number(7))
        XCTAssertEqual(commands.last?["expected_conversation_version"], .number(4))
    }

    func testStaleCardsDoNotCancelNewerRunningTurns() async throws {
        for status in ["completed", "cancelled", "failed"] {
            let host = ConversationStopHost(turnStatus: status, runStatus: "failed")
            let service = try await service(host)
            try await service.stopTurn(conversationID: "conversation-1", turnID: "turn-1")
            try await service.stopTurn(conversationID: "conversation-1", turnID: "missing-turn")

            let commands = await host.commands
            XCTAssertFalse(commands.contains { $0["type"] == .string("cancel_conversation_turn") })
        }
    }

    func testStopWithoutExplicitTurnStillTargetsCurrentRunningTurn() async throws {
        let host = ConversationStopHost(turnStatus: "failed", runStatus: "needs_review")
        let service = try await service(host)
        try await service.stopTurn(conversationID: "conversation-1", turnID: nil)
        let commands = await host.commands
        XCTAssertEqual(commands.last?["type"], .string("cancel_conversation_turn"))
        XCTAssertEqual(commands.last?["turn_id"], .string("new-turn"))
        XCTAssertFalse(commands.contains { $0["type"] == .string("get_run") })
    }

    func testRunLookupFailureRemainsVisibleInsteadOfPretendingCancellationSucceeded() async throws {
        let host = ConversationStopHost(turnStatus: "failed", runStatus: "needs_review", failLookup: true)
        let service = try await service(host)
        do {
            try await service.stopTurn(conversationID: "conversation-1", turnID: "turn-1")
            XCTFail("Run lookup failure must be reported")
        } catch {
            let commands = await host.commands
            XCTAssertFalse(commands.contains { $0["type"] == .string("cancel_conversation_turn") })
        }
    }

    private func service(_ host: ConversationStopHost) async throws -> NativeLocalAgentConversationService {
        let service = NativeLocalAgentConversationService(
            host: host,
            attachmentRootURL: URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent(UUID().uuidString),
            runtimeSettings: NativeLocalAgentConversationRuntimeSettingsService(host: host)
        )
        let capability = LocalAgentCapabilityPolicySnapshot(
            ownerUserID: "user-1", profileKey: "main_chat", capabilityPolicyRevision: "policy-1"
        )
        try await service.configure(ownerUserID: "user-1", bootstrap: .init(
            modelSnapshots: [.init(
                ownerUserID: "user-1", modelConfigRef: "model-1", modelConfigRevision: "revision-1",
                credentialRef: "env:TEST", baseURL: "https://example.test/v1", model: "test",
                provider: "openai", supportsResponses: true
            )],
            modelOptions: [], defaultConversationModelConfigRef: "model-1",
            capabilitySnapshot: capability,
            capabilitySnapshotsByModelConfigRef: ["model-1": capability],
            pluginInstallerCapabilitySnapshotsByModelConfigRef: ["model-1": capability]
        ))
        return service
    }
}

private actor ConversationStopHost: LocalAgentHostClientServicing {
    private(set) var commands: [[String: LocalAgentJSONValue]] = []
    let turnStatus: String
    let runStatus: String
    let failLookup: Bool

    init(turnStatus: String, runStatus: String, failLookup: Bool = false) {
        self.turnStatus = turnStatus
        self.runStatus = runStatus
        self.failLookup = failLookup
    }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        guard case let .object(object) = try JSONDecoder().decode(LocalAgentJSONValue.self, from: command) else {
            throw CocoaError(.fileReadCorruptFile)
        }
        commands.append(object)
        let conversation: [String: Any] = [
            "conversation_id": "conversation-1", "owner_user_id": "user-1", "title": "Conversation",
            "version": 4, "created_at_unix_ms": 1, "updated_at_unix_ms": 2,
        ]
        func turn(_ id: String, status: String) -> [String: Any] {
            ["turn_id": id, "conversation_id": "conversation-1", "user_message_id": "message-1",
             "run_id": id == "turn-1" ? "run-1" : "new-run", "status": status,
             "created_at_unix_ms": 1, "updated_at_unix_ms": 2]
        }
        let response: [String: Any]
        switch object["type"] {
        case .string("get_conversation_context"):
            response = ["type": "conversation", "conversation": [
                "conversation": conversation,
                "turns": [turn("turn-1", status: turnStatus), turn("new-turn", status: "running")],
                "messages": [], "attachments": [],
            ]]
        case .string("get_run"):
            if failLookup { throw CocoaError(.fileReadUnknown) }
            response = ["type": "run", "run": [
                "run_id": "run-1", "owner_user_id": "user-1", "owner_entity_type": "conversation_turn",
                "owner_entity_id": "turn-1", "profile_key": "main_chat",
                "capability_policy_revision": "policy-1", "input": [:], "status": runStatus,
                "version": 7, "created_at_unix_ms": 1, "updated_at_unix_ms": 2,
            ]]
        case .string("cancel_conversation_turn"):
            let targetID = object["turn_id"] == .string("new-turn") ? "new-turn" : "turn-1"
            response = ["type": "conversation_turn_updated", "result": [
                "conversation": conversation, "turn": turn(targetID, status: "cancelled"),
                "message": NSNull(), "attachments": [],
            ]]
        default:
            throw CocoaError(.featureUnsupported)
        }
        return try JSONSerialization.data(withJSONObject: response)
    }
}
