import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSAPIClientErrorTests: XCTestCase {
    func testHTMLGatewayFailureUsesFriendlyMessage() async throws {
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com")!),
            transport: APIErrorTransport(
                response: HTTPResponse(
                    statusCode: 503,
                    headers: ["content-type": "text/html"],
                    body: Data("<html><body>Service Unavailable</body></html>".utf8)
                )
            )
        )

        do {
            let _: ErrorResponseDTO = try await client.request(
                "/history",
                service: .userService
            )
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
            configuration: .init(baseURL: URL(string: "https://example.com")!),
            transport: APIErrorTransport(
                response: HTTPResponse(
                    statusCode: 404,
                    headers: ["content-type": "application/json"],
                    body: Data(#"{"error":{"code":"not_found","message":"请求的资源不存在"}}"#.utf8)
                )
            )
        )

        do {
            let _: ErrorResponseDTO = try await client.request(
                "/missing",
                service: .userService
            )
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

    func testRegistrationProxySurfacesActionableUpstreamDetail() async throws {
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com")!),
            transport: APIErrorTransport(
                response: HTTPResponse(
                    statusCode: 400,
                    headers: ["content-type": "application/json"],
                    body: Data(#"{"error":"register via user_service failed","detail":"user_service request failed: 400 Bad Request: {\"error\":\"invite code is invalid\"}"}"#.utf8)
                )
            )
        )

        do {
            let _: ErrorResponseDTO = try await client.request(
                "/auth/register",
                method: "POST",
                service: .userService
            )
            XCTFail("Expected request to fail")
        } catch let error as ChatOSAPIError {
            XCTAssertEqual(
                error,
                .server(
                    statusCode: 400,
                    message: #"user_service request failed: 400 Bad Request: {"error":"invite code is invalid"}"#
                )
            )
        }
    }

    func testAuthenticatedUnauthorizedRequestClearsCredentialAndPublishesExpiration() async throws {
        let store = APIErrorCredentialStore(token: "expired-token")
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com")!),
            accessToken: "expired-token",
            credentialStore: store,
            transport: APIErrorTransport(
                response: HTTPResponse(
                    statusCode: 401,
                    headers: [
                        "content-type": "application/json",
                        "x-access-token": "invalid-user-refresh-token",
                    ],
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
            let _: ErrorResponseDTO = try await client.request(
                "/auth/me",
                service: .userService
            )
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

    func testMemoryUnauthorizedRequestPreservesLoginCredentialAndDoesNotPublishExpiration() async throws {
        let store = APIErrorCredentialStore(token: "valid-user-token")
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com")!),
            accessToken: "valid-user-token",
            credentialStore: store,
            transport: APIErrorTransport(
                response: HTTPResponse(
                    statusCode: 401,
                    headers: [
                        "content-type": "application/json",
                        "x-access-token": "invalid-memory-refresh-token",
                    ],
                    body: Data(#"{"error":"memory scope is unavailable"}"#.utf8)
                )
            )
        )
        let expiration = expectation(description: "authentication expiration is not published")
        expiration.isInverted = true
        let observer = NotificationCenter.default.addObserver(
            forName: .chatOSAuthenticationDidExpire,
            object: nil,
            queue: nil
        ) { _ in
            expiration.fulfill()
        }
        defer { NotificationCenter.default.removeObserver(observer) }

        do {
            let _: ErrorResponseDTO = try await client.request(
                "/context/compose",
                method: "POST",
                service: .memoryEngine
            )
            XCTFail("Expected request to fail")
        } catch let error as ChatOSAPIError {
            XCTAssertEqual(error, .unauthorized)
        }

        await fulfillment(of: [expiration], timeout: 0.1)
        let currentToken = await client.currentAccessToken()
        let storedToken = await store.storedToken()
        let credentialWasDeleted = await store.wasDeleted()
        XCTAssertEqual(currentToken, "valid-user-token")
        XCTAssertEqual(storedToken, "valid-user-token")
        XCTAssertFalse(credentialWasDeleted)
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

    func storedToken() -> String? { token }
}
