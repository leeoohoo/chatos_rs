@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentTeamExecutorResumeTests: XCTestCase {
    func testExplicitRecoveryResumesSameHostRunWithVersionsInsteadOfReturningOldError() async throws {
        let host = TeamResumeHostStub()
        let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: folder) }
        let executor = NativeLocalAgentTeamExecutor(
            host: host,
            groupChatService: NativeAgentGroupChatService(databaseURL: folder.appendingPathComponent("chat.db"))
        )
        let run = try await NativeLocalAgentRuntimeClient(host: host).run(ownerUserID: "alice", runID: "run-1")
        let recovered = try await executor.resumeHostRunIfRequested(
            ownerUserID: "alice", conversationID: "conversation-1", run: run, resumeRequested: true
        )
        XCTAssertEqual(recovered.runID, run.runID)
        XCTAssertEqual(recovered.status, "continuation_ready")
        XCTAssertNil(recovered.terminalOutcome)
        let commands = try await host.recordedCommandData().map { try JSONSerialization.jsonObject(with: $0) as! [String: Any] }
        let resume = try XCTUnwrap(commands.first { $0["type"] as? String == "resume_conversation_turn" })
        XCTAssertEqual(resume["expected_run_status"] as? String, "needs_review")
        XCTAssertEqual(resume["expected_run_version"] as? Int, 7)
        XCTAssertEqual(resume["expected_conversation_version"] as? Int, 4)
        XCTAssertEqual(resume["turn_id"] as? String, "turn-1")
        XCTAssertFalse(commands.contains { $0["type"] as? String == "start_conversation_turn" })
    }

    func testReconciliationDoesNotAutomaticallyRetryReviewOrAnswerWaitingUser() async throws {
        for status in ["needs_review", "waiting_user", "model_running", "succeeded"] {
            let host = TeamResumeHostStub(status: status)
            let folder = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
            defer { try? FileManager.default.removeItem(at: folder) }
            let executor = NativeLocalAgentTeamExecutor(
                host: host,
                groupChatService: NativeAgentGroupChatService(databaseURL: folder.appendingPathComponent("chat.db"))
            )
            let run = try await NativeLocalAgentRuntimeClient(host: host).run(ownerUserID: "alice", runID: "run-1")
            let recovered = try await executor.resumeHostRunIfRequested(
                ownerUserID: "alice", conversationID: "conversation-1", run: run,
                resumeRequested: status != "needs_review"
            )
            XCTAssertEqual(recovered, run)
            let commands = try await host.recordedCommandData().map { try JSONSerialization.jsonObject(with: $0) as! [String: Any] }
            XCTAssertEqual(commands.count, 1)
        }
    }
}

private actor TeamResumeHostStub: LocalAgentHostClientServicing {
    private var status: String
    private var commands: [Data] = []
    init(status: String = "needs_review") { self.status = status }
    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as! [String: Any]
        let response: [String: Any]
        switch object["type"] as? String {
        case "get_run": response = ["type": "run", "run": run()]
        case "get_conversation_context":
            response = ["type": "conversation", "conversation": [
                "conversation": conversation(), "turns": [turn()], "messages": [], "attachments": []
            ]]
        case "resume_conversation_turn":
            status = "continuation_ready"
            response = ["type": "conversation_turn_updated", "result": [
                "conversation": conversation(), "turn": turn(), "message": NSNull(), "attachments": []
            ]]
        default: throw NativeLocalAgentHostError.invalidCommand
        }
        return try JSONSerialization.data(withJSONObject: response)
    }
    func recordedCommandData() -> [Data] { commands }
    private func run() -> [String: Any] {
        var value: [String: Any] = [
            "run_id": "run-1", "owner_user_id": "alice", "owner_entity_type": "conversation_turn",
            "owner_entity_id": "turn-1", "profile_key": "task_execution", "capability_policy_revision": "policy-1",
            "input": [:], "status": status, "version": status == "continuation_ready" ? 8 : 7,
            "created_at_unix_ms": 1, "updated_at_unix_ms": 2
        ]
        if status == "needs_review" { value["terminal_outcome"] = ["error": "old summary timeout"] }
        return value
    }
    private func conversation() -> [String: Any] {
        ["conversation_id": "conversation-1", "owner_user_id": "alice", "title": "Todo",
         "version": 4, "created_at_unix_ms": 1, "updated_at_unix_ms": 2]
    }
    private func turn() -> [String: Any] {
        ["turn_id": "turn-1", "conversation_id": "conversation-1", "run_id": "run-1", "user_message_id": "message-1",
         "status": status == "needs_review" ? "failed" : "running", "created_at_unix_ms": 1, "updated_at_unix_ms": 2]
    }
}
