import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSWeChatCompanionServiceTests: XCTestCase {
    func testBindingFlowUsesUserServiceGatewayAndDecodesCodeImage() async throws {
        let transport = WeChatCompanionTransport()
        let service = ChatOSWeChatCompanionService(client: ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "desktop-token",
            transport: transport
        ))

        let binding = try await service.bindingStatus()
        let ticket = try await service.issueBindTicket()
        let status = try await service.bindTicketStatus(id: ticket.id)
        try await service.confirmBindTicket(id: ticket.id)
        let sessions = try await service.clientSessions()
        try await service.revokeClientSession(id: "session-1")
        try await service.unbind()

        XCTAssertFalse(binding.bound)
        XCTAssertEqual(ticket.id, "ticket-1")
        XCTAssertTrue(ticket.codeImageData.starts(with: Data([0x89, 0x50, 0x4e, 0x47])))
        XCTAssertEqual(status.status, "claimed")
        XCTAssertEqual(sessions.map(\.id), ["session-1"])

        let requests = await transport.requests
        XCTAssertEqual(requests.map(\.path), [
            "/api/user/auth/wechat/mini-program/binding",
            "/api/user/auth/wechat/mini-program/bind-tickets",
            "/api/user/auth/wechat/mini-program/bind-tickets/ticket-1",
            "/api/user/auth/wechat/mini-program/bind-tickets/ticket-1/confirm",
            "/api/user/auth/client-sessions",
            "/api/user/auth/client-sessions/session-1",
            "/api/user/auth/wechat/mini-program/binding",
        ])
        XCTAssertTrue(requests.allSatisfy { $0.authorization == "Bearer desktop-token" })
    }
}

private actor WeChatCompanionTransport: HTTPTransport {
    struct CapturedRequest: Sendable {
        var path: String
        var method: String
        var authorization: String?
    }

    private(set) var requests: [CapturedRequest] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(CapturedRequest(
            path: request.url.path(percentEncoded: true),
            method: request.method,
            authorization: request.headers["Authorization"]
        ))
        let path = request.url.path
        let body: Data
        let status: Int
        switch (request.method, path) {
        case ("GET", "/api/user/auth/wechat/mini-program/binding"):
            status = 200
            body = Data(#"{"bound":false,"created_at":null,"last_login_at":null}"#.utf8)
        case ("POST", "/api/user/auth/wechat/mini-program/bind-tickets"):
            status = 200
            body = Data(#"{"ticket_id":"ticket-1","scene":"scene-1","expires_at_unix":9999999999,"qr_code_data_url":"data:image/png;base64,iVBORw0KGgo="}"#.utf8)
        case ("GET", "/api/user/auth/wechat/mini-program/bind-tickets/ticket-1"):
            status = 200
            body = Data(#"{"ticket_id":"ticket-1","status":"claimed","expires_at_unix":9999999999,"claimed_at":"2026-09-14T00:00:00Z"}"#.utf8)
        case ("POST", "/api/user/auth/wechat/mini-program/bind-tickets/ticket-1/confirm"):
            status = 200
            body = Data(#"{"status":"confirmed","external_identity_id":"identity-1"}"#.utf8)
        case ("GET", "/api/user/auth/client-sessions"):
            status = 200
            body = Data(#"[{"id":"session-1","client_type":"wechat_mini_program","created_at":"2026-09-14T00:00:00Z","last_seen_at":"2026-09-14T00:00:00Z","expires_at_unix":9999999999,"revoked_at":null}]"#.utf8)
        case ("DELETE", _):
            status = 204
            body = Data()
        default:
            status = 404
            body = Data(#"{"error":"not found"}"#.utf8)
        }
        return HTTPResponse(statusCode: status, headers: [:], body: body)
    }
}
