@testable import ChatOSConnector
import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentTeamProjectContextTests: XCTestCase {
    func testTeamExecutorReadsAndWritesBoundProjectWithoutConversationResource() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("team-project-context-\(UUID().uuidString)", isDirectory: true)
        let projectRoot = root.appendingPathComponent("repo", isDirectory: true)
        try FileManager.default.createDirectory(at: projectRoot, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let stateURL = root.appendingPathComponent("connector-state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "user-1", username: "user", displayName: nil, role: "user")
        state.deviceID = "device-1"
        state.workspaces = [.init(
            id: "workspace-1",
            alias: "workspace",
            absoluteRoot: root.path,
            fingerprint: "fingerprint-1"
        )]
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let connector = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: stateURL
            ),
            ticketProvider: TeamProjectTicketProvider()
        )
        let projects = NativeLocalProjectsService(
            connector: connector,
            databaseURL: root.appendingPathComponent("projects.sqlite3")
        )
        let project = try await projects.create(
            ownerUserID: "user-1",
            draft: .init(
                name: "Team Project",
                workspaceID: "workspace-1",
                relativeRoot: "repo"
            )
        )
        let host = TeamProjectContextHostStub(projectID: project.id)
        let executor = NativeLocalAgentProjectToolExecutor(
            host: host,
            projects: projects,
            connector: connector,
            codeReadWorker: TeamProjectCodeReadWorker(expectedRoot: projectRoot)
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
            return XCTFail("team project list_dir returned an invalid result")
        }
        XCTAssertTrue(entries.isEmpty)

        let opened = try await executor.execute(
            ownerUserID: "user-1",
            invocation: Self.invocation(toolName: "open_edit_session", arguments: .object([:]))
        )
        guard case let .object(openResult) = opened,
              case let .object(editResult)? = openResult["result"],
              case let .string(editID)? = editResult["session_id"] else {
            return XCTFail("team project edit session did not open")
        }
        _ = try await executor.execute(
            ownerUserID: "user-1",
            invocation: Self.invocation(
                toolName: "stage_edit_batch",
                arguments: .object([
                    "session_id": .string(editID),
                    "operations": .array([.object([
                        "kind": .string("write"),
                        "path": .string("team-result.md"),
                        "content": .string("Team executor can write the project."),
                        "expected_sha256": .null,
                    ])]),
                ])
            )
        )
        _ = try await executor.execute(
            ownerUserID: "user-1",
            invocation: Self.invocation(
                toolName: "commit_edit_session",
                arguments: .object(["session_id": .string(editID)]),
                requiresApproval: true,
                approvalStatus: "approved"
            )
        )

        XCTAssertEqual(
            try String(
                contentsOf: projectRoot.appendingPathComponent("team-result.md"),
                encoding: .utf8
            ),
            "Team executor can write the project."
        )
    }

    func testTeamExecutorRejectsMismatchedProjectIdentity() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("team-project-identity-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let stateURL = root.appendingPathComponent("connector-state.json")
        var state = NativeConnectorPersistentState.empty
        state.user = .init(id: "user-1", username: "user", displayName: nil, role: "user")
        state.deviceID = "device-1"
        try NativeConnectorStateStore(stateURL: stateURL).save(state)
        let connector = NativeLocalConnectorService(
            configuration: .init(
                gatewayBaseURL: URL(string: "http://127.0.0.1:1")!,
                stateURL: stateURL
            ),
            ticketProvider: TeamProjectTicketProvider()
        )
        let resolver = NativeLocalAgentProjectContextResolver(
            host: TeamProjectContextHostStub(
                projectID: "project-1",
                identityProjectID: "other-project"
            ),
            projects: NativeLocalProjectsService(
                connector: connector,
                databaseURL: root.appendingPathComponent("projects.sqlite3")
            ),
            connector: connector
        )

        do {
            _ = try await resolver.resolve(ownerUserID: "user-1", runID: "run-1")
            XCTFail("mismatched team project identity must be rejected")
        } catch let error as NativeLocalAgentPlatformToolError {
            XCTAssertEqual(error, .invalidRunContext)
        }
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

private struct TeamProjectCodeReadWorker: CodeReadWorkerExecuting {
    let expectedRoot: URL

    func execute(
        operation: String,
        arguments _: [String: CodeReadWorkerJSONValue],
        projectRoot: URL,
        requestCWD _: String?,
        defaultToolRoot _: String?,
        ripgrepURL _: URL?
    ) async throws -> CodeReadWorkerJSONValue {
        XCTAssertEqual(operation, "list_dir")
        XCTAssertEqual(projectRoot.standardizedFileURL, expectedRoot.standardizedFileURL)
        return .object(["entries": .array([]), "truncated": .bool(false)])
    }
}

private struct TeamProjectTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}

private actor TeamProjectContextHostStub: LocalAgentHostClientServicing {
    let projectID: String
    let identityProjectID: String

    init(projectID: String, identityProjectID: String? = nil) {
        self.projectID = projectID
        self.identityProjectID = identityProjectID ?? projectID
    }

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
                    "owner_entity_type": "conversation_turn",
                    "owner_entity_id": "turn-1",
                    "profile_key": "task_execution",
                    "capability_policy_revision": "policy-1",
                    "input": [
                        "conversation_id": "agent-team-executor-delivery-1",
                        "project_id": projectID,
                        "owned_paths": [],
                        "tool_options": [
                            "requires_execution": true,
                            "enabled_builtin_kinds": [
                                "CodeMaintainerRead", "CodeMaintainerWrite",
                            ],
                            "plugin_hints": [],
                            "plugin_release_snapshots": [],
                            "external_mcp_config_ids": [],
                        ],
                        "team_executor_context": [
                            "schema_version": 1,
                            "delivery_id": "delivery-1",
                            "todo_id": "todo-1",
                            "team_id": "team-1",
                            "agent_id": "agent-1",
                            "project_id": projectID,
                        ],
                        "agent_execution_context": [
                            "schema_version": 2,
                            "room_id": "team-1",
                            "agent_id": "agent-1",
                            "project_id": identityProjectID,
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
                        "conversation_id": "agent-team-executor-delivery-1",
                        "owner_user_id": "user-1",
                        "title": "Team Project · Todo",
                        "resource": NSNull(),
                        "version": 1,
                        "created_at_unix_ms": 1,
                        "updated_at_unix_ms": 1,
                    ],
                    "turns": [],
                    "messages": [],
                    "attachments": [],
                ],
            ])
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    private func response(_ object: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: object)
    }
}
