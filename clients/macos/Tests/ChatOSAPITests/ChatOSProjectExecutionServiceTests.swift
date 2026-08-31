import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSProjectExecutionServiceTests: XCTestCase {
    func testFetchExecutionUsesPreciseIdentityAndDecodesFailureReason() async throws {
        let transport = ProjectExecutionTransport()
        let service = makeService(transport: transport)

        let fetched = try await service.fetchExecution(identity)
        let launch = try XCTUnwrap(fetched)

        let capturedRequest = await transport.lastRequest()
        let request = try XCTUnwrap(capturedRequest)
        let components = try XCTUnwrap(URLComponents(url: request.url, resolvingAgainstBaseURL: false))
        XCTAssertEqual(
            components.percentEncodedPath,
            "/api/chatos/projects/project%201/requirements/requirement%2F1/execution-plan"
        )
        XCTAssertEqual(
            Dictionary(uniqueKeysWithValues: (components.queryItems ?? []).map { ($0.name, $0.value) })["conversation_id"]!,
            "conversation-1"
        )
        XCTAssertEqual(launch.overallStatus, "failed")
        XCTAssertEqual(launch.failureReason, "规划 Agent 调用失败")
    }

    func testConfirmUsesExactGatewayPathAndBody() async throws {
        let transport = ProjectExecutionTransport()
        let service = makeService(transport: transport)

        _ = try await service.confirmExecution(identity)

        let capturedRequest = await transport.lastRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertEqual(
            URLComponents(url: request.url, resolvingAgainstBaseURL: false)?.percentEncodedPath,
            "/api/chatos/projects/project%201/requirements/requirement%2F1/confirm-execution"
        )
        let body = try XCTUnwrap(request.body)
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        XCTAssertEqual(json["execution_group_id"] as? String, "group-1")
        XCTAssertEqual(json["conversation_id"] as? String, "conversation-1")
        XCTAssertEqual(json["contact_id"] as? String, "contact-1")
        XCTAssertNil(json["discard_tasks"])
    }

    func testAbandonUsesStopAndExplicitlyDiscardsTasks() async throws {
        let transport = ProjectExecutionTransport()
        let service = makeService(transport: transport)

        _ = try await service.abandonPlan(identity)

        let capturedRequest = await transport.lastRequest()
        let request = try XCTUnwrap(capturedRequest)
        XCTAssertEqual(
            URLComponents(url: request.url, resolvingAgainstBaseURL: false)?.percentEncodedPath,
            "/api/chatos/projects/project%201/requirements/requirement%2F1/stop"
        )
        let body = try XCTUnwrap(request.body)
        let json = try XCTUnwrap(JSONSerialization.jsonObject(with: body) as? [String: Any])
        XCTAssertEqual(json["discard_tasks"] as? Bool, true)
        XCTAssertEqual(json["execution_group_id"] as? String, "group-1")
    }

    private var identity: ProjectExecutionIdentity {
        ProjectExecutionIdentity(
            projectID: "project 1",
            requirementID: "requirement/1",
            executionGroupID: "group-1",
            conversationID: "conversation-1",
            contactID: "contact-1"
        )
    }

    private func makeService(transport: ProjectExecutionTransport) -> ChatOSProjectExecutionService {
        let client = ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        )
        return ChatOSProjectExecutionService(client: client)
    }
}

private actor ProjectExecutionTransport: HTTPTransport {
    private var request: HTTPRequest?

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        self.request = request
        let body = request.url.path.hasSuffix("/execution-plan")
            ? #"{"found":true,"project_id":"project 1","requirement_id":"requirement/1","conversation_id":"conversation-1","execution_group_id":"group-1","message_id":"group-1","status":"failed","confirmation_status":"failed","has_started_runs":false,"failure_kind":"planner_failed","failure_reason":"规划 Agent 调用失败"}"#
            : #"{"success":true,"status":"accepted","execution_group_id":"group-1"}"#
        return HTTPResponse(statusCode: 200, headers: [:], body: Data(body.utf8))
    }

    func lastRequest() -> HTTPRequest? { request }
}
