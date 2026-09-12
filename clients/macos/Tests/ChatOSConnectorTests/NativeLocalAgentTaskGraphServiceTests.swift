// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native local Task Graph service")
struct NativeLocalAgentTaskGraphServiceTests {
    @Test("maps the Rust-owned graph and Run detail without remote lookup state")
    func mapsAuthoritativeLocalProjections() async throws {
        let fixture = try TaskGraphServiceFixture()

        let graph = try await fixture.service.fetchGraph(
            sourceThreadID: "thread-1",
            sourceTurnID: "turn-1"
        )
        let node = try #require(graph.nodes.first)
        #expect(graph.sourceSessionID == "thread-1")
        #expect(graph.sourceTurnID == "turn-1")
        #expect(graph.rootTaskIDs == ["task-1"])
        #expect(node.task.sourceSessionID == "thread-1")
        #expect(node.task.sourceTurnID == "turn-1")
        #expect(node.task.lastRunID == "task-run-2")
        #expect(node.task.resultSummary == "Design implemented")
        #expect(node.task.createdAt != nil)
        #expect(node.task.updatedAt != nil)

        let detail = try await fixture.service.fetchRun(
            taskID: "task-1",
            runID: "task-run-2",
            includeEvents: true,
            eventLimit: 40,
            eventOffset: 0
        )
        #expect(detail.task.id == "task-1")
        #expect(detail.run.id == "task-run-2")
        #expect(detail.run.reportContent?.contains("Design implemented") == true)
        #expect(detail.events.map(\.eventType) == ["run_started"])
        #expect(detail.eventsTotal == 1)
        #expect(!detail.eventsHasMore)

        #expect(await fixture.transport.commandTypes() == [
            "get_task_graph",
            "get_task_run_detail",
        ])
    }

    @Test("retry and cancel use the exact Task and current Run identities")
    func targetsAuthoritativeTaskRuns() async throws {
        let fixture = try TaskGraphServiceFixture()

        let retry = try await fixture.service.retryTask(
            taskID: "task-1",
            expectedRunID: "task-run-2",
            instruction: "  Preserve visual hierarchy.  "
        )
        #expect(retry.taskID == "task-1")
        #expect(retry.id == "task-run-3")

        try await fixture.service.cancelTask(taskID: "task-1")

        let commands = try await fixture.transport.requests().map(Self.command)
        #expect(commands.map { $0["type"] as? String } == [
            "retry_task",
            "get_task",
            "get_run",
            "cancel_run",
        ])
        let retryPayload = try #require(commands[0]["payload"] as? [String: Any])
        #expect(retryPayload["task_id"] as? String == "task-1")
        #expect(retryPayload["expected_run_id"] as? String == "task-run-2")
        #expect(retryPayload["instruction"] as? String == "Preserve visual hierarchy.")
        let cancelPayload = try #require(commands[3]["payload"] as? [String: Any])
        #expect(cancelPayload["run_id"] as? String == "task-run-2")
        #expect(cancelPayload["expected_version"] as? UInt64 == 5)
    }

    private static func command(_ request: Data) throws -> [String: Any] {
        let object = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
        return try #require(object["command"] as? [String: Any])
    }
}

private struct TaskGraphServiceFixture {
    let service: NativeLocalAgentTaskGraphService
    let transport: TaskGraphServiceTransport

    init() throws {
        let transport = try TaskGraphServiceTransport(fixturesDirectory: Self.fixturesDirectory)
        let client = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: transport
        )
        service = NativeLocalAgentTaskGraphService(
            accountSession: TaskGraphAccountSession(client: client)
        )
        self.transport = transport
    }

    private static var fixturesDirectory: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("shared/fixtures/local_agent/v15")
    }
}

private struct TaskGraphAccountSession: NativeLocalAgentAccountSessionAccess {
    let client: NativeLocalAgentIPCClient

    func client(accountID: String) async throws -> NativeLocalAgentIPCClient { client }
    func activeClient() async throws -> NativeLocalAgentIPCClient { client }
    func stageAttachments(
        _ attachments: [ConversationAttachmentDraft],
        accountID: String
    ) async throws -> [LocalAgentAttachmentReference] { [] }
    func discardStagedAttachments(
        _ references: [LocalAgentAttachmentReference],
        accountID: String
    ) async {}
}

private actor TaskGraphServiceTransport: LocalAgentFrameTransport {
    private let graph: [String: Any]
    private let task: [String: Any]
    private let detail: [String: Any]
    private var recordedRequests: [Data] = []

    init(fixturesDirectory: URL) throws {
        graph = try Self.responsePayload(
            fixturesDirectory.appendingPathComponent("task_graph_response.json")
        )
        task = try Self.responsePayload(
            fixturesDirectory.appendingPathComponent("task_snapshot_response.json")
        )
        detail = try Self.responsePayload(
            fixturesDirectory.appendingPathComponent("task_run_detail_response.json")
        )
    }

    func exchange(_ request: Data) async throws -> Data {
        let object = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
        let command = try #require(object["command"] as? [String: Any])
        let type = try #require(command["type"] as? String)
        recordedRequests.append(request)

        let response: [String: Any]
        switch type {
        case "get_task_graph": response = graph
        case "get_task": response = task
        case "get_run":
            let detailPayload = try #require(detail["payload"] as? [String: Any])
            let runSummary = try #require(detailPayload["run"] as? [String: Any])
            var run = try #require(runSummary["run"] as? [String: Any])
            run["status"] = "model_running"
            run["version"] = 5
            run["terminal_outcome"] = NSNull()
            response = ["type": "run", "payload": run]
        case "get_task_run_detail": response = detail
        case "retry_task":
            let detailPayload = try #require(detail["payload"] as? [String: Any])
            let runSummary = try #require(detailPayload["run"] as? [String: Any])
            var run = try #require(runSummary["run"] as? [String: Any])
            run["run_id"] = "task-run-3"
            run["status"] = "queued"
            run["version"] = 1
            run["step_seq"] = 0
            run["iteration"] = 0
            run["retry_count"] = 1
            run["terminal_outcome"] = NSNull()
            response = [
                "type": "run_created",
                "payload": [
                    "operation_id": "operation-retry-1",
                    "run": run,
                ],
            ]
        case "cancel_run":
            response = [
                "type": "accepted",
                "payload": ["operation_id": "operation-cancel-1"],
            ]
        default:
            throw TaskGraphServiceTestError.unexpectedCommand(type)
        }
        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": object["request_id"] as? String ?? "missing",
            "response": response,
        ])
    }

    func requests() -> [Data] { recordedRequests }

    func commandTypes() -> [String] {
        recordedRequests.compactMap { request in
            guard let object = try? JSONSerialization.jsonObject(with: request) as? [String: Any],
                  let command = object["command"] as? [String: Any]
            else { return nil }
            return command["type"] as? String
        }
    }

    private static func responsePayload(_ url: URL) throws -> [String: Any] {
        let object = try #require(
            JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any]
        )
        return try #require(object["response"] as? [String: Any])
    }
}

private enum TaskGraphServiceTestError: Error {
    case unexpectedCommand(String)
}
