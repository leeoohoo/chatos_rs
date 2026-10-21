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
    func remoteCommandApprovalReceivesTheCompleteCommandWithoutPrefixTruncation() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let command = String(repeating: "printf x; ", count: 80)
            + "printf 'tail marker' | sed -n '1p'"
        let invocation = Self.remoteCommandInvocation(command: command)
        let durable = DurableApprovalStub(pending: [invocation])
        let human = HumanApprovalStub(
            decision: .approve(reason: "完整命令已核对", rememberAllow: false)
        )
        let handler = NativeLocalAgentToolApprovalHandler(
            durableApprovals: durable,
            contextResolver: ContextResolverStub(root: root),
            humanApprovals: human
        )

        _ = try await handler.resolveNextPending(ownerUserID: "owner-1")

        let request = try #require(await human.requests.first)
        #expect(command.count > 500)
        #expect(request.command == "remote SSH command")
        #expect(request.arguments == [command])
        #expect(request.arguments[0].hasSuffix("printf 'tail marker' | sed -n '1p'"))
        #expect(request.automaticApprovalEvidenceComplete)
        #expect(request.evaluationArguments?.first?.contains("tail marker") == true)
    }

    @Test
    func remoteUploadWithHiddenContentCannotBeBlindlyAutoApproved() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let path = "/srv/" + String(repeating: "nested/", count: 90) + "artifact.txt"
        let invocation = Self.remoteUploadInvocation(path: path)
        let human = HumanApprovalStub(decision: .deny(reason: "需要人工查看上传内容"))
        let handler = NativeLocalAgentToolApprovalHandler(
            durableApprovals: DurableApprovalStub(pending: [invocation]),
            contextResolver: ContextResolverStub(root: root),
            humanApprovals: human
        )

        _ = try await handler.resolveNextPending(ownerUserID: "owner-1")

        let request = try #require(await human.requests.first)
        #expect(path.count > 500)
        #expect(request.arguments == [path])
        #expect(!request.automaticApprovalEvidenceComplete)
        #expect(request.evaluationArguments?.first?.contains("EVIDENCE_COMPLETE=false") == true)
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

    @Test
    func failedDurableWritebackRetriesRecordedDecisionWithoutReprompting() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let invocation = Self.invocation(root: root)
        let durable = RetryingDurableApprovalStub(
            pending: [invocation],
            failuresBeforeSuccess: 1
        )
        let human = HumanApprovalStub(
            decision: .approve(reason: "用户已批准", rememberAllow: false)
        )
        let journal = NativeLocalAgentApprovalDecisionJournal()
        let handler = NativeLocalAgentToolApprovalHandler(
            durableApprovals: durable,
            contextResolver: ContextResolverStub(root: root),
            humanApprovals: human,
            decisionJournal: journal
        )

        await #expect(throws: ApprovalWritebackStubError.self) {
            try await handler.resolveNextPending(ownerUserID: "owner-1")
        }
        #expect(await human.requests.count == 1)
        #expect(await journal.snapshot().count == 1)

        let resolved = try await handler.resolveNextPending(ownerUserID: "owner-1")

        #expect(resolved)
        #expect(await human.requests.count == 1)
        #expect(await durable.attempts.count == 2)
        #expect(await durable.successfulCommits.count == 1)
        #expect(await journal.snapshot().isEmpty)
    }

    @Test
    func persistedDecisionSurvivesHandlerRecreation() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let journalURL = root.appendingPathComponent("approval-writebacks.json")
        let invocation = Self.invocation(root: root)
        let firstHuman = HumanApprovalStub(
            decision: .deny(reason: "用户拒绝")
        )
        let failingDurable = RetryingDurableApprovalStub(
            pending: [invocation],
            failuresBeforeSuccess: 1
        )
        let firstHandler = NativeLocalAgentToolApprovalHandler(
            durableApprovals: failingDurable,
            contextResolver: ContextResolverStub(root: root),
            humanApprovals: firstHuman,
            decisionJournal: .init(fileURL: journalURL)
        )
        await #expect(throws: ApprovalWritebackStubError.self) {
            try await firstHandler.resolveNextPending(ownerUserID: "owner-1")
        }
        #expect(await firstHuman.requests.count == 1)

        let restoredJournal = NativeLocalAgentApprovalDecisionJournal(fileURL: journalURL)
        let succeedingDurable = RetryingDurableApprovalStub(
            pending: [invocation],
            failuresBeforeSuccess: 0
        )
        let secondHuman = HumanApprovalStub(
            decision: .approve(reason: "不应被调用", rememberAllow: false)
        )
        let secondHandler = NativeLocalAgentToolApprovalHandler(
            durableApprovals: succeedingDurable,
            contextResolver: ContextResolverStub(root: root),
            humanApprovals: secondHuman,
            decisionJournal: restoredJournal
        )

        let resolved = try await secondHandler.resolveNextPending(ownerUserID: "owner-1")

        #expect(resolved)
        #expect(await secondHuman.requests.isEmpty)
        let commit = try #require(await succeedingDurable.successfulCommits.first)
        #expect(!commit.approve)
        #expect(commit.reason == "用户拒绝")
        #expect(await restoredJournal.snapshot().isEmpty)
        #expect(
            await NativeLocalAgentApprovalDecisionJournal(fileURL: journalURL)
                .snapshot().isEmpty
        )
    }

    @Test
    func journalStorageFailureStillKeepsDecisionForInProcessRetry() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let blockedParent = root.appendingPathComponent("not-a-directory")
        try Data("blocked".utf8).write(to: blockedParent)
        let invocation = Self.invocation(root: root)
        let durable = RetryingDurableApprovalStub(
            pending: [invocation],
            failuresBeforeSuccess: 0
        )
        let human = HumanApprovalStub(
            decision: .approve(reason: "用户已批准", rememberAllow: false)
        )
        let journal = NativeLocalAgentApprovalDecisionJournal(
            fileURL: blockedParent.appendingPathComponent("approval-writebacks.json")
        )
        let handler = NativeLocalAgentToolApprovalHandler(
            durableApprovals: durable,
            contextResolver: ContextResolverStub(root: root),
            humanApprovals: human,
            decisionJournal: journal
        )

        await #expect(throws: Error.self) {
            try await handler.resolveNextPending(ownerUserID: "owner-1")
        }
        #expect(await human.requests.count == 1)
        #expect(await durable.attempts.isEmpty)
        #expect(await journal.snapshot().count == 1)

        let resolved = try await handler.resolveNextPending(ownerUserID: "owner-1")

        #expect(resolved)
        #expect(await human.requests.count == 1)
        #expect(await durable.successfulCommits.count == 1)
        #expect(await journal.snapshot().isEmpty)
    }

    @Test
    func commitApprovalReceivesStagedEvidenceForAIReview() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let invocation = Self.commitInvocation()
        let durable = DurableApprovalStub(pending: [invocation])
        let human = HumanApprovalStub(
            decision: .approve(reason: "host verified", rememberAllow: false)
        )
        let handler = NativeLocalAgentToolApprovalHandler(
            durableApprovals: durable,
            contextResolver: ContextResolverStub(root: root),
            humanApprovals: human,
            editEvidenceProvider: EditEvidenceStub()
        )

        _ = try await handler.resolveNextPending(ownerUserID: "owner-1")

        let request = try #require(await human.requests.first)
        #expect(request.evaluationArguments == ["trusted staged edit summary"])
        #expect(request.purpose == "修复图标")
    }

    @Test
    func commitApprovalWithoutVerifiedStagedEvidenceMustBeMarkedIncomplete() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let human = HumanApprovalStub(
            decision: .deny(reason: "证据不完整")
        )
        let handler = NativeLocalAgentToolApprovalHandler(
            durableApprovals: DurableApprovalStub(pending: [Self.commitInvocation()]),
            contextResolver: ContextResolverStub(root: root),
            humanApprovals: human
        )

        _ = try await handler.resolveNextPending(ownerUserID: "owner-1")

        let request = try #require(await human.requests.first)
        let evidence = try #require(request.evaluationArguments?.first)
        #expect(evidence.contains("EVIDENCE_COMPLETE=false"))
        #expect(evidence.contains("必须转人工确认"))
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

    private static func commitInvocation() -> LocalAgentToolInvocationRecord {
        LocalAgentToolInvocationRecord(
            invocationID: "commit-invocation-1",
            runID: "run-1",
            batchID: "batch-1",
            callID: "call-1",
            toolName: "commit_edit_session",
            executionRoute: "platform",
            arguments: .object(["session_id": .string("edit-session-1")]),
            sideEffecting: true,
            requiresApproval: true,
            approvalStatus: "pending",
            approvalDecidedBy: nil,
            approvalReason: nil,
            approvalDecidedAtUnixMs: nil,
            status: "pending_approval",
            result: nil,
            error: nil,
            version: 1,
            claimToken: nil,
            claimUntilUnixMs: nil,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
    }

    private static func remoteCommandInvocation(
        command: String
    ) -> LocalAgentToolInvocationRecord {
        LocalAgentToolInvocationRecord(
            invocationID: "remote-command-invocation-1",
            runID: "run-1",
            batchID: "batch-1",
            callID: "call-1",
            toolName: "remote_connection_controller_run_command",
            executionRoute: "platform",
            arguments: .object(["command": .string(command)]),
            sideEffecting: true,
            requiresApproval: true,
            approvalStatus: "pending",
            approvalDecidedBy: nil,
            approvalReason: nil,
            approvalDecidedAtUnixMs: nil,
            status: "pending_approval",
            result: nil,
            error: nil,
            version: 1,
            claimToken: nil,
            claimUntilUnixMs: nil,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
    }

    private static func remoteUploadInvocation(path: String) -> LocalAgentToolInvocationRecord {
        LocalAgentToolInvocationRecord(
            invocationID: "remote-upload-invocation-1",
            runID: "run-1",
            batchID: "batch-1",
            callID: "call-1",
            toolName: "remote_connection_controller_upload_file",
            executionRoute: "platform",
            arguments: .object([
                "path": .string(path),
                "content": .string("content intentionally hidden from approval model"),
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
            version: 1,
            claimToken: nil,
            claimUntilUnixMs: nil,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
    }
}

private enum ApprovalWritebackStubError: Error {
    case unavailable
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

private actor RetryingDurableApprovalStub: NativeLocalAgentDurableApprovalServicing {
    let pending: [LocalAgentToolInvocationRecord]
    private var failuresBeforeSuccess: Int
    private(set) var attempts: [DurableApprovalCommit] = []
    private(set) var successfulCommits: [DurableApprovalCommit] = []

    init(
        pending: [LocalAgentToolInvocationRecord],
        failuresBeforeSuccess: Int
    ) {
        self.pending = pending
        self.failuresBeforeSuccess = failuresBeforeSuccess
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
        let commit = DurableApprovalCommit(
            ownerUserID: ownerUserID,
            invocationID: invocation.invocationID,
            invocationVersion: invocation.version,
            approve: approve,
            decidedBy: decidedBy,
            reason: reason
        )
        attempts.append(commit)
        if failuresBeforeSuccess > 0 {
            failuresBeforeSuccess -= 1
            throw ApprovalWritebackStubError.unavailable
        }
        successfulCommits.append(commit)
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

private struct EditEvidenceStub: NativeMCPCodeWriteApprovalEvidenceProviding {
    func approvalEvidence(
        arguments _: [String: NativeJSONValue],
        scope _: NativeMCPCodeWriteScope,
        projectRoot _: URL,
        ownedPaths _: [String]
    ) async throws -> NativeMCPCodeWriteApprovalEvidence {
        .init(
            evaluationSummary: "trusted staged edit summary",
            purpose: "修复图标",
            isComplete: true
        )
    }
}
