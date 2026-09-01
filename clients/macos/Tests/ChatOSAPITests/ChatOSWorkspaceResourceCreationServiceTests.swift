import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSWorkspaceResourceCreationServiceTests: XCTestCase {
    func testCreatesLocalProjectBindsDefaultContactAndPreparesConversation() async throws {
        let transport = ResourceCreationTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )
        let service = ChatOSWorkspaceResourceCreationService(client: client)

        let project = try await service.createLocalProject(.init(
            name: "Swift App",
            deviceID: "device-1",
            workspaceID: "workspace-1",
            relativePath: "projects/swift-app"
        ))
        try await service.bindContact(projectID: project.id, contactID: "contact-default")
        let conversationID = try await service.ensureConversation(
            project: project,
            contact: WorkspaceContact(
                id: "contact-default",
                agentID: "agent-default",
                name: "叽咕狸",
                status: "active"
            )
        )

        XCTAssertEqual(project.id, "project/1")
        XCTAssertEqual(project.rootPath, "local://connector/device-1/workspace-1/projects/swift-app")
        XCTAssertEqual(project.displayRootPath, "projects/swift-app")
        XCTAssertEqual(conversationID, "conversation-1")

        let requests = await transport.recordedRequests()
        XCTAssertEqual(requests.map(\.path), [
            "/api/chatos/local-connectors/projects",
            "/api/chatos/projects/project%2F1/contacts",
            "/api/chatos/projects/project%2F1/contacts",
            "/api/chatos/conversations",
            "/api/chatos/conversations",
        ])
        XCTAssertEqual(requests.map(\.method), ["POST", "POST", "GET", "GET", "POST"])
        XCTAssertEqual(requests.first?.timeoutInterval, 2 * 60 * 60)
        XCTAssertNil(requests[1].timeoutInterval)

        let projectBody = try XCTUnwrap(requests.first?.body)
        let projectJSON = try XCTUnwrap(
            JSONSerialization.jsonObject(with: projectBody) as? [String: Any]
        )
        XCTAssertEqual(projectJSON["name"] as? String, "Swift App")
        XCTAssertEqual(projectJSON["device_id"] as? String, "device-1")
        XCTAssertEqual(projectJSON["workspace_id"] as? String, "workspace-1")
        XCTAssertEqual(projectJSON["relative_path"] as? String, "projects/swift-app")

        let contactBody = try XCTUnwrap(requests[1].body)
        let contactJSON = try XCTUnwrap(
            JSONSerialization.jsonObject(with: contactBody) as? [String: Any]
        )
        XCTAssertEqual(contactJSON["contact_id"] as? String, "contact-default")

        let conversationBody = try XCTUnwrap(requests.last?.body)
        let conversationJSON = try XCTUnwrap(
            JSONSerialization.jsonObject(with: conversationBody) as? [String: Any]
        )
        XCTAssertEqual(conversationJSON["title"] as? String, "叽咕狸")
        XCTAssertEqual(conversationJSON["project_id"] as? String, "project/1")
        let metadata = try XCTUnwrap(conversationJSON["metadata"] as? [String: Any])
        let contact = try XCTUnwrap(metadata["contact"] as? [String: Any])
        let runtime = try XCTUnwrap(metadata["chat_runtime"] as? [String: Any])
        XCTAssertEqual(contact["contact_id"] as? String, "contact-default")
        XCTAssertEqual(contact["agent_id"] as? String, "agent-default")
        XCTAssertEqual(runtime["project_id"] as? String, "project/1")
        XCTAssertEqual(
            runtime["project_root"] as? String,
            "local://connector/device-1/workspace-1/projects/swift-app"
        )
    }
}

private actor ResourceCreationTransport: HTTPTransport {
    struct RecordedRequest: Sendable {
        var path: String
        var method: String
        var body: Data?
        var timeoutInterval: TimeInterval?
    }

    private var requests: [RecordedRequest] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(.init(
            path: request.url.path(percentEncoded: true),
            method: request.method,
            body: request.body,
            timeoutInterval: request.timeoutInterval
        ))
        switch (request.method, request.url.path(percentEncoded: true)) {
        case ("POST", "/api/chatos/local-connectors/projects"):
            return HTTPResponse(
                statusCode: 201,
                headers: [:],
                body: Data(#"{"id":"project/1","name":"Swift App","root_path":"local://connector/device-1/workspace-1/projects/swift-app","display_root_path":"projects/swift-app","latest_session_id":null}"#.utf8)
            )
        case ("POST", "/api/chatos/projects/project%2F1/contacts"):
            return HTTPResponse(
                statusCode: 200,
                headers: [:],
                body: Data(#"{"contact_id":"contact-default"}"#.utf8)
            )
        case ("GET", "/api/chatos/projects/project%2F1/contacts"):
            return HTTPResponse(
                statusCode: 200,
                headers: [:],
                body: Data(#"[{"contact_id":"contact-default","latest_session_id":null}]"#.utf8)
            )
        case ("GET", "/api/chatos/conversations"):
            return HTTPResponse(statusCode: 200, headers: [:], body: Data("[]".utf8))
        case ("POST", "/api/chatos/conversations"):
            return HTTPResponse(
                statusCode: 201,
                headers: [:],
                body: Data(#"{"id":"conversation-1","project_id":"project/1","message_count":0}"#.utf8)
            )
        default:
            return HTTPResponse(statusCode: 404, headers: [:], body: Data())
        }
    }

    func recordedRequests() -> [RecordedRequest] { requests }
}
