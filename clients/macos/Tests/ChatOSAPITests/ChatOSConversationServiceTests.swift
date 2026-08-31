import Foundation
import XCTest
@testable import ChatOSAPI
@testable import ChatOSCore

final class ChatOSConversationServiceTests: XCTestCase {
    func testDefaultHistoryPageRequestsTenUserTurns() async throws {
        let transport = RecordingTransport(
            responseBody: Data(#"{"items":[],"has_more":false,"next_before":null}"#.utf8)
        )
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token-1",
            transport: transport
        )

        _ = try await ChatOSConversationService(client: client).fetchHistory(
            ConversationHistoryQuery(sessionID: "conversation-1", requestGeneration: 1)
        )

        let capturedRequest = await transport.lastRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertEqual(request.url.query, "limit=10")
    }

    func testHistoryRequestUsesCurrentCompactHistoryEndpoint() async throws {
        let transport = RecordingTransport(
            responseBody: Data(#"{"items":[],"has_more":false,"next_before":null}"#.utf8)
        )
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token-1",
            transport: transport
        )
        let service = ChatOSConversationService(client: client)

        _ = try await service.fetchHistory(
            ConversationHistoryQuery(
                sessionID: "conversation-1",
                limit: 25,
                before: "turn-9",
                requestGeneration: 3
            )
        )

        let capturedRequest = await transport.lastRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertEqual(request.url.path, "/api/chatos/conversations/conversation-1/compact-history")
        XCTAssertEqual(request.url.query, "limit=25&before=turn-9")
        XCTAssertEqual(request.headers["Authorization"], "Bearer token-1")
        XCTAssertEqual(request.headers["X-Chatos-Client-Surface"], "local-connector-desktop")
    }
}

private actor RecordingTransport: HTTPTransport {
    private let responseBody: Data
    private var capturedRequest: HTTPRequest?

    init(responseBody: Data) {
        self.responseBody = responseBody
    }

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        capturedRequest = request
        return HTTPResponse(statusCode: 200, headers: [:], body: responseBody)
    }

    func lastRequest() -> HTTPRequest? {
        capturedRequest
    }
}
