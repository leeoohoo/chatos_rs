@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentAskUserPromptServiceTests: XCTestCase {
    func testFetchesAndSubmitsChatPromptThroughLocalHost() async throws {
        let host = AskUserHostStub(mode: .chat)
        let service = NativeLocalAgentAskUserPromptService(host: host)
        await service.configure(ownerUserID: "user-1")

        let prompts = try await service.fetchPrompts(sessionID: "conversation-1", limit: 10)
        let prompt = try XCTUnwrap(prompts.first)
        XCTAssertEqual(prompt.id, "local-ask:run-chat")
        XCTAssertEqual(prompt.message, "请选择发布环境")
        XCTAssertEqual(prompt.choice?.options.map(\.value), ["staging", "production"])
        let listCommand = try await host.firstCommand()
        XCTAssertEqual(listCommand["status"], .string("waiting_user"))
        XCTAssertEqual(listCommand["limit"], .number(10))
        let eventCommand = try await host.firstCommand(type: "list_events")
        XCTAssertEqual(eventCommand["event_type"], .string("user_input_requested"))
        XCTAssertEqual(eventCommand["newest_first"], .bool(true))
        XCTAssertEqual(eventCommand["limit"], .number(1))

        let updated = try await service.submit(
            promptID: prompt.id,
            sessionID: prompt.sessionID,
            submission: .init(selection: .single("staging"))
        )
        XCTAssertEqual(updated.status, .ok)
        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("resume_conversation_turn"))
        XCTAssertEqual(command["expected_run_status"], .string("waiting_user"))
        XCTAssertEqual(command["message_metadata"]?.objectValue?["selection"], .string("staging"))
    }

    func testTaskPromptUsesSourceConversationAndCancelsTask() async throws {
        let host = AskUserHostStub(mode: .task)
        let service = NativeLocalAgentAskUserPromptService(host: host)
        await service.configure(ownerUserID: "user-1")

        let prompts = try await service.fetchPrompts(sessionID: "conversation-1", limit: 10)
        let prompt = try XCTUnwrap(prompts.first)
        XCTAssertEqual(prompt.turnID, "turn-1")

        _ = try await service.submit(
            promptID: prompt.id,
            sessionID: prompt.sessionID,
            submission: .init(selection: .single("staging"))
        )
        let submittedCommandTypes = try await host.commandTypes()
        XCTAssertTrue(submittedCommandTypes.contains("resume_run"))

        let updated = try await service.cancel(
            promptID: prompt.id,
            sessionID: prompt.sessionID
        )
        XCTAssertEqual(updated.status, .canceled)
        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("cancel_task"))
        XCTAssertEqual(command["task_id"], .string("task-1"))
        XCTAssertNil(command["expected_version"])
    }

    func testRejectsSecretAnswersBeforeResumeIPC() async throws {
        let host = AskUserHostStub(mode: .secret)
        let service = NativeLocalAgentAskUserPromptService(host: host)
        await service.configure(ownerUserID: "user-1")
        let prompts = try await service.fetchPrompts(sessionID: "conversation-1", limit: 10)
        let prompt = try XCTUnwrap(prompts.first)

        do {
            _ = try await service.submit(
                promptID: prompt.id,
                sessionID: prompt.sessionID,
                submission: .init(values: ["token": "must-not-cross-ipc"])
            )
            XCTFail("secret submission must be rejected")
        } catch let error as NativeLocalAgentAskUserPromptError {
            guard case .secretSubmissionUnsupported = error else {
                return XCTFail("unexpected error: \(error)")
            }
        }
        let commandTypes = try await host.commandTypes()
        XCTAssertFalse(commandTypes.contains("resume_conversation_turn"))
        XCTAssertFalse(commandTypes.contains("resume_run"))
    }
}

