import ChatOSCore
import Foundation
import XCTest
@testable import ChatOSAPI

final class ChatOSPetActivityInboxServiceTests: XCTestCase {
    func testFetchMapsPersistentCompletedActivityAndRunVersion() async throws {
        let transport = PetActivityInboxTransport()
        let service = ChatOSPetActivityInboxService(client: ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        ))

        let activities = try await service.fetchOpenActivities(limit: 20)

        let activity = try XCTUnwrap(activities.first)
        XCTAssertEqual(activity.inboxID, "pet_1")
        XCTAssertEqual(activity.activityVersion, "run-2")
        XCTAssertEqual(activity.inboxStatus, .displayed)
        XCTAssertEqual(activity.source, .taskRunner)
        XCTAssertEqual(activity.kind, .succeeded)
        XCTAssertEqual(activity.title, "任务「整理发布说明」已完成")
        XCTAssertEqual(activity.route.taskID, "task-1")
        XCTAssertEqual(activity.route.runID, "run-2")
        XCTAssertNil(activity.expiresAt)

        let requests = await transport.requests
        let request = try XCTUnwrap(requests.first)
        XCTAssertEqual(request.path, "/api/chatos/pet-activities")
        XCTAssertEqual(
            request.query,
            "include_closed=false&mark_displayed=true&limit=20"
        )
    }

    func testIgnoreWritesDispositionToInboxRecord() async throws {
        let transport = PetActivityInboxTransport()
        let service = ChatOSPetActivityInboxService(client: ChatOSAPIClient(
            configuration: .init(baseURL: URL(string: "https://example.com/api/chatos")!),
            accessToken: "token",
            transport: transport
        ))
        let activities = try await service.fetchOpenActivities()
        let activity = try XCTUnwrap(activities.first)

        try await service.apply(.ignored, to: activity)

        let requests = await transport.requests
        let request = try XCTUnwrap(requests.last)
        XCTAssertEqual(request.method, "POST")
        XCTAssertEqual(request.path, "/api/chatos/pet-activities/pet_1/ignore")
    }
}

private actor PetActivityInboxTransport: HTTPTransport {
    struct Request: Sendable {
        var path: String
        var query: String?
        var method: String
    }

    private(set) var requests: [Request] = []

    func send(_ request: HTTPRequest) async throws -> HTTPResponse {
        requests.append(Request(
            path: request.url.path(percentEncoded: true),
            query: request.url.query(percentEncoded: true),
            method: request.method
        ))
        if request.method == "POST" {
            return HTTPResponse(
                statusCode: 200,
                headers: [:],
                body: Data(#"{"success":true}"#.utf8)
            )
        }
        return HTTPResponse(
            statusCode: 200,
            headers: [:],
            body: Data(Self.listResponse.utf8)
        )
    }

    private static let listResponse = #"""
    {
      "success": true,
      "activities": [{
        "id": "pet_1",
        "user_id": "user-1",
        "activity_key": "task-runner:task-1",
        "activity_version": "run-2",
        "source": "task_runner",
        "kind": "succeeded",
        "title": "任务「整理发布说明」已完成",
        "detail": "生成了发布说明",
        "route": {
          "project_id": "project-1",
          "conversation_id": "conversation-1",
          "turn_id": "turn-1",
          "message_id": "message-1",
          "task_id": "task-1",
          "run_id": "run-2"
        },
        "business_status": "completed",
        "inbox_status": "displayed",
        "requires_action": false,
        "occurred_at": "2026-08-28T08:00:00Z",
        "displayed_at": "2026-08-28T08:00:01Z",
        "created_at": "2026-08-28T08:00:00Z",
        "updated_at": "2026-08-28T08:00:01Z"
      }]
    }
    """#
}
