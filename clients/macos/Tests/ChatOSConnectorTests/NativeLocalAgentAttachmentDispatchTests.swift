@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentAttachmentDispatchTests: XCTestCase {
    func testContactAttachmentReadsWithoutProjectExecutorOrBuiltinGrants() async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        let executor = try fixture.executor()
        let first = try await executor.execute(
            ownerUserID: "user-1", invocation: fixture.invocation(limit: 3)
        )
        guard case let .object(result) = first else { return XCTFail("Expected attachment bytes") }
        XCTAssertEqual(result["encoding"], .string("base64"))
        XCTAssertEqual(result["content"], .string(fixture.bytes.prefix(3).base64EncodedString()))
        XCTAssertEqual(result["next_offset"], .number(3))
        XCTAssertEqual(result["sha256"], .string(fixture.spec.sha256))
        let tail = try await executor.execute(
            ownerUserID: "user-1", invocation: fixture.invocation(offset: 3)
        )
        guard case let .object(rest) = tail else { return XCTFail("Expected remaining bytes") }
        XCTAssertEqual(rest["next_offset"], .null)
        XCTAssertEqual(rest["content"], .string(fixture.bytes.dropFirst(3).base64EncodedString()))
    }

    func testAttachmentBypassesProjectCapabilityDispatch() async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        let projectTools = RejectingAttachmentProjectExecutor()
        let result = try await fixture.executor(projectTools: projectTools).execute(
            ownerUserID: "user-1", invocation: fixture.invocation()
        )
        guard case let .object(value) = result else { return XCTFail("Expected attachment bytes") }
        XCTAssertEqual(value["content"], .string(fixture.bytes.base64EncodedString()))
        let calls = await projectTools.callCount()
        XCTAssertEqual(calls, 0)
    }

    func testForeignRunOwnerIsRejected() async throws {
        try await assertRejected(runOwner: "other-user", expected: .invalidRunContext)
    }

    func testForeignConversationOwnerIsRejected() async throws {
        try await assertRejected(conversationOwner: "other-user", expected: .invalidRunContext)
    }

    func testForeignConversationResponseIsRejected() async throws {
        try await assertRejected(conversationID: "other-conversation", expected: .invalidRunContext)
    }

    func testForeignAttachmentRecordIsRejected() async throws {
        try await assertRejected(attachmentConversationID: "other-conversation", expected: .attachmentNotAuthorized)
    }

    func testUnregisteredReferenceIsRejected() async throws {
        try await assertRejected(reference: "local-attachment:" + UUID().uuidString.lowercased(), expected: .attachmentNotAuthorized)
    }

    func testReadLimitRemainsBounded() async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        do {
            _ = try await fixture.executor().execute(
                ownerUserID: "user-1",
                invocation: fixture.invocation(limit: NativeLocalAgentAttachmentVault.maximumReadBytes + 1)
            )
            XCTFail("Oversized reads must fail")
        } catch {
            XCTAssertEqual(error as? NativeLocalAgentPlatformToolError, .invalidArguments)
        }
    }

    func testTamperedAttachmentIsRejectedThroughRealDispatch() async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        let token = String(fixture.spec.authorizedLocalRef.dropFirst(NativeLocalAgentAttachmentVault.referencePrefix.count))
        let file = fixture.root
            .appendingPathComponent(NativePluginManifestLoader.sha256("user-1"))
            .appendingPathComponent(NativePluginManifestLoader.sha256("conversation-1"))
            .appendingPathComponent(token)
        XCTAssertTrue(FileManager.default.fileExists(atPath: file.path))
        try Data(repeating: 0, count: fixture.bytes.count).write(to: file, options: .atomic)
        do {
            _ = try await fixture.executor().execute(ownerUserID: "user-1", invocation: fixture.invocation())
            XCTFail("Tampered attachment must fail")
        } catch {
            XCTAssertEqual(error as? NativeLocalAgentAttachmentVaultError, .integrityMismatch)
        }
    }

    private func assertRejected(
        runOwner: String = "user-1",
        conversationOwner: String = "user-1",
        conversationID: String = "conversation-1",
        attachmentConversationID: String = "conversation-1",
        reference: String? = nil,
        expected: NativeLocalAgentPlatformToolError
    ) async throws {
        let fixture = try AttachmentDispatchFixture()
        defer { fixture.cleanup() }
        let executor = try fixture.executor(
            runOwner: runOwner, conversationOwner: conversationOwner,
            conversationID: conversationID, attachmentConversationID: attachmentConversationID
        )
        do {
            _ = try await executor.execute(
                ownerUserID: "user-1", invocation: fixture.invocation(reference: reference)
            )
            XCTFail("Unauthorized attachment access must fail")
        } catch {
            XCTAssertEqual(error as? NativeLocalAgentPlatformToolError, expected)
        }
    }
}

struct AttachmentDispatchFixture {
    let root: URL
    // Opaque binary fixture: no résumé/private data is used by these tests.
    let bytes: Data
    let spec: LocalAgentConversationAttachmentSpec

