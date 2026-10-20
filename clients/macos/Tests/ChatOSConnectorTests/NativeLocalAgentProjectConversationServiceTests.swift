@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentProjectConversationServiceTests: XCTestCase {
    func testExistingProjectConversationIsReusedWithoutCreatingAnother() async throws {
        let host = ProjectConversationHostStub(
            initialConversations: [conversation(id: "conversation-existing", projectID: "project-1")]
        )
        let service = makeService(host: host)
        await service.configure(ownerUserID: "owner-1")

        let conversationID = try await service.ensureConversation(
            project: try project(id: "project-1"),
            contact: contact
        )

        XCTAssertEqual(conversationID, "conversation-existing")
        let commandTypes = await host.commandTypes()
        XCTAssertEqual(commandTypes, ["list_conversations"])
    }

    func testInvalidProjectContextIsRejectedBeforeHostRequest() async throws {
        let host = ProjectConversationHostStub()
        let service = makeService(host: host)
        await service.configure(ownerUserID: "owner-1")
        var invalid = try project(id: "project-1")
        invalid.projectContext = try projectContext(projectID: "another-project")

        do {
            _ = try await service.ensureConversation(project: invalid, contact: contact)
            XCTFail("A mismatched project context must be rejected")
        } catch {
            XCTAssertEqual(error as? ProjectRegistryError, .invalidField("projectContext"))
        }
        let commandTypes = await host.commandTypes()
        XCTAssertEqual(commandTypes, [])
    }

    func testCreatedConversationUsesAuthenticatedOwnerAndProjectBinding() async throws {
        let host = ProjectConversationHostStub()
        let service = makeService(host: host)
        await service.configure(ownerUserID: "owner-1")

        let conversationID = try await service.ensureConversation(
            project: try project(id: "project-1"),
            contact: contact
        )

        XCTAssertTrue(conversationID.hasPrefix("conversation_"))
        let createCommand = await host.lastCreateCommand()
        let commandData = try XCTUnwrap(createCommand)
        let command = try XCTUnwrap(
            JSONSerialization.jsonObject(with: commandData) as? [String: Any]
        )
        XCTAssertEqual(command["owner_user_id"] as? String, "owner-1")
        XCTAssertEqual(command["title"] as? String, "Project project-1")
        let resource = try XCTUnwrap(command["resource"] as? [String: Any])
        XCTAssertEqual(resource["kind"] as? String, "project")
        XCTAssertEqual(resource["resource_id"] as? String, "project-1")
    }

    func testConcurrentCreateFailureRechecksBindingAndUsesWinningConversation() async throws {
        let winner = conversation(id: "conversation-winner", projectID: "project-1")
        let host = ProjectConversationHostStub(
            conversationsAfterCreateFailure: [winner]
        )
        let service = makeService(host: host)
        await service.configure(ownerUserID: "owner-1")

        let conversationID = try await service.ensureConversation(
            project: try project(id: "project-1"),
            contact: contact
        )

        XCTAssertEqual(conversationID, "conversation-winner")
        let commandTypes = await host.commandTypes()
        XCTAssertEqual(
            commandTypes,
            ["list_conversations", "create_conversation", "list_conversations"]
        )
    }

    private func makeService(
        host: ProjectConversationHostStub
    ) -> NativeLocalAgentProjectConversationService {
        NativeLocalAgentProjectConversationService(
            host: host,
            workspace: NativeLocalAgentWorkspaceService(host: host)
        )
    }

    private func project(id: String) throws -> WorkspaceProject {
        WorkspaceProject(
            id: id,
            name: "Project \(id)",
            rootPath: nil,
            latestConversationID: nil,
            projectContext: try projectContext(projectID: id)
        )
    }

    private func projectContext(projectID: String) throws -> ProjectContextSnapshot {
        try ProjectContextSnapshot(
            record: LocalProjectRecord(
                id: projectID,
                ownerUserID: "owner-1",
                draft: LocalProjectDraft(name: "Project \(projectID)", workspaceID: "workspace-1"),
                createdAtUnixMs: 1,
                updatedAtUnixMs: 1
            ),
            deviceID: "device-1"
        )
    }

    private var contact: WorkspaceContact {
        WorkspaceContact(id: "jiguli", agentID: "jiguli", name: "叽咕狸", status: "active")
    }

    private func conversation(id: String, projectID: String) -> ProjectConversationHostStub.Record {
        .init(
            conversationID: id,
            ownerUserID: "owner-1",
            title: "Project \(projectID)",
            projectID: projectID
        )
    }
}

private actor ProjectConversationHostStub: LocalAgentHostClientServicing {
    struct Record: Sendable {
        let conversationID: String
        let ownerUserID: String
        let title: String
        let projectID: String
    }

    private let initialConversations: [Record]
    private let conversationsAfterCreateFailure: [Record]?
    private var commands: [Data] = []

    init(
        initialConversations: [Record] = [],
        conversationsAfterCreateFailure: [Record]? = nil
    ) {
        self.initialConversations = initialConversations
        self.conversationsAfterCreateFailure = conversationsAfterCreateFailure
    }

    func start(ownerUserID: String) async throws {}

    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "list_conversations":
            let listCount = commands.reduce(into: 0) { count, data in
                guard let value = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
                      value["type"] as? String == "list_conversations" else { return }
                count += 1
            }
            let records = if listCount > 1 {
                conversationsAfterCreateFailure ?? initialConversations
            } else {
                initialConversations
            }
            return try JSONSerialization.data(withJSONObject: [
                "type": "conversations",
                "page": [
                    "conversations": records.map(Self.json),
                    "next_before_updated_at_unix_ms": NSNull(),
                    "next_before_conversation_id": NSNull(),
                ],
            ])
        case "create_conversation":
            if conversationsAfterCreateFailure != nil {
                throw URLError(.cannotWriteToFile)
            }
            let conversationID = object?["conversation_id"] as? String ?? "conversation-created"
            let ownerUserID = object?["owner_user_id"] as? String ?? "owner-1"
            let title = object?["title"] as? String ?? "Project"
            let resource = object?["resource"] as? [String: Any]
            let projectID = resource?["resource_id"] as? String ?? "project-1"
            return try JSONSerialization.data(withJSONObject: [
                "type": "conversation",
                "conversation": [
                    "conversation": Self.json(.init(
                        conversationID: conversationID,
                        ownerUserID: ownerUserID,
                        title: title,
                        projectID: projectID
                    )),
                    "turns": [],
                    "messages": [],
                    "attachments": [],
                ],
            ])
        default:
            throw NativeLocalAgentHostError.invalidResponse
        }
    }

    func commandTypes() -> [String] {
        commands.compactMap { data in
            guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { return nil }
            return object["type"] as? String
        }
    }

    func lastCreateCommand() -> Data? {
        commands.last { data in
            guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
            else { return false }
            return object["type"] as? String == "create_conversation"
        }
    }

    private static func json(_ record: Record) -> [String: Any] {
        [
            "conversation_id": record.conversationID,
            "owner_user_id": record.ownerUserID,
            "title": record.title,
            "resource": ["kind": "project", "resource_id": record.projectID],
            "version": 1,
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 1,
        ]
    }
}
