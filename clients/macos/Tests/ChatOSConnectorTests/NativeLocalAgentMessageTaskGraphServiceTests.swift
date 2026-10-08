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
        XCTAssertEqual(graph.rootTaskIDs, ["task-1", "task-2", "task-3"])
        XCTAssertEqual(graph.nodes.map(\.task.id), ["task-1", "task-2", "task-3"])
        XCTAssertEqual(graph.nodes.map(\.depth), [0, 0, 0])
        XCTAssertEqual(graph.edges.first?.sourceID, "task-1")
        XCTAssertEqual(graph.edges.first?.targetID, "task-2")
        XCTAssertEqual(graph.edges.first?.kind, "prerequisite")
        XCTAssertEqual(graph.edges.last?.sourceID, "task-1")
        XCTAssertEqual(graph.edges.last?.targetID, "task-3")
        XCTAssertEqual(graph.edges.last?.kind, "context")
        XCTAssertEqual(graph.nodes[0].task.executionClientRef, "research")
        XCTAssertEqual(graph.nodes[2].task.dependencyContextRefs, ["research"])
        let recordedCommands = try await host.recordedCommands()
        let listCommand = try XCTUnwrap(recordedCommands.first)
        XCTAssertEqual(listCommand["type"], .string("get_message_task_graph"))
        XCTAssertEqual(listCommand["source_conversation_id"], .string("conversation-1"))
        XCTAssertEqual(listCommand["source_turn_id"], .string("turn-1"))

        let task = try await service.fetchTask(
            messageID: "message-1",
            taskID: "task-2",
            lookup: lookup
        )
        XCTAssertEqual(task.objective, "Ship locally")
        XCTAssertEqual(task.prerequisiteTaskIDs, ["task-1"])
        XCTAssertEqual(task.defaultModelConfig?.displayName, "gpt/gpt-6-sol")
        XCTAssertEqual(task.thinkingLevel, "high")
        XCTAssertEqual(task.lastRun?.resultSummary, "Completed locally")
        XCTAssertEqual(task.lastRun?.reportContent, "Completed locally")
        XCTAssertTrue(task.processLog?.contains("检查项目结构") == true)
        XCTAssertTrue(task.processLog?.contains("已确认入口和运行方式。") == true)
        XCTAssertFalse(task.processLog?.contains("list_dir") == true)
        XCTAssertFalse(task.processLog?.contains("private model output") == true)

        let defaultThinkingTask = try await service.fetchTask(
            messageID: "message-1",
            taskID: "task-1",
            lookup: lookup
        )
        XCTAssertEqual(defaultThinkingTask.defaultModelConfig?.displayName, "gpt/gpt-6-sol")
        XCTAssertEqual(defaultThinkingTask.thinkingLevel, "medium")

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
        let service = NativeLocalAgentMessageTaskGraphService(
            host: host,
            beforeRetry: { runID in
                await host.recordRelease(runID)
            }
        )
        await service.configure(ownerUserID: "user-1")

        _ = try await service.retryRun(
            messageID: "message-1",
            runID: "run-task-2",
            lookup: .init(turnID: "turn-1"),
            instruction: "Use the local fallback"
        )

        let command = try await host.lastCommand()
        let lifecycle = await host.recordedLifecycle()
        XCTAssertEqual(command["type"], .string("retry_task"))
        XCTAssertEqual(command["retry_instruction"], .string("Use the local fallback"))
        XCTAssertEqual(
            Array(lifecycle.suffix(2)),
            ["release:run-task-2", "command:retry_task"]
        )
    }

    func testTaskClientRestartsWithCurrentVersionAndReason() async throws {
        let host = LocalTaskHostStub()
        let client = NativeLocalAgentTaskClient(host: host)

        _ = try await client.restart(
            ownerUserID: "user-1",
            taskID: "task-2",
            expectedVersion: 2,
            reason: "restart from the beginning"
        )

        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("restart_task"))
        XCTAssertEqual(command["owner_user_id"], .string("user-1"))
        XCTAssertEqual(command["task_id"], .string("task-2"))
        XCTAssertEqual(command["expected_version"], .number(2))
        XCTAssertEqual(command["reason"], .string("restart from the beginning"))
    }

    func testProcessLogHonorsReplaceClearAndSuccessfulInvocationsOnly() throws {
        let events = try JSONDecoder().decode(
            [LocalAgentEventRecord].self,
            from: JSONSerialization.data(withJSONObject: [
                processEvent(cursor: 1, operation: "append", content: "first"),
                processEvent(cursor: 2, operation: "replace", content: "replacement"),
                processEvent(
                    cursor: 3,
                    operation: "append",
                    content: "failed entry",
                    status: "failed"
                ),
                processEvent(cursor: 4, operation: "clear", content: NSNull()),
                processEvent(cursor: 5, operation: "append", content: "final milestone"),
            ])
        )

        let log = NativeLocalAgentMessageTaskGraphService.processLog(from: events)

        XCTAssertTrue(log?.contains("final milestone") == true)
        XCTAssertFalse(log?.contains("first") == true)
        XCTAssertFalse(log?.contains("replacement") == true)
        XCTAssertFalse(log?.contains("failed entry") == true)
    }

    private func processEvent(
        cursor: Int,
        operation: String,
        content: Any,
        status: String = "succeeded"
    ) -> [String: Any] {
        [
            "cursor": cursor,
            "event_id": "event-\(cursor)",
            "run_id": "run-task-2",
            "event_type": "tool_batch_completed",
            "payload": [
                "invocations": [[
                    "tool_name": "task_run_process_record_process",
                    "status": status,
                    "arguments": [
                        "operation": operation,
                        "content": content,
                    ],
                ]],
            ],
            "created_at_unix_ms": cursor,
        ]
    }
}

