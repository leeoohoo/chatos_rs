import ChatOSAgentRuntime
@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentTeamExecutorTests: XCTestCase {
    func testHumanOnlyBlockDoesNotContinueHostRun() {
        XCTAssertFalse(NativeLocalAgentTeamExecutor.shouldContinueAfterSuccessfulHostRun(
            acceptanceChecks: [
                .init(
                    position: 1,
                    criterion: "需要 Human 提供生产账号",
                    status: .blocked,
                    blockedReason: "只有 Human 可以提供账号。",
                    requiresHuman: true
                ),
            ]
        ))
    }

    func testHumanBlockDoesNotHideAnotherActionableAcceptanceCheck() {
        XCTAssertTrue(NativeLocalAgentTeamExecutor.shouldContinueAfterSuccessfulHostRun(
            acceptanceChecks: [
                .init(
                    position: 1,
                    criterion: "需要 Human 提供生产账号",
                    status: .blocked,
                    blockedReason: "只有 Human 可以提供账号。",
                    requiresHuman: true
                ),
                .init(
                    position: 2,
                    criterion: "完成本地回归测试",
                    status: .notVerified
                ),
            ]
        ))
    }

    func testPassedChecksDoNotCauseAnotherHostRun() {
        XCTAssertFalse(NativeLocalAgentTeamExecutor.shouldContinueAfterSuccessfulHostRun(
            acceptanceChecks: [
                .init(
                    position: 1,
                    criterion: "完成本地回归测试",
                    status: .passed,
                    evidence: "测试全部通过。"
                ),
            ]
        ))
    }

    func testTeamToolRouterRejectsOrdinaryTaskRun() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("native-team-router-tests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let router = NativeLocalAgentTeamExecutorToolRouter(
            service: NativeAgentGroupChatService(
                databaseURL: folder.appendingPathComponent("chat.db")
            )
        )
        let run = try hostRun(input: [
            "conversation_id": .string("ordinary-task-conversation"),
        ])

        do {
            _ = try await router.execute(
                ownerUserID: "alice",
                run: run,
                invocation: invocation(runID: run.runID)
            )
            XCTFail("ordinary Task Run must not use Agent-team tools")
        } catch let error as NativeLocalAgentPlatformToolError {
            XCTAssertEqual(error, .invalidRunContext)
        }
    }

    func testTeamToolRouterRejectsMismatchedHostIdentityBeforeStoreLookup() async throws {
        let folder = FileManager.default.temporaryDirectory
            .appendingPathComponent("native-team-router-identity-tests-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: folder) }
        let router = NativeLocalAgentTeamExecutorToolRouter(
            service: NativeAgentGroupChatService(
                databaseURL: folder.appendingPathComponent("chat.db")
            )
        )
        let run = try hostRun(input: [
            "conversation_id": .string("agent-team-executor-delivery-1"),
            "team_executor_context": .object([
                "delivery_id": .string("delivery-1"),
                "todo_id": .string("todo-1"),
                "team_id": .string("team-1"),
                "agent_id": .string("agent-1"),
                "project_id": .string("project-1"),
            ]),
            "agent_execution_context": .object([
                "schema_version": .number(2),
                "room_id": .string("team-1"),
                "agent_id": .string("forged-agent"),
                "project_id": .string("project-1"),
            ]),
        ])

        do {
            _ = try await router.execute(
                ownerUserID: "alice",
                run: run,
                invocation: invocation(runID: run.runID)
            )
            XCTFail("mismatched immutable Host identity must be rejected")
        } catch let error as NativeLocalAgentPlatformToolError {
            XCTAssertEqual(error, .invalidRunContext)
        }
    }

    private func hostRun(
        input: [String: LocalAgentJSONValue]
    ) throws -> LocalAgentRunRecord {
        let data = try JSONSerialization.data(withJSONObject: [
            "run_id": "host-run-1",
            "owner_user_id": "alice",
            "owner_entity_type": "conversation_turn",
            "owner_entity_id": "turn-1",
            "profile_key": "task_execution",
            "capability_policy_revision": "capability-1",
            "input": try jsonObject(.object(input)),
            "status": "running",
            "version": 1,
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 1,
        ])
        return try JSONDecoder().decode(LocalAgentRunRecord.self, from: data)
    }

    private func invocation(runID: String) -> LocalAgentToolInvocationRecord {
        .init(
            invocationID: "invocation-1",
            runID: runID,
            batchID: "batch-1",
            callID: "call-1",
            toolName: "todo_get_context",
            executionRoute: "platform",
            arguments: .object([:]),
            sideEffecting: false,
            requiresApproval: false,
            approvalStatus: "not_required",
            approvalDecidedBy: nil,
            approvalReason: nil,
            approvalDecidedAtUnixMs: nil,
            status: "running",
            result: nil,
            error: nil,
            version: 1,
            claimToken: "claim-1",
            claimUntilUnixMs: 2,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
    }

    private func jsonObject(_ value: LocalAgentJSONValue) throws -> Any {
        let data = try JSONEncoder().encode(value)
        return try JSONSerialization.jsonObject(with: data)
    }
}
