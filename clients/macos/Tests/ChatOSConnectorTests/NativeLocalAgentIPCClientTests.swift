// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSConnector
import ChatOSCore
import Foundation
import Testing

@Suite("Native local Agent IPC client")
struct NativeLocalAgentIPCClientTests {
    @Test("encodes the Rust tagged command and preserves plural ID fields")
    func encodesTaggedCommand() async throws {
        let transport = RecordingLocalAgentTransport(responseType: "accepted")
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)

        let operationID = try await client.accepted(.answerUserQuestion(
            runID: "run-1",
            interactionID: "interaction-1",
            answer: LocalAgentUserAnswer(
                text: "Use this visual direction",
                selectedOptionIDs: ["option-1"],
                attachments: []
            )
        ))

        #expect(operationID == "operation-1")
        let request = try #require(await transport.lastRequest())
        let object = try #require(
            JSONSerialization.jsonObject(with: request) as? [String: Any]
        )
        #expect(object["protocol_version"] as? Int == Int(localAgentProtocolVersion))
        #expect(object["owner_user_id"] as? String == "user-1")
        let command = try #require(object["command"] as? [String: Any])
        #expect(command["type"] as? String == "answer_user_question")
        let payload = try #require(command["payload"] as? [String: Any])
        #expect(payload["run_id"] as? String == "run-1")
        let answer = try #require(payload["answer"] as? [String: Any])
        #expect(answer["selected_option_ids"] as? [String] == ["option-1"])
        #expect(answer["selected_option_i_ds"] == nil)
    }

    @Test("decodes resumable event pages with the exact UInt64 cursor")
    func decodesEventCursor() async throws {
        let transport = RecordingLocalAgentTransport(responseType: "events")
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)

        let page = try await client.events(after: 9_007_199_254_740_992, limit: 50)

        #expect(page.events.count == 1)
        #expect(page.nextSequence == 9_007_199_254_740_993)
        #expect(page.hasMore)
        #expect(page.events[0].event == .hostStatus(LocalAgentHostRuntimeStatus(
            state: .ready,
            activeRunCount: 3,
            errorCode: nil
        )))
    }

    @Test("restores the durable UI cursor and authoritative Main Chat binding")
    func decodesCursorAndBinding() async throws {
        let cursorTransport = RecordingLocalAgentTransport(responseType: "ui_event_cursor")
        let cursorClient = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: cursorTransport
        )
        #expect(try await cursorClient.uiEventCursor() == 41)
        #expect(try await cursorClient.acknowledgeUIEvents(through: 42) == 41)
        let cursorRequest = try #require(await cursorTransport.lastRequest())
        let cursorObject = try #require(
            JSONSerialization.jsonObject(with: cursorRequest) as? [String: Any]
        )
        let cursorCommand = try #require(cursorObject["command"] as? [String: Any])
        #expect(cursorCommand["type"] as? String == "acknowledge_ui_events")
        let cursorPayload = try #require(cursorCommand["payload"] as? [String: Any])
        #expect(cursorPayload["through_seq"] as? Int == 42)

        let bindingTransport = RecordingLocalAgentTransport(responseType: "main_chat_run_binding")
        let bindingClient = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: bindingTransport
        )
        let binding = try await bindingClient.mainChatRunBinding(runID: "run-1")
        #expect(binding.runID == "run-1")
        #expect(binding.threadID == "thread-1")
        #expect(binding.turnID == "turn-1")
        #expect(binding.messageID == "message-1")
        #expect(binding.userMessage.content == "Design it")
        #expect(binding.userMessage.sequence == 1)
    }

    @Test("creates a Run and returns its authoritative local identity")
    func createsMainChatRun() async throws {
        let transport = RecordingLocalAgentTransport(responseType: "run_created")
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)
        let payload: LocalAgentJSONValue = .object(["prompt_revision": .string("prompt-1")])
        let snapshot = LocalAgentFrozenSnapshot(
            snapshotID: "snapshot-1",
            revision: "prompt-1",
            digest: "sha256:" + String(repeating: "a", count: 64),
            payload: payload
        )

        let created = try await client.createMainChatTurn(LocalAgentCreateMainChatTurn(
            threadID: "thread-1",
            turnID: "turn-1",
            messageID: "message-1",
            projectID: nil,
            modelConfigID: "model-1",
            promptSnapshot: snapshot,
            capabilitySnapshot: snapshot,
            projectSnapshot: nil,
            content: "Design the page",
            attachments: []
        ))

        #expect(created.operationID == "operation-1")
        #expect(created.run.runID == "run-1")
        #expect(created.run.ownerEntityID == "thread-1")
    }

    @Test("queries a bounded generic Run detail for restart recovery")
    func queriesRunDetail() async throws {
        let transport = RecordingLocalAgentTransport(responseType: "run_detail")
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)

        let detail = try await client.runDetail(id: "run-1", eventLimit: 50, eventOffset: 10)

        #expect(detail.run.runID == "run-1")
        #expect(detail.events.map(\.eventType) == ["message_assistant_reasoning"])
        #expect(detail.tools.isEmpty)
        let request = try #require(await transport.lastRequest())
        let object = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
        let command = try #require(object["command"] as? [String: Any])
        #expect(command["type"] as? String == "get_run_detail")
        let payload = try #require(command["payload"] as? [String: Any])
        #expect(payload["run_id"] as? String == "run-1")
        #expect(payload["event_limit"] as? Int == 50)
        #expect(payload["event_offset"] as? Int == 10)
    }

    @Test("retries a Task as a new Run with the expected current Run identity")
    func retriesTask() async throws {
        let transport = RecordingLocalAgentTransport(responseType: "run_created")
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)

        _ = try await client.retryTask(LocalAgentRetryTask(
            taskID: "task-1",
            expectedRunID: "task-run-1",
            instruction: "Preserve the approved visual hierarchy."
        ))

        let request = try #require(await transport.lastRequest())
        let object = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
        let command = try #require(object["command"] as? [String: Any])
        #expect(command["type"] as? String == "retry_task")
        let payload = try #require(command["payload"] as? [String: Any])
        #expect(payload["task_id"] as? String == "task-1")
        #expect(payload["expected_run_id"] as? String == "task-run-1")
    }

    @Test("restores Task identity and frozen planning input from the Host")
    func restoresTaskSnapshots() async throws {
        let transport = RecordingLocalAgentTransport(responseType: "tasks")
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)

        let page = try await client.tasks(limit: 20)

        let task = try #require(page.tasks.first)
        #expect(task.taskID == "task-1")
        #expect(task.initialRunID == "task-run-1")
        #expect(task.currentRunID == "task-run-2")
        #expect(task.runIDs == ["task-run-1", "task-run-2"])
        #expect(task.sourceThreadID == "thread-1")
        #expect(task.sourceTurnID == "turn-1")
        #expect(task.projectID == "project-1")
        #expect(task.acceptanceCriteria == ["Match the approved visual", "Pass visual QA"])
        #expect(page.nextCursor == nil)

        let request = try #require(await transport.lastRequest())
        let object = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
        let command = try #require(object["command"] as? [String: Any])
        #expect(command["type"] as? String == "list_tasks")
        let payload = try #require(command["payload"] as? [String: Any])
        #expect(payload["limit"] as? Int == 20)
    }

    @Test("queries Rust-owned Task Graph and historical Run detail projections")
    func queriesTaskProjections() async throws {
        let graphTransport = try FixtureLocalAgentTransport(
            fixture: fixtureURL("task_graph_response.json")
        )
        let graphClient = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: graphTransport
        )
        let graph = try await graphClient.taskGraph(
            sourceThreadID: "thread-1",
            sourceTurnID: "turn-1"
        )
        #expect(graph.rootTaskIDs == ["task-1"])
        #expect(graph.nodes.first?.task.currentRun.resultSummary == "Design implemented")
        let graphRequest = try #require(await graphTransport.request())
        let graphObject = try #require(
            JSONSerialization.jsonObject(with: graphRequest) as? [String: Any]
        )
        let graphCommand = try #require(graphObject["command"] as? [String: Any])
        #expect(graphCommand["type"] as? String == "get_task_graph")
        let graphPayload = try #require(graphCommand["payload"] as? [String: Any])
        #expect(graphPayload["source_thread_id"] as? String == "thread-1")

        let detailTransport = try FixtureLocalAgentTransport(
            fixture: fixtureURL("task_run_detail_response.json")
        )
        let detailClient = try NativeLocalAgentIPCClient(
            ownerUserID: "user-1",
            transport: detailTransport
        )
        let detail = try await detailClient.taskRunDetail(
            taskID: "task-1",
            runID: "task-run-2",
            eventLimit: 40,
            eventOffset: 0
        )
        #expect(detail.run.run.runID == "task-run-2")
        #expect(detail.events.first?.eventType == "run_started")
        let detailRequest = try #require(await detailTransport.request())
        let detailObject = try #require(
            JSONSerialization.jsonObject(with: detailRequest) as? [String: Any]
        )
        let detailCommand = try #require(detailObject["command"] as? [String: Any])
        #expect(detailCommand["type"] as? String == "get_task_run_detail")
        let detailPayload = try #require(detailCommand["payload"] as? [String: Any])
        #expect(detailPayload["event_limit"] as? Int == 40)
    }

    @Test("encodes one project Plugin capability for Host validation")
    func encodesPluginCapabilityMutation() async throws {
        let transport = RecordingLocalAgentTransport(responseType: "success")
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)

        let response = try await client.send(.installProjectPluginCapability(
            projectID: "project-1",
            pluginID: "plugin-1",
            releaseID: "release-1",
            capabilityRecord: .object([
                "schema_version": .signed(2),
                "project_id": .string("project-1"),
            ])
        ))

        #expect(response == .success)
        let request = try #require(await transport.lastRequest())
        let object = try #require(JSONSerialization.jsonObject(with: request) as? [String: Any])
        let command = try #require(object["command"] as? [String: Any])
        #expect(command["type"] as? String == "install_project_plugin_capability")
        let payload = try #require(command["payload"] as? [String: Any])
        #expect(payload["project_id"] as? String == "project-1")
        #expect(payload["plugin_id"] as? String == "plugin-1")
        let capability = try #require(payload["capability_record"] as? [String: Any])
        #expect(capability["schema_version"] as? Int == 2)
    }

    @Test("rejects a response correlated to another request")
    func rejectsRequestMismatch() async throws {
        let transport = RecordingLocalAgentTransport(
            responseType: "accepted",
            replaceRequestID: "another-request"
        )
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)

        await #expect(throws: NativeLocalAgentIPCError.self) {
            _ = try await client.accepted(.pauseRun(runID: "run-1"))
        }
    }

    private func fixtureURL(_ name: String) -> URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("shared/fixtures/local_agent/v12")
            .appendingPathComponent(name)
    }
}

