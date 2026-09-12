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
        #expect(object["protocol_version"] as? Int == 1)
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
        #expect(page.events[0].event.payload == .object([
            "model_config_id": .string("opaque-value"),
        ]))
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
                            "payload": ["model_config_id": "opaque-value"],
                        ],
                    ]],
                    "next_seq": UInt64(9_007_199_254_740_993),
                    "has_more": true,
                ],
            ]
        default:
            response = [
                "type": "accepted",
                "payload": ["operation_id": "operation-1"],
            ]
        }
        return try JSONSerialization.data(withJSONObject: [
            "protocol_version": 1,
            "request_id": requestID,
            "response": response,
        ])
    }

    func lastRequest() -> Data? { request }
}
