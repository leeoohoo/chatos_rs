import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSRemoteConnectionServiceTests: XCTestCase {
    func testListsCreatesAndTestsRemoteConnectionWithVerificationHeader() async throws {
        let transport = RemoteConnectionTransport()
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api")!),
            accessToken: "token",
            transport: transport
        )
        let service = ChatOSRemoteConnectionService(client: client)

        let listed = try await service.listConnections()
        XCTAssertEqual(listed.first?.authenticationType, .privateKey)
        XCTAssertEqual(listed.first?.localConnectorWorkspaceID, "workspace-1")

        let created = try await service.createConnection(Self.draft)
        XCTAssertEqual(created.id, "remote/1")

        let result = try await service.testDraft(Self.draft, verificationCode: "123456")
        XCTAssertTrue(result.success)

        let requests = await transport.requests
        XCTAssertEqual(requests.map(\.path), [
            "/api/remote-connections",
            "/api/remote-connections",
            "/api/remote-connections/test",
        ])
        XCTAssertEqual(requests.last?.headers["x-remote-verification-code"], "123456")
    }

    func testMapsSecondFactorErrorToChallenge() async throws {
        let transport = RemoteConnectionTransport(challenges: true)
        let service = ChatOSRemoteConnectionService(client: ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api")!),
            accessToken: "token",
            transport: transport
        ))

        do {
            _ = try await service.testDraft(Self.draft, verificationCode: nil)
            XCTFail("Expected verification challenge")
        } catch let challenge as RemoteVerificationChallenge {
            XCTAssertEqual(challenge.prompt, "Verification code:")
        }
    }

    private static let draft = RemoteConnectionDraft(
        name: "Production",
        host: "server.example.com",
        port: 22,
        username: "deploy",
        authenticationType: .privateKey,
        password: nil,
        privateKeyPath: "/Users/me/.ssh/id_ed25519",
        certificatePath: nil,
        defaultRemotePath: "/srv/app",
        hostKeyPolicy: .strict,
        localConnectorDeviceID: "device-1",
        localConnectorWorkspaceID: "workspace-1",
        jumpEnabled: false,
        jumpConnectionID: nil,
        jumpHost: nil,
        jumpPort: nil,
        jumpUsername: nil,
        jumpPrivateKeyPath: nil,
        jumpCertificatePath: nil,
        jumpPassword: nil
    )
}

private actor RemoteConnectionTransport: HTTPTransport {
    struct Record: Sendable {
        var path: String
        var method: String
        var headers: [String: String]
    }

    private let challenges: Bool
    private(set) var requests: [Record] = []

    init(challenges: Bool = false) {
        self.challenges = challenges
    }

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        let path = request.url.path(percentEncoded: true)
        requests.append(.init(path: path, method: request.method, headers: request.headers))
        if path == "/api/remote-connections/test" {
            if challenges {
                return HTTPResponse(
                    statusCode: 400,
                    headers: [:],
                    body: Data(#"{"error":"需要二次验证","code":"second_factor_required","challenge_prompt":"Verification code:"}"#.utf8)
                )
            }
            return HTTPResponse(
                statusCode: 200,
                headers: [:],
                body: Data(#"{"success":true,"message":"connected"}"#.utf8)
            )
        }
        let response = #"{"id":"remote/1","name":"Production","host":"server.example.com","port":22,"username":"deploy","auth_type":"private_key","has_password":false,"has_private_key_path":true,"has_certificate_path":false,"default_remote_path":"/srv/app","host_key_policy":"strict","local_connector_device_id":"device-1","local_connector_workspace_id":"workspace-1","jump_enabled":false,"jump_connection_id":null,"jump_host":null,"jump_port":null,"jump_username":null,"has_jump_private_key_path":false,"has_jump_certificate_path":false,"has_jump_password":false,"last_active_at":"2026-08-25T04:00:00Z"}"#
        let body = request.method == "GET" ? "[\(response)]" : response
        return HTTPResponse(
            statusCode: request.method == "POST" ? 201 : 200,
            headers: [:],
            body: Data(body.utf8)
        )
    }
}