private actor FixtureLocalAgentTransport: LocalAgentFrameTransport {
    private let fixture: [String: Any]
    private var lastRequest: Data?

    init(fixture url: URL) throws {
        fixture = try #require(
            JSONSerialization.jsonObject(with: Data(contentsOf: url)) as? [String: Any]
        )
    }

    func exchange(_ request: Data) async throws -> Data {
        lastRequest = request
        let requestObject = try #require(
            JSONSerialization.jsonObject(with: request) as? [String: Any]
        )
        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": requestObject["request_id"] as? String ?? "missing",
            "response": try #require(fixture["response"]),
        ])
    }

    func request() -> Data? { lastRequest }
}

private actor RecordingLocalAgentTransport: LocalAgentFrameTransport {
    private let responseType: String
    private let replaceRequestID: String?
    private var request: Data?

    init(responseType: String, replaceRequestID: String? = nil) {
        self.responseType = responseType
        self.replaceRequestID = replaceRequestID
    }

    func exchange(_ request: Data) async throws -> Data {
        self.request = request
        let object = try #require(
            JSONSerialization.jsonObject(with: request) as? [String: Any]
        )
        let requestID = replaceRequestID ?? (object["request_id"] as? String ?? "missing")
        let response: [String: Any]
        switch responseType {
        case "events":
            response = [
                "type": "events",
                "payload": [
                    "events": [[
                        "event_seq": UInt64(9_007_199_254_740_993),
                        "emitted_at": "2026-09-12T03:00:00Z",
                        "event": [
                            "type": "host_status",
                            "payload": [
                                "state": "ready",
                                "active_run_count": 3,
                            ],
                        ],
                    ]],
                    "next_seq": UInt64(9_007_199_254_740_993),
                    "has_more": true,
                ],
            ]
        case "ui_event_cursor":
            response = [
                "type": "ui_event_cursor",
                "payload": ["event_seq": 41],
            ]
        case "main_chat_run_binding":
            response = [
                "type": "main_chat_run_binding",
                "payload": [
                    "run_id": "run-1",
                    "thread_id": "thread-1",
                    "turn_id": "turn-1",
                    "message_id": "message-1",
                    "user_message": [
                        "record_id": "message-1",
                        "run_id": "run-1",
                        "thread_id": "thread-1",
                        "turn_id": "turn-1",
                        "sequence": 1,
                        "role": "user",
                        "content": "Design it",
                        "message_mode": "semantic",
                        "message_source": "main_chat",
                        "memory_sync_status": "pending",
                        "created_at": "2026-09-12T03:00:00Z",
                    ],
                ],
            ]
        case "success":
            response = ["type": "success"]
        case "tasks":
            response = [
                "type": "tasks",
                "payload": [
                    "tasks": [[
                        "task_id": "task-1",
                        "revision": 2,
                        "source_thread_id": "thread-1",
                        "source_turn_id": "turn-1",
                        "project_id": "project-1",
                        "initial_run_id": "task-run-1",
                        "current_run_id": "task-run-2",
                        "run_ids": ["task-run-1", "task-run-2"],
                        "objective": "Implement the approved visual design",
                        "acceptance_criteria": [
                            "Match the approved visual",
                            "Pass visual QA",
                        ],
                        "status": "running",
                        "model_config_id": "model-task-1",
                        "model_config_revision": 4,
                        "created_at": "2026-09-12T03:00:00Z",
                        "updated_at": "2026-09-12T03:01:00Z",
                    ]],
                    "next_cursor": NSNull(),
                ],
            ]
        case "run_created":
            response = [
                "type": "run_created",
                "payload": [
                    "operation_id": "operation-1",
                    "run": [
                        "run_id": "run-1",
                        "profile_key": "main_chat",
                        "owner_user_id": "user-1",
                        "owner_entity_type": "conversation",
                        "owner_entity_id": "thread-1",
                        "status": "queued",
                        "version": 1,
                        "step_seq": 0,
                        "iteration": 0,
                        "retry_count": 0,
                        "model_config_id": "model-1",
                        "model_config_revision": 1,
                        "model_runtime_snapshot": [:],
                        "context_strategy": "provider_native",
                        "prompt_revision": "prompt-1",
                        "capability_snapshot_ref": "capabilities-1",
                        "created_at": "2026-09-12T03:00:00Z",
                        "updated_at": "2026-09-12T03:00:00Z",
                    ],
                ],
            ]
        case "run_detail":
            response = [
                "type": "run_detail",
                "payload": [
                    "run": [
                        "run_id": "run-1",
                        "profile_key": "main_chat",
                        "owner_user_id": "user-1",
                        "owner_entity_type": "conversation",
                        "owner_entity_id": "thread-1",
                        "status": "model_running",
                        "version": 2,
                        "step_seq": 1,
                        "iteration": 0,
                        "retry_count": 0,
                        "model_config_id": "model-1",
                        "model_config_revision": 1,
                        "model_runtime_snapshot": [:],
                        "context_strategy": "provider_native",
                        "prompt_revision": "prompt-1",
                        "capability_snapshot_ref": "capabilities-1",
                        "created_at": "2026-09-12T03:00:00Z",
                        "updated_at": "2026-09-12T03:01:00Z",
                    ],
                    "events": [[
                        "event_id": "message:assistant-1:reasoning",
                        "event_type": "message_assistant_reasoning",
                        "message": "Inspecting the hierarchy",
                        "created_at": "2026-09-12T03:00:30Z",
                    ]],
                    "tools": [],
                    "events_total": 1,
                    "events_has_more": false,
                    "snapshot_event_sequence": 42,
                ],
            ]
        default:
            response = [
                "type": "accepted",
                "payload": ["operation_id": "operation-1"],
            ]
        }
        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": localAgentProtocolVersion,
            "request_id": requestID,
            "response": response,
        ])
    }

    func lastRequest() -> Data? { request }
}
