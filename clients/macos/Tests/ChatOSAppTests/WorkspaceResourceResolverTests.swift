import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSApp

final class WorkspaceResourceResolverTests: XCTestCase {
    func testRegistryOnlySnapshotPreservesPreviouslyResolvedProjectMetadata() {
        var snapshot = WorkspaceSnapshot(
            projects: [
                WorkspaceProject(
                    id: "project-1",
                    name: "Renamed project",
                    rootPath: nil,
                    latestConversationID: nil
                ),
            ],
            contacts: [],
            conversations: []
        )
        snapshot.preserveUnavailableProjectMetadata(from: [
            WorkspaceProject(
                id: "project-1",
                name: "Old name",
                rootPath: "local://connector/device/workspace/project",
                displayRootPath: "/Users/test/project",
                latestConversationID: "conversation-1"
            ),
        ])

        XCTAssertEqual(snapshot.projects.first?.name, "Renamed project")
        XCTAssertEqual(snapshot.projects.first?.rootPath, "local://connector/device/workspace/project")
        XCTAssertEqual(snapshot.projects.first?.displayRootPath, "/Users/test/project")
        XCTAssertEqual(snapshot.projects.first?.latestConversationID, "conversation-1")
    }

    func testContactUsesOnlyGlobalConversationInsteadOfNewerProjectConversation() throws {
        let contact = WorkspaceContact(
            id: "contact-1",
            agentID: "agent-1",
            name: "叽咕狸",
            status: "active"
        )
        let snapshot = WorkspaceSnapshot(
            projects: [
                WorkspaceProject(
                    id: "project-1",
                    name: "项目一",
                    rootPath: nil,
                    latestConversationID: "project-conversation"
                ),
            ],
            contacts: [contact],
            conversations: [
                WorkspaceConversation(
                    id: "project-conversation",
                    title: "项目会话",
                    projectID: "project-1",
                    contactID: contact.id,
                    contactAgentID: contact.agentID,
                    messageCount: 20,
                    updatedAt: Date(timeIntervalSince1970: 2),
                    isArchived: false
                ),
                WorkspaceConversation(
                    id: "global-conversation",
                    title: "全局会话",
                    projectID: nil,
                    contactID: contact.id,
                    contactAgentID: contact.agentID,
                    messageCount: 1,
                    updatedAt: Date(timeIntervalSince1970: 1),
                    isArchived: false
                ),
            ]
        )

        let resources = WorkspaceResourceResolver.resolve(snapshot)

        XCTAssertEqual(try XCTUnwrap(resources.contacts.first).conversationID, "global-conversation")
        XCTAssertEqual(try XCTUnwrap(resources.projects.first).conversationID, "project-conversation")
    }
}
