import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSWorkspaceServiceTests: XCTestCase {
    func testWorkspaceLoadsGatewayResourcesAndResolvesConversationMetadata() async throws {
        let transport = WorkspaceTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        let snapshot = try await ChatOSWorkspaceService(client: client).fetchWorkspace()

        XCTAssertEqual(snapshot.projects.first?.latestConversationID, "conversation-1")
        XCTAssertEqual(snapshot.contacts.first?.name, "叽咕狸")
        let conversation = try XCTUnwrap(snapshot.conversations.first)
        XCTAssertEqual(conversation.projectID, "project-1")
        XCTAssertEqual(conversation.contactID, "contact-1")
        XCTAssertEqual(conversation.contactAgentID, "agent-1")
        XCTAssertEqual(conversation.messageCount, 12)

        let paths = await transport.requestPaths()
        XCTAssertEqual(
            Set(paths),
            Set([
                "/api/chatos/projects",
                "/api/chatos/contacts",
                "/api/chatos/conversations",
            ])
        )
    }

    func testDeleteProjectUsesEncodedProjectPathAndDeleteMethod() async throws {
        let transport = ProjectDeletionTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )

        try await ChatOSWorkspaceService(client: client).deleteProject(id: "project/1")

        let recordedRequest = await transport.recordedRequest()
        let request = try XCTUnwrap(recordedRequest)
        XCTAssertEqual(request.method, "DELETE")
        XCTAssertEqual(request.path, "/api/chatos/projects/project%2F1")
    }
}

private actor ProjectDeletionTransport: HTTPTransport {
    struct RecordedRequest: Sendable {
        var method: String
        var path: String
    }

    private var request: RecordedRequest?

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        self.request = .init(
            method: request.method,
            path: request.url.path(percentEncoded: true)
        )
        return HTTPResponse(
            statusCode: 200,
            headers: [:],
            body: Data(#"{"success":true,"message":"项目已删除"}"#.utf8)
        )
    }

    func recordedRequest() -> RecordedRequest? { request }
}

private actor WorkspaceTransport: HTTPTransport {
    private var paths: [String] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        paths.append(request.url.path)
        let body: String
        switch request.url.path {
        case "/api/chatos/projects":
            body = #"[{"id":"project-1","name":"Real Project","root_path":"/workspace/real","display_root_path":null,"latest_session_id":"conversation-1"}]"#
        case "/api/chatos/contacts":
            body = #"[{"id":"contact-1","agent_id":"agent-1","agent_name_snapshot":"叽咕狸","status":"active"}]"#
        case "/api/chatos/conversations":
            body = #"[{"id":"conversation-1","title":"真实会话","project_id":"project-1","message_count":12,"updated_at":"2026-08-24T05:30:00Z","archived":false,"status":"active","metadata":{"chat_runtime":{"project_id":"project-1","contact_agent_id":"agent-1"},"contact":{"contact_id":"contact-1","agent_id":"agent-1"}}}]"#
        default:
            return HTTPResponse(statusCode: 404, headers: [:], body: Data())
        }
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func requestPaths() -> [String] { paths }
}
