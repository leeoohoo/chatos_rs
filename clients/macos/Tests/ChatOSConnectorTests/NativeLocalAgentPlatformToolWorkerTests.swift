@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentPlatformToolWorkerTests: XCTestCase {
    func testCapabilityCatalogPartitionsRustAndNativeTools() {
        let names = NativeLocalAgentPlatformToolCatalog.capabilityTools.compactMap { value in
            guard case let .object(tool) = value,
                  case let .string(name)? = tool["name"] else { return nil as String? }
            return name
        }
        XCTAssertEqual(names, ["local_attachment_read"])
        XCTAssertEqual(
            NativeLocalAgentPlatformToolCatalog.readOnlyToolNames,
            [
                "local_attachment_read", "read_file_raw", "read_file_range", "list_dir",
                "search_text", "read_file", "search_files", "process_poll", "process_log",
                "process_wait", "capability_describe", "capability_search",
                "capability_skill_activate", "capability_skill_read_resource",
            ]
        )
        XCTAssertEqual(
            NativeLocalAgentPlatformToolCatalog.taskExecutionToolNames,
            Set([
                "read_file_raw", "read_file_range", "list_dir", "search_text", "read_file",
                "search_files", "open_edit_session", "stage_edit_batch",
                "commit_edit_session", "abort_edit_session",
                "execute_command", "process_poll", "process_log", "process_wait",
                "process_write", "process_kill",
                "capability_search", "capability_describe", "capability_skill_activate",
                "capability_skill_read_resource", "capability_invoke",
            ])
        )
        XCTAssertEqual(
            NativeLocalAgentPlatformToolCatalog.approvalExemptToolNames,
            [
                "open_edit_session", "stage_edit_batch", "abort_edit_session",
                "capability_invoke",
            ]
        )
    }

    func testExecutorDispatchesTaskExecutionProjectTool() async throws {
        let projectExecutor = RecordingProjectToolExecutor()
        let executor = NativeLocalAgentPlatformToolExecutor(
            host: PlatformToolHostStub(mode: .idle),
            attachmentRootURL: FileManager.default.temporaryDirectory,
            projectTools: projectExecutor
        )
        let invocation = LocalAgentToolInvocationRecord(
            invocationID: "invocation-1",
            runID: "run-1",
            batchID: "batch-1",
            callID: "call-1",
            toolName: "read_file_raw",
            arguments: .object(["path": .string("README.md")]),
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
            claimToken: "token-1",
            claimUntilUnixMs: 2,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )

        let result = try await executor.execute(ownerUserID: "user-1", invocation: invocation)

        XCTAssertEqual(result, .object(["dispatched": .bool(true)]))
        let recorded = await projectExecutor.recordedInvocation()
        XCTAssertEqual(recorded?.toolName, "read_file_raw")
    }

    func testAttachmentVaultResolvesBoundedContentAndRejectsTampering() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("local-agent-attachment-test-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: root) }
        let vault = NativeLocalAgentAttachmentVault(rootURL: root)
        let data = Data("hello local attachment".utf8)
        let spec = try XCTUnwrap(vault.authorize(
            [.init(
                id: "attachment-1",
                name: "notes.txt",
                mimeType: "text/plain",
                kind: .file,
                origin: .pastedText,
                data: data
            )],
            ownerUserID: "owner/a",
            conversationID: "conversation/a"
        ).first)
        let record = attachmentRecord(spec)

        let previewURL = try vault.previewURL(
            record,
            ownerUserID: "owner/a",
            conversationID: "conversation/a"
        )
        XCTAssertTrue(previewURL.isFileURL)
        XCTAssertEqual(try Data(contentsOf: previewURL), data)

        let first = try vault.resolve(
            record,
            ownerUserID: "owner/a",
            conversationID: "conversation/a",
            offset: 0,
            limit: 5
        )
        XCTAssertEqual(first.content, "hello")
        XCTAssertEqual(first.encoding, "utf-8")
        XCTAssertEqual(first.nextOffset, 5)

        XCTAssertThrowsError(try vault.resolve(
            record,
            ownerUserID: "owner/b",
            conversationID: "conversation/a",
            offset: 0,
            limit: 5
        ))

        let file = try XCTUnwrap(try FileManager.default.subpathsOfDirectory(atPath: root.path)
            .map { root.appendingPathComponent($0) }
            .first(where: { !$0.hasDirectoryPath && $0.lastPathComponent.count == 36 }))
        try Data("HELLO local attachment".utf8).write(to: file, options: .atomic)
        XCTAssertThrowsError(try vault.resolve(
            record,
            ownerUserID: "owner/a",
            conversationID: "conversation/a",
            offset: 0,
            limit: 5
        )) { error in
            XCTAssertEqual(
                error as? NativeLocalAgentAttachmentVaultError,
                .integrityMismatch
            )
        }
    }

    func testTypedClientAlwaysExcludesRustReservedTools() async throws {
        let host = PlatformToolHostStub(mode: .idle)
        let client = NativeLocalAgentToolClient(host: host)
        let claim = try await client.claimNext(
            ownerUserID: "user-1",
            workerID: "worker-1",
            excludeToolNames: ["another_reserved_tool"]
        )
        XCTAssertNil(claim)
        let command = try await host.lastCommand()
        guard case let .array(excluded)? = command["exclude_tool_names"] else {
            return XCTFail("missing exclude_tool_names")
        }
        let names = excluded.compactMap { value -> String? in
            guard case let .string(name) = value else { return nil }
            return name
        }
        XCTAssertEqual(Set(names), Set([
            "another_reserved_tool",
            "create_task",
            "create_tasks_with_prerequisites",
        ]))
    }

    func testTypedClientListsAndDecidesDurableToolApproval() async throws {
        let host = PlatformToolHostStub(mode: .onePendingApproval)
        let client = NativeLocalAgentToolClient(host: host)

        let pending = try await client.pendingApprovals(ownerUserID: "user-1", limit: 1)
        let invocation = try XCTUnwrap(pending.first)
        XCTAssertEqual(invocation.toolName, "commit_edit_session")
        XCTAssertEqual(invocation.approvalStatus, "pending")

        let result = try await client.decideApproval(
            ownerUserID: "user-1",
            invocation: invocation,
            approve: true,
            decidedBy: "macos-local-approval",
            reason: "Approved in the local client."
        )
        XCTAssertEqual(result.invocation.approvalStatus, "approved")
        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("decide_tool_approval"))
        XCTAssertEqual(command["expected_version"], .number(2))
        XCTAssertEqual(command["decision"], .string("approve"))
    }

    func testWorkerCommitsSideEffectingExecutionErrorAsNeedsReview() async throws {
        let host = PlatformToolHostStub(mode: .oneSideEffectingClaim)
        let worker = NativeLocalAgentPlatformToolWorker(
            client: .init(host: host),
            executor: FailingPlatformToolExecutor(),
            workerID: "worker-1"
        )
        await worker.configure(ownerUserID: "user-1")

        var outcomeType: LocalAgentJSONValue?
        for _ in 0..<100 where outcomeType == nil {
            outcomeType = try await host.committedOutcomeType()
            if outcomeType == nil {
                try await Task.sleep(for: .milliseconds(10))
            }
        }
        XCTAssertEqual(outcomeType, .string("needs_review"))
        await worker.reset()
    }

    private func attachmentRecord(
        _ spec: LocalAgentConversationAttachmentSpec
    ) -> LocalAgentConversationAttachmentRecord {
        .init(
            attachmentID: spec.attachmentID,
            conversationID: "conversation/a",
            turnID: "turn-1",
            messageID: "message-1",
            ordinal: 1,
            displayName: spec.displayName,
            mediaType: spec.mediaType,
            byteSize: spec.byteSize,
            sha256: spec.sha256,
            authorizedLocalRef: spec.authorizedLocalRef,
            metadata: spec.metadata,
            createdAtUnixMs: 1
        )
    }
}

