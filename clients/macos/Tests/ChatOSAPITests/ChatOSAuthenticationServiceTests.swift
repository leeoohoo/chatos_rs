import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSAuthenticationServiceTests: XCTestCase {
    func testLoginPersistsTokenAndMapsUser() async throws {
        let transport = QueueTransport(responses: [
            HTTPResponse(
                statusCode: 200,
                headers: [:],
                body: Data(#"{"access_token":"token-new","user":{"id":"user-1","username":"person@example.com","display_name":"Person","role":"user"}}"#.utf8)
            ),
        ])
        let store = MemoryCredentialStore()
        let service = makeService(transport: transport, store: store)

        let session = try await service.login(
            username: " person@example.com ",
            password: "secret-value"
        )

        XCTAssertEqual(session.user.id, "user-1")
        XCTAssertEqual(session.user.displayName, "Person")
        let storedToken = try await store.loadAccessToken()
        XCTAssertEqual(storedToken, "token-new")

        let requests = await transport.requests()
        let request = try XCTUnwrap(requests.first)
        XCTAssertEqual(request.url.path, "/api/chatos/auth/login")
        let body = try XCTUnwrap(request.body)
        let payload = try XCTUnwrap(
            JSONSerialization.jsonObject(with: body) as? [String: String]
        )
        XCTAssertEqual(payload["username"], "person@example.com")
        XCTAssertEqual(payload["password"], "secret-value")
    }

    func testRestoreValidatesStoredTokenWithMe() async throws {
        let transport = QueueTransport(responses: [
            HTTPResponse(
                statusCode: 200,
                headers: ["x-access-token": "token-refreshed"],
                body: Data(#"{"user":{"id":"user-2","username":"restored@example.com","display_name":null,"role":"admin"}}"#.utf8)
            ),
        ])
        let store = MemoryCredentialStore(token: "token-old")
        let service = makeService(transport: transport, store: store)

        let session = try await service.restoreSession()

        XCTAssertEqual(session?.user.username, "restored@example.com")
        let storedToken = try await store.loadAccessToken()
        XCTAssertEqual(storedToken, "token-refreshed")
        let requests = await transport.requests()
        let request = try XCTUnwrap(requests.first)
        XCTAssertEqual(request.headers["Authorization"], "Bearer token-old")
    }

    func testUnauthorizedRestoreClearsCredential() async throws {
        let transport = QueueTransport(responses: [
            HTTPResponse(statusCode: 401, headers: [:], body: Data()),
        ])
        let store = MemoryCredentialStore(token: "expired-token")
        let service = makeService(transport: transport, store: store)

        let session = try await service.restoreSession()

        XCTAssertNil(session)
        let storedToken = try await store.loadAccessToken()
        XCTAssertNil(storedToken)
    }

    func testLogoutClearsCredential() async throws {
        let store = MemoryCredentialStore(token: "token")
        let service = makeService(transport: QueueTransport(responses: []), store: store)

        await service.logout()

        let storedToken = try await store.loadAccessToken()
        XCTAssertNil(storedToken)
    }

    private func makeService(
        transport: QueueTransport,
        store: MemoryCredentialStore
    ) -> ChatOSAuthenticationService {
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            credentialStore: store,
            transport: transport
        )
        return ChatOSAuthenticationService(client: client, credentialStore: store)
    }
}

private actor MemoryCredentialStore: CredentialStoring {
    private var token: String?

    init(token: String? = nil) {
        self.token = token
    }

    func loadAccessToken() async throws -> String? { token }
    func saveAccessToken(_ token: String) async throws { self.token = token }
    func deleteAccessToken() async throws { token = nil }
}

private actor QueueTransport: HTTPTransport {
    private var queuedResponses: [HTTPResponse]
    private var capturedRequests: [HTTPRequest] = []

    init(responses: [HTTPResponse]) {
        self.queuedResponses = responses
    }

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        capturedRequests.append(request)
        guard !queuedResponses.isEmpty else { throw ChatOSAPIError.invalidResponse }
        return queuedResponses.removeFirst()
    }

    func requests() -> [HTTPRequest] { capturedRequests }
}
