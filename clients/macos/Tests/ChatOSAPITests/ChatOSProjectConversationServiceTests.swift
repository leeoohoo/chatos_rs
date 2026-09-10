import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSProjectConversationServiceTests: XCTestCase {
    func testPreparesConversationWithoutProjectCRUDOrContactBinding() async throws {
        let transport = ProjectConversationTransport()
        let client = ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
                                     accessToken: "token", transport: transport)
        let project = try project()
        let contact = WorkspaceContact(id: "contact-1", agentID: "agent-1", name: "叽咕狸", status: "active")
        let id = try await ChatOSProjectConversationService(client: client).ensureConversation(project: project, contact: contact)
        XCTAssertEqual(id, "conversation-1")
        let requests = await transport.requests
        XCTAssertEqual(requests.map(\.method), ["GET", "POST"])
        XCTAssertTrue(requests.allSatisfy { $0.url.path == "/api/chatos/conversations" })
        let body = try XCTUnwrap(requests.last?.body)
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        XCTAssertEqual(json["project_id"] as? String, "local-id")
        let metadata = try XCTUnwrap(json["metadata"] as? [String: Any])
        let contactJSON = try XCTUnwrap(metadata["contact"] as? [String: Any])
        XCTAssertEqual(contactJSON["contact_id"] as? String, "contact-1")
        let runtime = try XCTUnwrap(metadata["chat_runtime"] as? [String: Any])
        let context = try XCTUnwrap(runtime["project_context"] as? [String: Any])
        XCTAssertEqual(context["projectId"] as? String, "local-id")
    }

    func testExistingConversationAvoidsCreatingAnotherOne() async throws {
        let transport = ProjectConversationTransport(existing: true)
        let client = ChatOSAPIClient(configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
                                     accessToken: "token", transport: transport)
        let id = try await ChatOSProjectConversationService(client: client).ensureConversation(
            project: try project(),
            contact: .init(id: "contact-1", agentID: "agent-1", name: "叽咕狸", status: "active"))
        XCTAssertEqual(id, "conversation-1")
        let requests = await transport.requests
        XCTAssertEqual(requests.count, 1)
    }

    private func project() throws -> WorkspaceProject {
        let record = LocalProjectRecord(
            id: "local-id", ownerUserID: "owner",
            draft: .init(name: "Local", workspaceID: "ws", relativeRoot: "repo"),
            createdAtUnixMs: 1, updatedAtUnixMs: 1
        )
        return WorkspaceProject(
            id: record.id, name: record.draft.name,
            rootPath: "local://connector/device/ws/repo", latestConversationID: nil,
            projectContext: try ProjectContextSnapshot(record: record, deviceID: "device")
        )
    }
}

private actor ProjectConversationTransport: HTTPTransport {
    var requests: [HTTPRequest] = []
    let existing: Bool
    init(existing: Bool = false) { self.existing = existing }

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(request)
        guard request.url.path == "/api/chatos/conversations" else {
            throw URLError(.unsupportedURL)
        }
        let item = #"{"id":"conversation-1","project_id":"local-id","message_count":1,"metadata":{"contact":{"contact_id":"contact-1"},"chat_runtime":{"project_context":{"schemaVersion":1,"projectId":"local-id","projectName":"Local","projectRevision":1,"executionTarget":{"deviceId":"device","workspaceId":"ws","relativeRoot":"repo"}}}}}"#
        let body = request.method == "GET" ? (existing ? "[" + item + "]" : "[]") : item
        return HTTPResponse(statusCode: request.method == "POST" ? 201 : 200, headers: [:], body: Data(body.utf8))
    }
}
