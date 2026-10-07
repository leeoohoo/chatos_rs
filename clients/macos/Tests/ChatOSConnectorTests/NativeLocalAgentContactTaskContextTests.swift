@testable import ChatOSConnector
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
        let resolver = NativeLocalAgentProjectContextResolver(
            host: ContactTaskContextHostStub(),
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
        let approvalScope = NativeLocalAgentToolApprovalHandler.approvalScope(for: first)
        XCTAssertEqual(approvalScope.rootURL, first.executionRootURL)
        XCTAssertEqual(approvalScope.workspaceID, first.workspaceScopeID)
        XCTAssertThrowsError(try first.requireProject()) { error in
            XCTAssertEqual(error as? NativeLocalAgentPlatformToolError, .projectUnavailable)
        }
    }
}

private struct ContactTaskTicketProvider: LocalConnectorPairingTicketProviding {
    func issueLocalConnectorPairingTicket() async throws -> String { "unused" }
}

private actor ContactTaskContextHostStub: LocalAgentHostClientServicing {
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
                    "input": [
                        "source_conversation_id": "conversation-1",
                        "tool_options": [
                            "requires_execution": true,
                            "enabled_builtin_kinds": ["CodeMaintainerRead"],
                            "plugin_hints": [["plugin_key": "browser-plugin"]],
                        ],
                    ],
                    "status": "running",
                    "version": 1,
                    "terminal_outcome": NSNull(),
                    "created_at_unix_ms": 1,
                    "updated_at_unix_ms": 1,
                ],
            ])
        case "get_conversation":
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
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    private func response(_ object: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: object)
    }
}
