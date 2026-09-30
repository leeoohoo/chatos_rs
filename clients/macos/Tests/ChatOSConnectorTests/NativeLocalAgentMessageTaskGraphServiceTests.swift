@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentMessageTaskGraphServiceTests: XCTestCase {
    func testMapsTurnTaskGraphAndCancelsWithCurrentVersion() async throws {
        let host = LocalTaskHostStub()
        let service = NativeLocalAgentMessageTaskGraphService(host: host)
        await service.configure(ownerUserID: "user-1")
        let lookup = MessageTaskLookup(
            sessionID: "conversation-1",
            turnID: "turn-1",
            sourceUserMessageID: "message-1"
        )

        let graph = try await service.fetchGraph(messageID: "message-1", lookup: lookup)
        XCTAssertEqual(graph.rootTaskIDs, ["task-1"])
        XCTAssertEqual(graph.nodes.map(\.task.id), ["task-1", "task-2"])
        XCTAssertEqual(graph.nodes.map(\.depth), [0, 1])
        XCTAssertEqual(graph.edges.first?.sourceID, "task-1")
        XCTAssertEqual(graph.edges.first?.targetID, "task-2")

        let task = try await service.fetchTask(
            messageID: "message-1",
            taskID: "task-2",
            lookup: lookup
        )
        XCTAssertEqual(task.objective, "Ship locally")
        XCTAssertEqual(task.prerequisiteTaskIDs, ["task-1"])

        try await service.cancelTask(
            messageID: "message-1",
            taskID: "task-2",
            lookup: lookup,
            reason: "stop"
        )
        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("cancel_task"))
        XCTAssertEqual(command["task_id"], .string("task-2"))
        XCTAssertEqual(command["expected_version"], .number(2))
    }

    func testRetryPersistsAdditionalInstructionInLocalCommand() async throws {
        let host = LocalTaskHostStub()
        let service = NativeLocalAgentMessageTaskGraphService(host: host)
        await service.configure(ownerUserID: "user-1")

        _ = try await service.retryRun(
            messageID: "message-1",
            runID: "run-task-2",
            lookup: .init(turnID: "turn-1"),
            instruction: "Use the local fallback"
        )

        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("retry_task"))
        XCTAssertEqual(command["retry_instruction"], .string("Use the local fallback"))
    }
}

private actor LocalTaskHostStub: LocalAgentHostClientServicing {
    private var commands: [Data] = []

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "list_task_graphs":
            return try json([
                "type": "task_graphs",
                "page": [
                    "graphs": [[
                        "graph_id": "graph-1",
                        "source_entity_type": "conversation_turn",
                        "source_entity_id": "turn-1",
                        "updated_at_unix_ms": 3,
                    ]],
                    "next_before_updated_at_unix_ms": NSNull(),
                    "next_before_graph_id": NSNull(),
                ],
            ])
        case "get_task_graph", "cancel_task":
            return try json(["type": "task_graph", "graph": graph()])
        case "retry_task":
            return try json(["type": "task_graph", "graph": graph()])
        case "get_run":
            return try json([
                "type": "run",
                "run": [
                    "run_id": "run-task-2",
                    "owner_user_id": "user-1",
                    "owner_entity_type": "task",
                    "owner_entity_id": "task-2",
                    "profile_key": "task_runner",
                    "input": [:],
                    "status": "failed",
                    "version": 3,
                    "created_at_unix_ms": 1,
                    "updated_at_unix_ms": 2,
                ],
            ])
        case "get_task_runs":
            return try json([
                "type": "task_runs",
                "task_id": object?["task_id"] ?? "task-1",
                "runs": [],
            ])
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    func lastCommand() throws -> [String: LocalAgentJSONValue] {
        guard let data = commands.last else { throw CocoaError(.fileNoSuchFile) }
        let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
        guard case let .object(object) = value else { throw CocoaError(.fileReadCorruptFile) }
        return object
    }

    private func graph() -> [String: Any] {
        [
            "graph_id": "graph-1",
            "owner_user_id": "user-1",
            "source_entity_type": "conversation_turn",
            "source_entity_id": "turn-1",
            "status": "running",
            "tasks": [task("task-1", version: 1), task("task-2", version: 2)],
            "dependencies": [[
                "task_id": "task-2",
                "prerequisite_task_id": "task-1",
            ]],
            "created_at_unix_ms": 1,
        ]
    }

    private func task(_ id: String, version: Int) -> [String: Any] {
        [
            "graph_id": "graph-1",
            "owner_user_id": "user-1",
            "source_entity_type": "conversation_turn",
            "source_entity_id": "turn-1",
            "task_id": id,
            "title": id,
            "model_config_ref": "model-1",
            "input": ["objective": "Ship locally"],
            "status": id == "task-1" ? "succeeded" : "running",
            "active_run_id": NSNull(),
            "version": version,
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func json(_ value: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: value)
    }
}
