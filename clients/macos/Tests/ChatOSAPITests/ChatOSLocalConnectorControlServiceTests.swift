import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSLocalConnectorControlServiceTests: XCTestCase {
    func testStatusMapsRealLocalCoreFields() async throws {
        let transport = LocalConnectorTransport()
        let service = makeService(transport: transport)

        let status = try await service.fetchStatus()

        XCTAssertTrue(status.configured)
        XCTAssertTrue(status.connectorRunning)
        XCTAssertEqual(status.user?.username, "person@example.com")
        XCTAssertEqual(status.workspaces.first?.absoluteRoot, "/workspace/project")
        let requests = await transport.requests()
        let request = try XCTUnwrap(requests.first)
        XCTAssertEqual(request.url.path, "/api/local/status")
        XCTAssertEqual(request.headers["Authorization"], "Bearer desktop-secret")
    }

    func testPairingUsesGatewayTicketAndLocalDesktopTicketEndpoint() async throws {
        let transport = LocalConnectorTransport()
        let service = makeService(transport: transport)

        _ = try await service.pairWithCurrentChatOSSession(deviceName: "Test Mac")

        let requests = await transport.requests()
        XCTAssertEqual(requests.map(\.url.path), [
            "/api/chatos/auth/local-connector-ticket",
            "/api/local/auth/desktop-ticket",
        ])
        let payload = try XCTUnwrap(
            JSONSerialization.jsonObject(with: try XCTUnwrap(requests.last?.body)) as? [String: String]
        )
        XCTAssertEqual(payload["ticket"], "one-time-ticket")
        XCTAssertEqual(payload["cloud_base_url"], "http://127.0.0.1:39230")
        XCTAssertEqual(payload["device_name"], "Test Mac")
    }

    func testTerminalRunsThroughZshInsideSelectedWorkspace() async throws {
        let transport = LocalConnectorTransport()
        let service = makeService(transport: transport)

        let result = try await service.executeTerminal(
            workspaceID: "workspace-1",
            commandLine: "pwd && git status --short",
            cwd: "/workspace/project"
        )

        XCTAssertTrue(result.success)
        XCTAssertEqual(result.stdout, "/workspace/project\n")
        let requests = await transport.requests()
        let request = try XCTUnwrap(requests.last)
        let payload = try XCTUnwrap(
            JSONSerialization.jsonObject(with: try XCTUnwrap(request.body)) as? [String: Any]
        )
        XCTAssertEqual(payload["workspace_id"] as? String, "workspace-1")
        XCTAssertEqual(payload["command"] as? String, "/bin/zsh")
        XCTAssertEqual(payload["args"] as? [String], ["-lc", "pwd && git status --short"])
    }

    private func makeService(
        transport: LocalConnectorTransport
    ) -> ChatOSLocalConnectorControlService {
        let chatOSClient = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "gateway-token",
            transport: transport
        )
        return ChatOSLocalConnectorControlService(
            chatOSClient: chatOSClient,
            localBaseURL: URL(string: "http://127.0.0.1:39234")!,
            localDesktopToken: "desktop-secret",
            connectorCloudBaseURL: URL(string: "http://127.0.0.1:39230")!,
            transport: transport
        )
    }
}

private actor LocalConnectorTransport: HTTPTransport {
    private var captured: [HTTPRequest] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        captured.append(request)
        let body: String
        switch request.url.path {
        case "/api/chatos/auth/local-connector-ticket":
            body = #"{"ticket":"one-time-ticket","expires_in_seconds":60}"#
        case "/api/local/status", "/api/local/auth/desktop-ticket":
            body = Self.statusBody
        case "/api/local/terminal/exec":
            body = #"{"command":"/bin/zsh","args":["-lc","pwd && git status --short"],"cwd":"/workspace/project","success":true,"exit_code":0,"timed_out":false,"stdout":"/workspace/project\n","stderr":"","error":null}"#
        default:
            return HTTPResponse(statusCode: 404, headers: [:], body: Data())
        }
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func requests() -> [HTTPRequest] { captured }

    private static let statusBody = #"{"configured":true,"connector_running":true,"developer_mode":true,"cloud_base_url":"http://127.0.0.1:39230","user_service_base_url":"http://127.0.0.1:39190","device_id":"device-1","device_name":"Test Mac","user":{"id":"user-1","username":"person@example.com","display_name":"Person","role":"user"},"default_workspace_id":"workspace-1","workspaces":[{"id":"workspace-1","alias":"Project","absolute_root":"/workspace/project","fingerprint":"fp-1","project_config_trusted":true,"project_config_trust_stale":false}]}"#
}