private actor AskUserHostStub: LocalAgentHostClientServicing {
    enum Mode: Equatable { case chat, task, secret }

    private let mode: Mode
    private var commands: [Data] = []

    init(mode: Mode) {
        self.mode = mode
    }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "list_runs":
            return try json([
                "type": "runs",
                "page": [
                    "runs": [run()],
                    "next_before_updated_at_unix_ms": NSNull(),
                    "next_before_run_id": NSNull(),
                ],
            ])
        case "list_events":
            return try json([
                "type": "events",
                "events": [[
                    "cursor": 3,
                    "event_id": "event-ask",
                    "run_id": runID,
                    "event_type": "user_input_requested",
                    "payload": ["prompt": prompt()],
                    "created_at_unix_ms": 2_000,
                ]],
                "next_cursor": 3,
            ])
        case "get_run":
            return try json(["type": "run", "run": run()])
        case "resume_run":
            var value = run()
            value["status"] = "continuation_ready"
            return try json(["type": "run", "run": value])
        case "get_conversation":
            return try json(["type": "conversation", "conversation": conversation()])
        case "resume_conversation_turn":
            return try json(turnMutation(status: "running"))
        case "cancel_conversation_turn":
            return try json(turnMutation(status: "cancelled"))
        case "cancel_task":
            return try json(["type": "task_graph", "graph": graph()])
        default:
            throw CocoaError(.featureUnsupported)
        }
    }

    func lastCommand() throws -> [String: LocalAgentJSONValue] {
        guard let command = commands.last else { throw CocoaError(.fileNoSuchFile) }
        return try Self.commandObject(command)
    }

    func firstCommand() throws -> [String: LocalAgentJSONValue] {
        guard let command = commands.first else { throw CocoaError(.fileNoSuchFile) }
        return try Self.commandObject(command)
    }

    func firstCommand(type: String) throws -> [String: LocalAgentJSONValue] {
        for command in commands {
            let object = try Self.commandObject(command)
            if object["type"]?.stringValue == type { return object }
        }
        throw CocoaError(.fileNoSuchFile)
    }

    func commandTypes() throws -> [String] {
        try commands.compactMap { try Self.commandObject($0)["type"]?.stringValue }
    }

    private var runID: String { mode == .task ? "run-task" : "run-chat" }

    private func run() -> [String: Any] {
        let task = mode == .task
        let input: [String: Any] = task
            ? ["source_conversation_id": "conversation-1", "source_turn_id": "turn-1"]
            : ["conversation_id": "conversation-1", "turn_id": "turn-1"]
        return [
            "run_id": runID,
            "owner_user_id": "user-1",
            "owner_entity_type": task ? "task" : "conversation_turn",
            "owner_entity_id": task ? "task-1" : "turn-1",
            "profile_key": task ? "task_execution" : "main_chat",
            "input": input,
            "status": "waiting_user",
            "version": 4,
            "created_at_unix_ms": 1_000,
            "updated_at_unix_ms": 2_000,
        ]
    }

    private func prompt() -> [String: Any] {
        if mode == .secret {
            return [
                "title": "凭据",
                "message": "请输入访问令牌",
                "payload": ["fields": [[
                    "key": "token", "label": "令牌", "required": true, "secret": true,
                ]]],
            ]
        }
        return [
            "title": "发布",
            "message": "请选择发布环境",
            "payload": ["choice": [
                "options": [
                    ["value": "staging", "label": "预发布"],
                    ["value": "production", "label": "生产环境"],
                ],
            ]],
        ]
    }

    private func conversation() -> [String: Any] {
        [
            "conversation": [
                "conversation_id": "conversation-1",
                "owner_user_id": "user-1",
                "title": "Local",
                "version": 2,
                "created_at_unix_ms": 1,
                "updated_at_unix_ms": 2,
            ],
            "turns": [], "messages": [], "attachments": [],
        ]
    }

    private func turnMutation(status: String) -> [String: Any] {
        [
            "type": "conversation_turn_updated",
            "result": [
                "conversation": conversation()["conversation"]!,
                "turn": [
                    "turn_id": "turn-1",
                    "conversation_id": "conversation-1",
                    "user_message_id": "message-1",
                    "run_id": "run-chat",
                    "status": status,
                    "created_at_unix_ms": 1,
                    "updated_at_unix_ms": 2,
                ],
                "message": NSNull(), "attachments": [],
            ],
        ]
    }

    private func graph() -> [String: Any] {
        [
            "graph_id": "graph-1",
            "owner_user_id": "user-1",
            "source_entity_type": "conversation_turn",
            "source_entity_id": "turn-1",
            "status": "cancelled",
            "tasks": [[
                "graph_id": "graph-1",
                "owner_user_id": "user-1",
                "source_entity_type": "conversation_turn",
                "source_entity_id": "turn-1",
                "task_id": "task-1",
                "title": "Task",
                "model_config_ref": "model-1",
                "input": [:],
                "status": "cancelled",
                "active_run_id": NSNull(),
                "version": 2,
                "created_at_unix_ms": 1,
                "updated_at_unix_ms": 2,
            ]],
            "dependencies": [],
            "created_at_unix_ms": 1,
        ]
    }

    private func json(_ value: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: value)
    }

    private static func commandObject(_ data: Data) throws -> [String: LocalAgentJSONValue] {
        let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
        guard case let .object(object) = value else { throw CocoaError(.fileReadCorruptFile) }
        return object
    }
}

private extension LocalAgentJSONValue {
    var objectValue: [String: LocalAgentJSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }

    var stringValue: String? {
        guard case let .string(value) = self else { return nil }
        return value
    }
}
