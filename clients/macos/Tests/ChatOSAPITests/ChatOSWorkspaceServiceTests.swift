import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSWorkspaceServiceTests: XCTestCase {
    func testRelationsUseOnlyContactsAndConversations() async throws {
        let transport = WorkspaceTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token", transport: transport
        )
        let relations = try await ChatOSWorkspaceService(client: client).fetchWorkspaceRelations()
        let paths = await transport.requestPaths()
        XCTAssertEqual(Set(paths), ["/api/chatos/contacts", "/api/chatos/conversations"])
        XCTAssertEqual(relations.conversations.first?.projectID, "project-1")
    }

    func testWorkspaceLoadsGatewayResourcesAndResolvesConversationMetadata() async throws {
        let transport = WorkspaceTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        let snapshot = try await ChatOSWorkspaceService(client: client).fetchWorkspaceRelations()

        XCTAssertEqual(snapshot.contacts.first?.name, "叽咕狸")
        let conversation = try XCTUnwrap(snapshot.conversations.first)
        XCTAssertEqual(conversation.projectID, "project-1")
        XCTAssertEqual(conversation.contactID, "contact-1")
        XCTAssertEqual(conversation.contactAgentID, "agent-1")
        XCTAssertEqual(conversation.messageCount, 12)
        XCTAssertNil(snapshot.conversations.last?.projectID)

        let paths = await transport.requestPaths()
        XCTAssertEqual(
            Set(paths),
            Set([
                "/api/chatos/contacts",
                "/api/chatos/conversations",
            ])
        )
    }

}

private actor WorkspaceTransport: HTTPTransport {
    private var paths: [String] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        paths.append(request.url.path)
        let body: String
        switch request.url.path {
        case "/api/chatos/contacts":
            body = #"[{"id":"contact-1","agent_id":"agent-1","agent_name_snapshot":"叽咕狸","status":"active"}]"#
        case "/api/chatos/conversations":
            body = #"[{"id":"conversation-1","title":"真实会话","project_id":"project-1","message_count":12,"updated_at":"2026-08-24T05:30:00Z","archived":false,"status":"active","metadata":{"chat_runtime":{"project_id":"project-1","contact_agent_id":"agent-1"},"contact":{"contact_id":"contact-1","agent_id":"agent-1"}}},{"id":"conversation-global","title":"叽咕狸","project_id":"-1","message_count":1,"metadata":{"legacy_session_mapping":{"project_id":"-1"},"source_metadata":{"contact":{"contact_id":"contact-1","agent_id":"agent-1"}}}}]"#
        default:
            return HTTPResponse(statusCode: 404, headers: [:], body: Data())
        }
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func requestPaths() -> [String] { paths }
}