private actor LocalTaskHostStub: LocalAgentHostClientServicing {
    private var commands: [Data] = []
    private var lifecycle: [String] = []

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        lifecycle.append("command:\(object?["type"] as? String ?? "unknown")")
        switch object?["type"] as? String {
        case "get_message_task_graph":
            return try json(["type": "message_task_graph", "graph": messageGraph()])
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
        case "get_task_graph", "cancel_task", "restart_task":
            return try json(["type": "task_graph", "graph": graph()])
        case "retry_task":
            return try json(["type": "task_graph", "graph": graph()])
        case "get_run":
            return try json([
                "type": "run",
                "run": run(),
            ])
        case "get_model_config_snapshot":
            return try json([
                "type": "model_config_snapshot",
                "snapshot": [
                    "owner_user_id": "user-1",
                    "model_config_ref": "model-1",
                    "model_config_revision": "revision-1",
                    "credential_ref": "env:MODEL_1_API_KEY",
                    "base_url": "https://api.example.test/v1",
                    "model": "gpt-6-sol",
                    "provider": "gpt",
                    "supports_responses": true,
                    "thinking_level": "medium",
                    "include_prompt_cache_retention": false,
                ],
            ])
        case "get_task_runs":
            let taskID = object?["task_id"] as? String ?? "task-1"
            return try json([
                "type": "task_runs",
                "task_id": taskID,
                "runs": taskID == "task-2" ? [run()] : [],
            ])
        case "list_events":
            return try json([
                "type": "events",
                "events": [
                    event(
                        cursor: 1,
                        type: "tool_batch_completed",
                        payload: [
                            "type": "tool_results",
                            "invocations": [
                                [
                                    "tool_name": "list_dir",
                                    "status": "succeeded",
                                    "arguments": ["path": "."],
                                ],
                                [
                                    "tool_name": "task_run_process_record_process",
                                    "status": "succeeded",
                                    "arguments": [
                                        "operation": "append",
                                        "heading": "检查项目结构",
                                        "content": "已确认入口和运行方式。",
                                    ],
                                ],
                            ],
                        ]
                    ),
                ],
                "next_cursor": 1,
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

    func recordedCommands() throws -> [[String: LocalAgentJSONValue]] {
        try commands.map { data in
            let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
            guard case let .object(object) = value else {
                throw CocoaError(.fileReadCorruptFile)
            }
            return object
        }
    }

    func recordRelease(_ runID: String) {
        lifecycle.append("release:\(runID)")
    }

    func recordedLifecycle() -> [String] {
        lifecycle
    }

    private func graph() -> [String: Any] {
        [
            "graph_id": "graph-1",
            "owner_user_id": "user-1",
            "source_entity_type": "conversation_turn",
            "source_entity_id": "turn-1",
            "status": "running",
            "tasks": [
                task("task-1", version: 1, clientRef: "research"),
                task("task-2", version: 2, clientRef: "implement"),
                task(
                    "task-3",
                    version: 1,
                    clientRef: "review",
                    contextRefs: ["research"]
                ),
            ],
            "dependencies": [[
                "task_id": "task-2",
                "prerequisite_task_id": "task-1",
            ]],
            "created_at_unix_ms": 1,
        ]
    }

    private func messageGraph() -> [String: Any] {
        let tasks = [
            task("task-1", version: 1, clientRef: "research"),
            task("task-2", version: 2, clientRef: "implement"),
            task(
                "task-3",
                version: 1,
                clientRef: "review",
                contextRefs: ["research"]
            ),
        ]
        return [
            "root_task_ids": ["task-1", "task-2", "task-3"],
            "nodes": tasks.map { task in
                [
                    "task": task,
                    "depth": 0,
                    "is_root": true,
                    "is_current_message": true,
                ]
            },
            "edges": [
                [
                    "source_task_id": "task-1",
                    "target_task_id": "task-2",
                    "kind": "prerequisite",
                ],
                [
                    "source_task_id": "task-1",
                    "target_task_id": "task-3",
                    "kind": "context",
                ],
            ],
            "source_conversation_id": "conversation-1",
            "source_turn_id": "turn-1",
            "source_user_message_id": "message-1",
        ]
    }

    private func task(
        _ id: String,
        version: Int,
        clientRef: String,
        contextRefs: [String] = []
    ) -> [String: Any] {
        let runtimeSettings: Any = id == "task-2"
            ? [
                "selected_thinking_level": "high",
                "reasoning_enabled": true,
            ]
            : NSNull()
        return [
            "graph_id": "graph-1",
            "owner_user_id": "user-1",
            "source_entity_type": "conversation_turn",
            "source_entity_id": "turn-1",
            "task_id": id,
            "title": id,
            "model_config_ref": "model-1",
            "model_config_revision": "revision-1",
            "input": [
                "objective": "Ship locally",
                "runtime_settings": runtimeSettings,
                "input_payload": [
                    "execution_client_ref": clientRef,
                    "dependency_context_refs": contextRefs,
                ],
            ],
            "status": id == "task-1" ? "succeeded" : "running",
            "active_run_id": NSNull(),
            "version": version,
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func run() -> [String: Any] {
        [
            "run_id": "run-task-2",
            "owner_user_id": "user-1",
            "owner_entity_type": "task",
            "owner_entity_id": "task-2",
            "profile_key": "task_execution",
            "input": [:],
            "status": "succeeded",
            "version": 3,
            "terminal_outcome": [
                "content": "Completed locally",
                "reasoning": "private model output",
            ],
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": 2,
        ]
    }

    private func event(cursor: Int, type: String, payload: [String: Any]) -> [String: Any] {
        [
            "cursor": cursor,
            "event_id": "event-\(cursor)",
            "run_id": "run-task-2",
            "event_type": type,
            "payload": payload,
            "created_at_unix_ms": cursor,
        ]
    }

    private func json(_ value: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: value)
    }
}
