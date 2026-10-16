@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentPetActivityServiceTests: XCTestCase {
    func testInterruptedActivityUsesTheSameLocalizedReasonAsTaskDetails() async throws {
        let host = PetActivityHostStub(interruptedTask: true)
        let service = NativeLocalAgentPetActivityService(host: host)
        await service.configure(ownerUserID: "user-1")
        let activities = try await service.fetchOpenActivities()
        let task = try XCTUnwrap(activities.first(where: { $0.source == .taskExecution }))
        XCTAssertEqual(task.kind, .reviewing)
        XCTAssertEqual(task.detail, NSLocalizedString("任务执行意外中断，无法确认当前步骤的结果。请先检查执行记录，再决定是否重试，避免重复操作。", comment: "Interrupted task execution"))
        XCTAssertFalse(try XCTUnwrap(task.detail).contains("Execution was interrupted"))
    }

    func testPetActivityRefreshSkipsRunProjectionNeutralEvents() {
        XCTAssertFalse(NativeLocalAgentPetActivityService.shouldRefresh(forEventTypes: [
            "tool_invocation_claimed",
            "tool_invocation_completed",
            "task_state_reconciled",
        ]))
        XCTAssertTrue(NativeLocalAgentPetActivityService.shouldRefresh(forEventTypes: [
            "tool_invocation_completed",
            "run_succeeded",
        ]))
        XCTAssertTrue(NativeLocalAgentPetActivityService.shouldRefresh(forEventTypes: [
            "future_run_transition",
        ]))
        XCTAssertTrue(NativeLocalAgentPetActivityService.shouldRefresh(forEventTypes: [
            "task_cancelled",
        ]))
    }

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

        let task = try XCTUnwrap(activities.first(where: { $0.source == .taskExecution }))
        XCTAssertEqual(task.kind, .failed)
        XCTAssertEqual(task.route.taskID, "task-1")
        XCTAssertEqual(task.detail, "build failed")

        let commands = try await host.recordedCommands()
        let terminalCommand = try XCTUnwrap(commands.first(where: {
            $0["scope"] == .string("terminal")
        }))
        guard case let .number(cutoff)? = terminalCommand["updated_after_unix_ms"] else {
            return XCTFail("Expected a terminal Run cutoff")
        }
        XCTAssertGreaterThan(cutoff, 0)
        XCTAssertLessThanOrEqual(cutoff, Double(PetActivityHostStub.now))
    }

    func testSurveyWaitingActivityRoutesToExactSurveyInsteadOfChatPrompt() async throws {
        for taskSurvey in [false, true] {
            let host = PetActivityHostStub(hasSurvey: true, taskSurvey: taskSurvey)
            let service = NativeLocalAgentPetActivityService(host: host)
            await service.configure(ownerUserID: "user-1")
            let activities = try await service.fetchOpenActivities()
            let waiting = try XCTUnwrap(activities.first(where: { $0.kind == .waitingForUser }))
            XCTAssertEqual(waiting.route.requirementSurveyID, "survey-1")
            XCTAssertEqual(waiting.route.projectID, "project-1")
            XCTAssertNil(waiting.route.promptID)
            XCTAssertEqual(waiting.route.taskID, taskSurvey ? "task-survey" : nil)
            XCTAssertEqual(waiting.title, "明确项目目标")
        }
    }

    func testLocalEventStreamUsesHostLongPolling() async throws {
        let host = PetActivityHostStub()
        let service = NativeLocalAgentPetActivityService(host: host)
        await service.configure(ownerUserID: "user-1")
        let stream = await service.petActivityEvents()
        var iterator = stream.makeAsyncIterator()

        let initial = try await iterator.next()
        let changed = try await iterator.next()
        XCTAssertEqual(initial, .reconcile)
        XCTAssertEqual(changed, .reconcile)
        let commands = try await host.recordedCommands()
        XCTAssertEqual(Array(commands.prefix(2)).map { $0["type"] }, [
            .string("get_event_cursor"),
            .string("wait_events"),
        ])
        let command = try XCTUnwrap(commands.dropFirst().first)
        XCTAssertEqual(command["type"], .string("wait_events"))
        XCTAssertEqual(command["owner_user_id"], .string("user-1"))
        XCTAssertEqual(command["after_cursor"], .number(41))
        XCTAssertEqual(command["timeout_ms"], .number(60_000))
        XCTAssertEqual(command["payload_mode"], .string("routing"))
    }
}

private actor PetActivityHostStub: LocalAgentHostClientServicing {
    private var commands: [Data] = []
    private let hasSurvey: Bool
    private let taskSurvey: Bool
    private let interruptedTask: Bool

    init(hasSurvey: Bool = false, taskSurvey: Bool = false, interruptedTask: Bool = false) {
        self.hasSurvey = hasSurvey
        self.taskSurvey = taskSurvey
        self.interruptedTask = interruptedTask
    }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        let type = object?["type"] as? String
        if type == "list_requirement_surveys" {
            let surveys: [[String: Any]] = hasSurvey ? [[
                "survey_id": "survey-1", "owner_user_id": "user-1",
                "project_resource_id": "project-1", "source_conversation_id": "conversation-1",
                "source_run_id": "run-chat", "source_task_id": taskSurvey ? "task-survey" : NSNull(),
                "title": "明确项目目标", "questions": [], "status": "open", "version": 1,
                "created_at_unix_ms": Self.now, "updated_at_unix_ms": Self.now,
            ]] : []
            return try JSONSerialization.data(withJSONObject: ["type": "requirement_surveys", "surveys": surveys])
        }
        if type == "get_event_cursor" {
            return try JSONSerialization.data(withJSONObject: [
                "type": "event_cursor",
                "cursor": 41,
            ])
        }
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
                "next_cursor": 42,
            ])
        }

        let scope = object?["scope"] as? String
        let runs: [[String: Any]] = scope == "active"
            ? [[
                "run_id": "run-chat",
                "owner_user_id": "user-1",
                "owner_entity_type": taskSurvey ? "task" : "conversation_turn",
                "owner_entity_id": taskSurvey ? "task-survey" : "turn-1",
                "profile_key": taskSurvey ? "task_execution" : "main_chat",
                "input": [
                    taskSurvey ? "source_conversation_id" : "conversation_id": "conversation-1",
                    taskSurvey ? "source_turn_id" : "turn_id": "turn-1",
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
                "status": interruptedTask ? "needs_review" : "failed",
                "version": 4,
                "terminal_outcome": interruptedTask
                    ? ["reason_code": "execution_interrupted", "error": "Execution was interrupted"]
                    : ["error": "build failed"],
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

    func recordedCommands() throws -> [[String: LocalAgentJSONValue]] {
        try commands.map { data in
            let value = try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
            guard case let .object(object) = value else {
                throw CocoaError(.fileReadCorruptFile)
            }
            return object
        }
    }

    fileprivate static let now = Int64(Date().timeIntervalSince1970 * 1_000)
}
