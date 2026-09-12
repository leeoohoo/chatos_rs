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

    @Test("encodes one project Plugin capability for Host validation")
    func encodesPluginCapabilityMutation() async throws {
        let transport = RecordingLocalAgentTransport(responseType: "success")
        let client = try NativeLocalAgentIPCClient(ownerUserID: "user-1", transport: transport)

        let response = try await client.send(.installProjectPluginCapability(
            projectID: "project-1",
            pluginID: "plugin-1",
            releaseID: "release-1",
            capabilityRecord: .object([
                "schema_version": .signed(1),
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
        #expect(capability["schema_version"] as? Int == 1)
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
                ],
            ]
        case "success":
            response = ["type": "success"]
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
