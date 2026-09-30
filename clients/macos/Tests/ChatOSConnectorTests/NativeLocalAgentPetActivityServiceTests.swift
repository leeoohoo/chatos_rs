@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentPetActivityServiceTests: XCTestCase {
    func testMapsLocalRunsToPetActivities() async throws {
        let host = PetActivityHostStub()
        let service = NativeLocalAgentPetActivityService(host: host)
        await service.configure(ownerUserID: "user-1")

        let activities = try await service.fetchOpenActivities()

        let chat = try XCTUnwrap(activities.first(where: { $0.source == .chat }))
        XCTAssertEqual(chat.kind, .waitingForUser)
        XCTAssertEqual(chat.route.conversationID, "conversation-1")
        XCTAssertEqual(chat.route.turnID, "turn-1")
        XCTAssertEqual(chat.route.runID, "run-chat")
        XCTAssertEqual(chat.route.promptID, "local-ask:run-chat")

        let task = try XCTUnwrap(activities.first(where: { $0.source == .taskRunner }))
        XCTAssertEqual(task.kind, .failed)
        XCTAssertEqual(task.route.taskID, "task-1")
        XCTAssertEqual(task.detail, "build failed")
    }

    func testLocalEventStreamUsesHostWaitEvents() async throws {
        let host = PetActivityHostStub()
        let service = NativeLocalAgentPetActivityService(host: host)
        await service.configure(ownerUserID: "user-1")
        let stream = await service.petActivityEvents()
        var iterator = stream.makeAsyncIterator()

        let initial = try await iterator.next()
        let changed = try await iterator.next()
        XCTAssertEqual(initial, .reconcile)
        XCTAssertEqual(changed, .reconcile)
        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("wait_events"))
        XCTAssertEqual(command["owner_user_id"], .string("user-1"))
    }
}

private actor PetActivityHostStub: LocalAgentHostClientServicing {
    private var commands: [Data] = []

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        let type = object?["type"] as? String
        if type == "wait_events" {
            return try JSONSerialization.data(withJSONObject: [
                "type": "events",
                "events": [[
                    "cursor": 1,
                    "event_id": "event-1",
                    "run_id": "run-chat",
                    "event_type": "run_updated",
                    "created_at_unix_ms": Self.now,
                ]],
                "next_cursor": 1,
            ])
        }

        let scope = object?["scope"] as? String
        let runs: [[String: Any]] = scope == "active"
            ? [[
                "run_id": "run-chat",
                "owner_user_id": "user-1",
                "owner_entity_type": "conversation_turn",
                "owner_entity_id": "turn-1",
                "profile_key": "main_chat",
                "input": [
                    "conversation_id": "conversation-1",
                    "turn_id": "turn-1",
                ],
                "status": "waiting_user",
                "version": 3,
                "created_at_unix_ms": Self.now - 1_000,
                "updated_at_unix_ms": Self.now,
            ]]
            : [[
                "run_id": "run-task",
                "owner_user_id": "user-1",
                "owner_entity_type": "task",
                "owner_entity_id": "task-1",
                "profile_key": "task_execution",
                "input": [:],
                "status": "failed",
                "version": 4,
                "terminal_outcome": ["error": "build failed"],
                "created_at_unix_ms": Self.now - 2_000,
                "updated_at_unix_ms": Self.now,
            ]]
        return try JSONSerialization.data(withJSONObject: [
            "type": "runs",
            "page": [
                "runs": runs,
                "next_before_updated_at_unix_ms": NSNull(),
                "next_before_run_id": NSNull(),
            ],
        ])
    }

    func lastCommand() throws -> [String: LocalAgentJSONValue] {
        guard let data = commands.last else { throw CocoaError(.fileNoSuchFile) }
        let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
        guard case let .object(object) = value else { throw CocoaError(.fileReadCorruptFile) }
        return object
    }

    private static let now = Int64(Date().timeIntervalSince1970 * 1_000)
}
