import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSAPIClientErrorTests: XCTestCase {
    func testHTMLGatewayFailureUsesFriendlyMessage() async throws {
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            transport: APIErrorTransport(
                response: HTTPResponse(
                    statusCode: 503,
                    headers: ["content-type": "text/html"],
                    body: Data("<html><body>Service Unavailable</body></html>".utf8)
                )
            )
        )

        do {
            let _: ErrorResponseDTO = try await client.request("/history")
            XCTFail("Expected request to fail")
        } catch let error as ChatOSAPIError {
            XCTAssertEqual(
                error,
                .server(statusCode: 503, message: "服务正在启动或暂时不可用，请稍后重试。")
            )
        }
    }

    func testNestedJSONErrorMessageIsPreserved() async throws {
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            transport: APIErrorTransport(
                response: HTTPResponse(
                    statusCode: 404,
                    headers: ["content-type": "application/json"],
                    body: Data(#"{"error":{"code":"not_found","message":"请求的资源不存在"}}"#.utf8)
                )
            )
        )

        do {
            let _: ErrorResponseDTO = try await client.request("/missing")
            XCTFail("Expected request to fail")
        } catch let error as ChatOSAPIError {
            XCTAssertEqual(
                error,
                .serverDetail(
                    statusCode: 404,
                    message: "请求的资源不存在",
                    code: "not_found",
                    challengePrompt: nil
                )
            )
        }
    }

    func testAuthenticatedUnauthorizedRequestClearsCredentialAndPublishesExpiration() async throws {
        let store = APIErrorCredentialStore(token: "expired-token")
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "expired-token",
            credentialStore: store,
            transport: APIErrorTransport(
                response: HTTPResponse(
                    statusCode: 401,
                    headers: ["content-type": "application/json"],
                    body: Data(#"{"error":"invalid or expired token"}"#.utf8)
                )
            )
        )
        let expiration = expectation(description: "authentication expiration is published")
        let observer = NotificationCenter.default.addObserver(
            forName: .chatOSAuthenticationDidExpire,
            object: nil,
            queue: nil
        ) { _ in
            expiration.fulfill()
        }
        defer { NotificationCenter.default.removeObserver(observer) }

        do {
            let _: ErrorResponseDTO = try await client.request("/auth/me")
            XCTFail("Expected request to fail")
        } catch let error as ChatOSAPIError {
            XCTAssertEqual(error, .unauthorized)
        }

        await fulfillment(of: [expiration], timeout: 1)
        let currentToken = await client.currentAccessToken()
        let credentialWasDeleted = await store.wasDeleted()
        XCTAssertNil(currentToken)
        XCTAssertTrue(credentialWasDeleted)
    }
}

private struct ErrorResponseDTO: Decodable, Sendable {}

private struct APIErrorTransport: HTTPTransport {
    var response: HTTPResponse

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        response
    }
}

private actor APIErrorCredentialStore: CredentialStoring {
    private var token: String?
    private var deleted = false

    init(token: String?) {
        self.token = token
    }

    func loadAccessToken() async throws -> String? { token }

    func saveAccessToken(_ token: String) async throws {
        self.token = token
        deleted = false
    }

    func deleteAccessToken() async throws {
        token = nil
        deleted = true
    }

    func wasDeleted() -> Bool { deleted }
}
