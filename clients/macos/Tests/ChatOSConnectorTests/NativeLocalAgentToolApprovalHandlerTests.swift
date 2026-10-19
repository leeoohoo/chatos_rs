import ChatOSCore
import Foundation
import Testing
@testable import ChatOSConnector

@Suite("NativeLocalAgentToolApprovalHandler")
struct NativeLocalAgentToolApprovalHandlerTests {
    @Test
    func humanApprovalIsCommittedToOriginalDurableInvocation() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let invocation = Self.invocation(root: root)
        let durable = DurableApprovalStub(pending: [invocation])
        let human = HumanApprovalStub(
            decision: .approve(reason: "用户已批准", rememberAllow: false)
        )
        let handler = NativeLocalAgentToolApprovalHandler(
            durableApprovals: durable,
            contextResolver: ContextResolverStub(root: root),
            humanApprovals: human
        )

        let resolved = try await handler.resolveNextPending(ownerUserID: "owner-1")

        #expect(resolved)
        let request = try #require(await human.requests.first)
        #expect(request.requestID == invocation.invocationID)
        #expect(request.cwd == root)
        #expect(request.projectRoot == root)
        #expect(request.purpose == "检查项目状态")
        #expect(request.approvalScopeKey == "local-agent-terminal:conversation-1")
        let commit = try #require(await durable.commits.first)
        #expect(commit.ownerUserID == "owner-1")
        #expect(commit.invocationID == invocation.invocationID)
        #expect(commit.invocationVersion == invocation.version)
        #expect(commit.approve)
        #expect(commit.decidedBy == "macos-local-approval")
        #expect(commit.reason == "用户已批准")
    }

    @Test
    func humanDenialIsCommittedAsDurableRejection() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let durable = DurableApprovalStub(pending: [Self.invocation(root: root)])
        let handler = NativeLocalAgentToolApprovalHandler(
            durableApprovals: durable,
            contextResolver: ContextResolverStub(root: root),
            humanApprovals: HumanApprovalStub(
                decision: .deny(reason: "用户拒绝")
            )
        )

        _ = try await handler.resolveNextPending(ownerUserID: "owner-1")

        let commit = try #require(await durable.commits.first)
        #expect(!commit.approve)
        #expect(commit.reason == "用户拒绝")
    }

    private static func invocation(root: URL) -> LocalAgentToolInvocationRecord {
        LocalAgentToolInvocationRecord(
            invocationID: "invocation-1",
            runID: "run-1",
            batchID: "batch-1",
            callID: "call-1",
            toolName: "execute_command",
            executionRoute: "platform",
            arguments: .object([
                "command": .string("git status"),
                "path": .string(root.path),
                "purpose": .string("检查项目状态"),
            ]),
            sideEffecting: true,
            requiresApproval: true,
            approvalStatus: "pending",
            approvalDecidedBy: nil,
            approvalReason: nil,
            approvalDecidedAtUnixMs: nil,
            status: "pending_approval",
            result: nil,
            error: nil,
            version: 7,
            claimToken: nil,
            claimUntilUnixMs: nil,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
    }
}

private struct DurableApprovalCommit: Sendable, Equatable {
    let ownerUserID: String
    let invocationID: String
    let invocationVersion: UInt64
    let approve: Bool
    let decidedBy: String
    let reason: String
}

private actor DurableApprovalStub: NativeLocalAgentDurableApprovalServicing {
    let pending: [LocalAgentToolInvocationRecord]
    private(set) var commits: [DurableApprovalCommit] = []

    init(pending: [LocalAgentToolInvocationRecord]) {
        self.pending = pending
    }

    func pendingApprovals(
        ownerUserID: String
    ) async throws -> [LocalAgentToolInvocationRecord] {
        pending
    }

    func decideApproval(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord,
        approve: Bool,
        decidedBy: String,
        reason: String
    ) async throws {
        commits.append(.init(
            ownerUserID: ownerUserID,
            invocationID: invocation.invocationID,
            invocationVersion: invocation.version,
            approve: approve,
            decidedBy: decidedBy,
            reason: reason
        ))
    }
}

private struct ContextResolverStub: NativeLocalAgentApprovalContextResolving {
    let root: URL

    func resolve(
        ownerUserID: String,
        runID: String
    ) async throws -> NativeLocalAgentTaskExecutionContext {
        .init(
            conversationID: "conversation-1",
            projectID: "project-1",
            applicationContext: .init(
                projectID: "project-1",
                projectName: "Project",
                projectRoot: root.path
            ),
            resolvedPath: nil,
            executionRootURL: root,
            workspaceScopeID: "workspace-1",
            toolAuthorization: .init(
                requiresExecution: true,
                enabledBuiltinKinds: ["TerminalController"],
                pluginSelectionIDs: []
            )
        )
    }
}

private actor HumanApprovalStub: NativeLocalHumanApprovalServicing {
    let decision: NativeApprovalDecision
    private(set) var requests: [NativeLocalHumanApprovalRequest] = []

    init(decision: NativeApprovalDecision) {
        self.decision = decision
    }

    func decideHumanApproval(
        _ request: NativeLocalHumanApprovalRequest
    ) async -> NativeApprovalDecision {
        requests.append(request)
        return decision
    }
}
