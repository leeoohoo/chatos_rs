@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentTurnProcessServiceTests: XCTestCase {
    func testLoadsDurableLocalEventsAndMergesToolLifecycle() async throws {
        let host = TurnProcessHostStub()
        let service = NativeLocalAgentTurnProcessService(host: host)
        await service.configure(ownerUserID: "user-1")

        let nodes = try await service.fetchProcessNodes(
            sessionID: "conversation-1",
            turnID: "turn-1"
        )

        XCTAssertEqual(nodes.first?.title, "开始处理")
        let tool = try XCTUnwrap(nodes.first(where: { $0.kind == .tool }))
        XCTAssertEqual(tool.title, "调用工具 · browser · open page")
        XCTAssertEqual(tool.status, .completed)
        XCTAssertEqual(tool.detail, "本地工具执行完成")
        let failure = try XCTUnwrap(nodes.first(where: { $0.title == "本地执行失败" }))
        XCTAssertEqual(failure.status, .failed)
        XCTAssertEqual(failure.detail, "详细信息已隐藏")

        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("list_events"))
        XCTAssertEqual(command["run_id"], .string("run-1"))
        XCTAssertEqual(command["owner_user_id"], .string("user-1"))
    }

    func testRequiresConfiguredOwnerBeforeReadingConversation() async {
        let service = NativeLocalAgentTurnProcessService(host: TurnProcessHostStub())
        do {
            _ = try await service.fetchProcessNodes(
                sessionID: "conversation-1",
                turnID: "turn-1"
            )
            XCTFail("unconfigured service must fail")
        } catch let error as NativeLocalAgentTurnProcessServiceError {
            guard case .notConfigured = error else {
                return XCTFail("unexpected error: \(error)")
            }
        } catch {
            XCTFail("unexpected error: \(error)")
        }
    }
}

private actor TurnProcessHostStub: LocalAgentHostClientServicing {
    private var commands: [Data] = []

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "get_conversation":
            return try json([
                "type": "conversation",
                "conversation": [
                    "conversation": [
                        "conversation_id": "conversation-1",
                        "owner_user_id": "user-1",
                        "title": "Local",
                        "version": 4,
                        "created_at_unix_ms": 1_000,
                        "updated_at_unix_ms": 5_000,
                    ],
                    "turns": [[
                        "turn_id": "turn-1",
                        "conversation_id": "conversation-1",
                        "user_message_id": "message-1",
                        "run_id": "run-1",
                        "status": "failed",
                        "created_at_unix_ms": 1_000,
                        "updated_at_unix_ms": 5_000,
                    ]],
                    "messages": [],
                    "attachments": [],
                ],
            ])
        case "list_events":
            return try json([
                "type": "events",
                "events": events(),
                "next_cursor": 5,
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

    private func events() -> [[String: Any]] {
        [
            event(1, "conversation_turn_started", [:]),
            event(2, "run_claimed", [:]),
            event(3, "tool_batch_requested", [
                "batch_id": "batch-1",
                "tool_calls": [[
                    "call_id": "call-1",
                    "tool_name": "browser__open_page",
                    "arguments": ["url": "https://example.com"],
                ]],
            ]),
            event(4, "tool_invocation_completed", [
                "call_id": "call-1",
                "status": "succeeded",
                "result": ["authorization": "must-not-be-rendered"],
            ]),
            event(5, "run_failed", ["error": "api_key=must-not-be-rendered"]),
        ]
    }

    private func event(_ cursor: Int, _ type: String, _ payload: [String: Any]) -> [String: Any] {
        [
            "cursor": cursor,
            "event_id": "event-\(cursor)",
            "run_id": "run-1",
            "event_type": type,
            "payload": payload,
            "created_at_unix_ms": cursor * 1_000,
        ]
    }

    private func json(_ value: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: value)
    }
}
