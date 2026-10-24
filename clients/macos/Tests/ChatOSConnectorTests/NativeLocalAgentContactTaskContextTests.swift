@testable import ChatOSConnector
import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentContactTaskContextTests: XCTestCase {
    func testContactTaskGetsStableWorkspaceButNoProjectAuthority() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("contact-task-context-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let stateURL = root.appendingPathComponent("connector-state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "user-1", username: "user", displayName: nil, role: "user")
        state.deviceID = "device-1"
        try JSONEncoder().encode(state).write(to: stateURL)
        let connector = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: stateURL
            ),
            ticketProvider: ContactTaskTicketProvider()
        )
        let projects = NativeLocalProjectsService(
            connector: connector,
            databaseURL: root.appendingPathComponent("projects.sqlite3")
        )
        let host = ContactTaskContextHostStub()
        let resolver = NativeLocalAgentProjectContextResolver(
            host: host,
            projects: projects,
            connector: connector
        )

        let first = try await resolver.resolve(ownerUserID: "user-1", runID: "run-1")
        let second = try await resolver.resolve(ownerUserID: "user-1", runID: "run-1")

        XCTAssertNil(first.projectID)
        XCTAssertNil(first.resolvedPath)
        XCTAssertEqual(first.applicationContext, .device)
        XCTAssertEqual(first.executionRootURL, second.executionRootURL)
        XCTAssertTrue(first.workspaceScopeID.hasPrefix("contact:"))
        XCTAssertTrue(FileManager.default.fileExists(atPath: first.executionRootURL.path))
        XCTAssertTrue(first.toolAuthorization.allows("capability_search"))
        XCTAssertEqual(first.toolAuthorization.ownedPaths, ["reports"])
        let approvalScope = NativeLocalAgentToolApprovalHandler.approvalScope(for: first)
        XCTAssertEqual(approvalScope.rootURL, first.executionRootURL)
        XCTAssertEqual(approvalScope.workspaceID, first.workspaceScopeID)
        XCTAssertEqual(first.executionWorkspace.id, first.workspaceScopeID)
        XCTAssertEqual(first.executionWorkspace.absoluteRoot, first.executionRootURL.path)
        XCTAssertThrowsError(try first.requireProject()) { error in
            XCTAssertEqual(error as? NativeLocalAgentPlatformToolError, .projectUnavailable)
        }

        let executor = NativeLocalAgentProjectToolExecutor(
            host: host,
            projects: projects,
            connector: connector,
            codeReadWorker: EmptyDirectoryCodeReadWorker()
        )
        let listed = try await executor.execute(
            ownerUserID: "user-1",
            invocation: Self.invocation(
                toolName: "list_dir",
                arguments: .object(["path": .string(".")])
            )
        )
        guard case let .object(listResult) = listed,
              case let .array(entries)? = listResult["entries"] else {
            return XCTFail("contact workspace list_dir returned an invalid result")
        }
        XCTAssertTrue(entries.isEmpty)

        let opened = try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation(
            toolName: "open_edit_session", arguments: .object([:])
        ))
        guard case let .object(openResult) = opened,
              case let .object(editResult)? = openResult["result"],
              case let .string(editID)? = editResult["session_id"] else {
            return XCTFail("Task edit session did not open")
        }
        func stageArguments(_ path: String) -> LocalAgentJSONValue {
            .object([
                "session_id": .string(editID), "operations": .array([.object([
                    "kind": .string("write"), "path": .string(path),
                    "content": .string("Task report"), "expected_sha256": .null,
                ])]),
            ])
        }
        do {
            _ = try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation(
                toolName: "stage_edit_batch", arguments: stageArguments("outside.txt")
            ))
            XCTFail("Task executor must pass frozen ownership to the editor")
        } catch let error as NativeLocalAgentPlatformToolError {
            guard case let .projectToolRejected(message) = error else {
                return XCTFail("unexpected ownership error: \(error)")
            }
            XCTAssertTrue(message.contains("owned_paths"))
        }
        _ = try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation(
            toolName: "stage_edit_batch", arguments: stageArguments("reports/result.txt")
        ))
        do {
            _ = try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation(
                toolName: "commit_edit_session", arguments: .object(["session_id": .string(editID)])
            ))
            XCTFail("Task write commit requires durable approval")
        } catch let error as NativeLocalAgentPlatformToolError {
            XCTAssertEqual(error, .approvalRequired)
        }
        XCTAssertFalse(FileManager.default.fileExists(atPath: first.executionRootURL.appendingPathComponent("reports/result.txt").path))
        _ = try await executor.execute(ownerUserID: "user-1", invocation: Self.invocation(
            toolName: "commit_edit_session", arguments: .object(["session_id": .string(editID)]),
            requiresApproval: true, approvalStatus: "approved"
        ))
        XCTAssertEqual(try String(contentsOf: first.executionRootURL.appendingPathComponent("reports/result.txt"), encoding: .utf8), "Task report")
        XCTAssertFalse(FileManager.default.fileExists(atPath: first.executionRootURL.appendingPathComponent("outside.txt").path))

        let terminal = try await executor.execute(
            ownerUserID: "user-1",
            invocation: Self.invocation(
                toolName: "execute_command",
                arguments: .object([
                    "purpose": .string("确认联系人任务使用隔离的执行目录"),
                    "command": .string("pwd"),
                ]),
                requiresApproval: true,
                approvalStatus: "approved"
            )
        )
        guard case let .object(terminalResult) = terminal,
              case let .string(output)? = terminalResult["output"] else {
            return XCTFail("contact workspace terminal returned an invalid result")
        }
        let terminalPath = output.trimmingCharacters(in: .whitespacesAndNewlines)
        XCTAssertTrue(
            [first.executionRootURL.path, "/private" + first.executionRootURL.path]
                .contains(terminalPath)
        )

        let attachmentRoot = root.appendingPathComponent("attachments", isDirectory: true)
        let vault = NativeLocalAgentAttachmentVault(rootURL: attachmentRoot)
        let bytes = Data(repeating: 0xff, count: 120_000)
        let spec = try XCTUnwrap(vault.authorize([
            .init(id: "attachment-1", name: "input.docx", mimeType: "application/octet-stream",
                  kind: .file, origin: .pastedText, data: bytes),
        ], ownerUserID: "user-1", conversationID: "conversation-1").first)
        await host.setAttachment(spec)
        let platform = NativeLocalAgentPlatformToolExecutor(
            host: host, attachmentRootURL: attachmentRoot, projectTools: executor,
            attachmentWorkspaceResolver: resolver,
            codeReadWorker: ConnectorCoreCodeReadWorker()
        )
        let imported = try await platform.execute(ownerUserID: "user-1", invocation: Self.invocation(
            toolName: "local_attachment_read",
            arguments: .object(["authorized_local_ref": .string(spec.authorizedLocalRef)])
        ))
        guard case let .object(importResult) = imported,
              case let .string(relativePath)? = importResult["workspace_relative_path"] else {
            return XCTFail("contact attachment import returned no file path")
        }
        XCTAssertEqual(try Data(contentsOf: first.executionRootURL.appendingPathComponent(relativePath)), bytes)
        XCTAssertNil(importResult["content"])
    }

    private static func invocation(
        toolName: String,
        arguments: LocalAgentJSONValue,
        requiresApproval: Bool = false,
        approvalStatus: String = "not_required"
    ) -> LocalAgentToolInvocationRecord {
        .init(
            invocationID: "invocation-\(toolName)",
            runID: "run-1",
            batchID: "batch-1",
            callID: "call-\(toolName)",
            toolName: toolName,
            executionRoute: "platform",
            arguments: arguments,
            sideEffecting: requiresApproval,
            requiresApproval: requiresApproval,
            approvalStatus: approvalStatus,
            approvalDecidedBy: requiresApproval ? "user" : nil,
            approvalReason: nil,
            approvalDecidedAtUnixMs: requiresApproval ? 1 : nil,
            status: "running",
            result: nil,
            error: nil,
            version: 1,
            claimToken: "claim-token",
            claimUntilUnixMs: 2,
            createdAtUnixMs: 1,
            updatedAtUnixMs: 1
        )
    }
}