    init(bytes: Data = Data([0x50, 0x4b, 0xff, 0x00, 0xfe, 0x80]), name: String = "test.docx",
         mediaType: String = "application/vnd.openxmlformats-officedocument.wordprocessingml.document") throws {
        self.bytes = bytes
        root = FileManager.default.temporaryDirectory
            .appendingPathComponent("attachment-dispatch-\(UUID().uuidString)", isDirectory: true)
        let vault = NativeLocalAgentAttachmentVault(rootURL: root)
        spec = try XCTUnwrap(vault.authorize([
            .init(id: "attachment-1", name: name,
                  mimeType: mediaType,
                  kind: .file, origin: .pastedText, data: bytes),
        ], ownerUserID: "user-1", conversationID: "conversation-1").first)
    }

    func cleanup() { try? FileManager.default.removeItem(at: root) }

    func executor(
        projectTools: (any NativeLocalAgentProjectToolExecuting)? = nil,
        runOwner: String = "user-1",
        conversationOwner: String = "user-1",
        conversationID: String = "conversation-1",
        attachmentConversationID: String = "conversation-1",
        workspaceResolver: (any NativeLocalAgentAttachmentWorkspaceResolving)? = nil,
        mainChat: Bool = false
    ) throws -> NativeLocalAgentPlatformToolExecutor {
        let run: [String: Any] = [
            "type": "run",
            "run": [
                "run_id": "run-1", "owner_user_id": runOwner,
                "owner_entity_type": mainChat ? "conversation" : "task", "owner_entity_id": mainChat ? "conversation-1" : "task-1",
                "profile_key": mainChat ? "main_chat" : "task_execution", "status": "running", "version": 1,
                "created_at_unix_ms": 1, "updated_at_unix_ms": 1,
                "input": [
                    (mainChat ? "conversation_id" : "source_conversation_id"): "conversation-1",
                    "tool_options": [
                        "requires_execution": false, "enabled_builtin_kinds": [], "plugin_hints": [],
                    ],
                ],
            ],
        ]
        let conversation: [String: Any] = [
            "type": "conversation",
            "conversation": [
                "conversation": [
                    "conversation_id": conversationID, "owner_user_id": conversationOwner,
                    "title": "Contact", "resource": ["kind": "contact", "resource_id": "contact-1"],
                    "version": 1, "created_at_unix_ms": 1, "updated_at_unix_ms": 1,
                ],
                "turns": [], "messages": [],
                "attachments": [[
                    "attachment_id": spec.attachmentID, "conversation_id": attachmentConversationID,
                    "turn_id": "turn-1", "message_id": "message-1", "ordinal": 1,
                    "display_name": spec.displayName, "media_type": spec.mediaType,
                    "byte_size": spec.byteSize, "sha256": spec.sha256,
                    "authorized_local_ref": spec.authorizedLocalRef,
                    "metadata": [:], "created_at_unix_ms": 1,
                ]],
            ],
        ]
        return .init(
            host: AttachmentDispatchHost(
                run: try JSONSerialization.data(withJSONObject: run),
                conversation: try JSONSerialization.data(withJSONObject: conversation)
            ), attachmentRootURL: root, projectTools: projectTools,
            attachmentWorkspaceResolver: workspaceResolver
        )
    }

    func invocation(reference: String? = nil, offset: Int = 0, limit: Int = 65_536, delivery: String? = "content") -> LocalAgentToolInvocationRecord {
        var arguments: [String: LocalAgentJSONValue] = [
            "authorized_local_ref": .string(reference ?? spec.authorizedLocalRef),
            "offset": .number(Double(offset)), "limit": .number(Double(limit)),
        ]
        if let delivery { arguments["delivery"] = .string(delivery) }
        return .init(
            invocationID: "invocation-1", runID: "run-1", batchID: "batch-1", callID: "call-1",
            toolName: NativeLocalAgentPlatformToolCatalog.attachmentReadToolName,
            executionRoute: "platform",
            arguments: .object(arguments), sideEffecting: false, requiresApproval: false, approvalStatus: "not_required",
            approvalDecidedBy: nil, approvalReason: nil, approvalDecidedAtUnixMs: nil,
            status: "running", result: nil, error: nil, version: 1, claimToken: "claim-1",
            claimUntilUnixMs: 2, createdAtUnixMs: 1, updatedAtUnixMs: 1
        )
    }
}

private actor AttachmentDispatchHost: LocalAgentHostClientServicing {
    let run: Data
    let conversation: Data

    init(run: Data, conversation: Data) { self.run = run; self.conversation = conversation }
    func start(ownerUserID: String) async throws {}
    func stop() async {}
    func request(command: Data) async throws -> Data {
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "get_run": return run
        case "get_conversation": return conversation
        default: throw CocoaError(.featureUnsupported)
        }
    }
}

private actor RejectingAttachmentProjectExecutor: NativeLocalAgentProjectToolExecuting {
    private var calls = 0
    func execute(ownerUserID: String, invocation: LocalAgentToolInvocationRecord) async throws -> LocalAgentJSONValue {
        calls += 1
        throw NativeLocalAgentPlatformToolError.capabilityNotSelected
    }
    func callCount() -> Int { calls }
    func reset() async {}
}