private actor RecordingProjectToolExecutor: NativeLocalAgentProjectToolExecuting {
    private var invocation: LocalAgentToolInvocationRecord?

    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        self.invocation = invocation
        return .object(["dispatched": .bool(true)])
    }

    func recordedInvocation() -> LocalAgentToolInvocationRecord? { invocation }

    func reset() async {}
}

private struct FailingPlatformToolExecutor: NativeLocalAgentPlatformToolExecuting {
    func execute(
        ownerUserID: String,
        invocation: LocalAgentToolInvocationRecord
    ) async throws -> LocalAgentJSONValue {
        throw CocoaError(.fileReadCorruptFile)
    }
}

private actor PlatformToolHostStub: LocalAgentHostClientServicing {
    enum Mode { case idle, oneSideEffectingClaim, onePendingApproval }

    private let mode: Mode
    private var didClaim = false
    private var commands: [Data] = []

    init(mode: Mode) {
        self.mode = mode
    }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "claim_next_tool":
            guard case .oneSideEffectingClaim = mode, !didClaim else {
                return try json(["type": "tool_claim", "claim": NSNull()])
            }
            didClaim = true
            return try json([
                "type": "tool_claim",
                "claim": [
                    "worker_id": "worker-1",
                    "claim_token": "claim-token-1",
                    "invocation": invocation(status: "running", version: 2),
                ],
            ])
        case "commit_tool":
            return try json([
                "type": "tool_commit",
                "result": [
                    "invocation": invocation(status: "needs_review", version: 3),
                    "run": run(),
                ],
            ])
        case "list_pending_tool_approvals":
            guard case .onePendingApproval = mode else {
                return try json(["type": "pending_tool_approvals", "invocations": []])
            }
            return try json([
                "type": "pending_tool_approvals",
                "invocations": [approvalInvocation(status: "pending", version: 2)],
            ])
        case "decide_tool_approval":
            return try json([
                "type": "tool_approval",
                "result": [
                    "invocation": approvalInvocation(status: "pending", version: 3, approved: true),
                    "run": run(status: "running"),
                ],
            ])
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    func lastCommand() throws -> [String: LocalAgentJSONValue] {
        guard let data = commands.last else { throw CocoaError(.fileNoSuchFile) }
        let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
        guard case let .object(object) = value else { throw CocoaError(.fileReadCorruptFile) }
        return object
    }

    func committedOutcomeType() throws -> LocalAgentJSONValue? {
        for data in commands.reversed() {
            let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
            guard case let .object(object) = value,
                  object["type"] == .string("commit_tool"),
                  case let .object(outcome)? = object["outcome"] else { continue }
            return outcome["type"]
        }
        return nil
    }

    private func invocation(status: String, version: Int) -> [String: Any] {
        [
            "invocation_id": "invocation-1",
            "run_id": "run-1",
            "batch_id": "batch-1",
            "call_id": "call-1",
            "tool_name": "write_local_state",
            "arguments": [:],
            "side_effecting": true,
            "requires_approval": false,
            "approval_status": "not_required",
            "status": status,
            "version": version,
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func approvalInvocation(
        status: String,
        version: Int,
        approved: Bool = false
    ) -> [String: Any] {
        [
            "invocation_id": "approval-invocation-1",
            "run_id": "run-1",
            "batch_id": "batch-1",
            "call_id": "call-1",
            "tool_name": "commit_edit_session",
            "arguments": ["session_id": "session-1"],
            "side_effecting": true,
            "requires_approval": true,
            "approval_status": approved ? "approved" : "pending",
            "approval_decided_by": approved ? "macos-local-approval" : NSNull(),
            "approval_reason": approved ? "Approved in the local client." : NSNull(),
            "approval_decided_at_unix_ms": approved ? 3 : NSNull(),
            "status": status,
            "version": version,
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func run(status: String = "needs_review") -> [String: Any] {
        [
            "run_id": "run-1",
            "owner_user_id": "user-1",
            "owner_entity_type": "conversation_turn",
            "owner_entity_id": "turn-1",
            "profile_key": "main_chat",
            "model_config_ref": "model-1",
            "model_config_revision": "revision-1",
            "capability_policy_revision": "policy-1",
            "input": [:],
            "status": status,
            "iteration": 1,
            "model_attempt": 1,
            "max_iterations": 8,
            "version": 3,
            "checkpoint": NSNull(),
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func json(_ object: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: object)
    }
}