private struct EmptyDirectoryCodeReadWorker: CodeReadWorkerExecuting {
    func execute(
        operation: String,
        arguments _: [String: CodeReadWorkerJSONValue],
        projectRoot _: URL,
        requestCWD _: String?,
        defaultToolRoot _: String?,
        ripgrepURL _: URL?
    ) async throws -> CodeReadWorkerJSONValue {
        XCTAssertEqual(operation, "list_dir")
        return .object(["entries": .array([]), "truncated": .bool(false)])
    }
}

private struct ContactTaskTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}

private actor ContactTaskContextHostStub: LocalAgentHostClientServicing {
    private var attachment: LocalAgentConversationAttachmentSpec?

    func setAttachment(_ spec: LocalAgentConversationAttachmentSpec) { attachment = spec }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "get_run":
            return try response([
                "type": "run",
                "run": [
                    "run_id": "run-1",
                    "owner_user_id": "user-1",
                    "owner_entity_type": "task",
                    "owner_entity_id": "task-1",
                    "profile_key": "task_execution",
                    "capability_policy_revision": "policy-1",
                    "input": [
                        "source_conversation_id": "conversation-1",
                        "owned_paths": ["reports"],
                        "tool_options": [
                            "requires_execution": true,
                            "enabled_builtin_kinds": [
                                "CodeMaintainerRead", "CodeMaintainerWrite", "TerminalController",
                            ],
                            "plugin_hints": [],
                            "plugin_release_snapshots": [],
                        ],
                    ],
                    "status": "running",
                    "version": 1,
                    "terminal_outcome": NSNull(),
                    "created_at_unix_ms": 1,
                    "updated_at_unix_ms": 1,
                ],
            ])
        case "get_conversation_context":
            return try response([
                "type": "conversation",
                "conversation": [
                    "conversation": [
                        "conversation_id": "conversation-1",
                        "owner_user_id": "user-1",
                        "title": "叽咕哩",
                        "resource": ["kind": "contact", "resource_id": "jiguli"],
                        "version": 1,
                        "created_at_unix_ms": 1,
                        "updated_at_unix_ms": 1,
                    ],
                    "turns": [],
                    "messages": [],
                    "attachments": [],
                ],
            ])
        case "get_conversation_attachment":
            let authorizedLocalRef = object?["authorized_local_ref"] as? String
            let attachment: Any
            if let matchingAttachment = attachments().first(where: {
                $0["authorized_local_ref"] as? String == authorizedLocalRef
            }) {
                attachment = matchingAttachment
            } else {
                attachment = NSNull()
            }
            return try response([
                "type": "conversation_attachment",
                "attachment": attachment,
            ])
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    private func response(_ object: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: object)
    }

    private func attachments() -> [[String: Any]] {
        guard let attachment else { return [] }
        return [[
            "attachment_id": attachment.attachmentID, "conversation_id": "conversation-1",
            "turn_id": "turn-1", "message_id": "message-1", "ordinal": 1,
            "display_name": attachment.displayName, "media_type": attachment.mediaType,
            "byte_size": attachment.byteSize, "sha256": attachment.sha256,
            "authorized_local_ref": attachment.authorizedLocalRef,
            "metadata": [:], "created_at_unix_ms": 1,
        ]]
    }
}
