@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentRequirementSurveyClientTests: XCTestCase {
    func testFollowUpReadsExactSourceRunAndOnlyFinalUserReply() async throws {
        for main in [true, false] {
            let host = RequirementSurveyHostStub(main: main, runStatus: "succeeded")
            let client = NativeLocalAgentRequirementSurveyClient(host: host)
            let result = try await client.resolve(ownerUserID: "user-1", surveyID: "survey-1", expectedVersion: 1, answers: [:])
            let followUp = try await client.followUp(ownerUserID: "user-1", survey: result.survey)
            XCTAssertEqual(followUp, .completed("## 需求方案\n交付虚构原型，分阶段验收。"))
            let command = try await host.lastCommand()
            XCTAssertEqual(command["type"], .string("get_run"))
            XCTAssertEqual(command["run_id"], .string("run-1"))
            XCTAssertEqual(command["owner_user_id"], .string("user-1"))
        }
    }

    func testSubmissionDoesNotImplyCompletedProposalAndFailuresStayVisible() async throws {
        let cases: [(String, RequirementSurveyFollowUp)] = [
            ("continuation_ready", .processing), ("model_running", .processing),
            ("waiting_user", .waitingForInput), ("needs_review", .needsReview),
            ("failed", .failed), ("cancelled", .cancelled),
        ]
        for (status, expected) in cases {
            let host = RequirementSurveyHostStub(runStatus: status)
            let client = NativeLocalAgentRequirementSurveyClient(host: host)
            let result = try await client.resolve(ownerUserID: "user-1", surveyID: "survey-1", expectedVersion: 1, answers: [:])
            let followUp = try await client.followUp(ownerUserID: "user-1", survey: result.survey)
            XCTAssertEqual(followUp, expected)
        }
    }

    func testFollowUpRejectsCrossOwnerAndMismatchedConversation() async throws {
        for wrongOwner in [true, false] {
            let host = RequirementSurveyHostStub(mismatchedConversation: !wrongOwner)
            let client = NativeLocalAgentRequirementSurveyClient(host: host)
            let result = try await client.resolve(ownerUserID: "user-1", surveyID: "survey-1", expectedVersion: 1, answers: [:])
            do {
                _ = try await client.followUp(ownerUserID: wrongOwner ? "other-user" : "user-1", survey: result.survey)
                XCTFail("Mismatched source should be rejected")
            } catch { XCTAssertTrue(error is NativeLocalAgentHostError) }
        }
    }

    func testListsAndDecodesAllQuestionKinds() async throws {
        let host = RequirementSurveyHostStub()
        let client = NativeLocalAgentRequirementSurveyClient(host: host)

        let surveys = try await client.list(
            ownerUserID: "user-1",
            projectResourceID: "project-1",
            status: .open
        )

        XCTAssertEqual(surveys.count, 1)
        XCTAssertEqual(surveys[0].questions.map(\.responseKind), [
            .text, .singleChoice, .multipleChoice, .boolean,
        ])
        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("list_requirement_surveys"))
        XCTAssertEqual(command["project_resource_id"], .string("project-1"))
        XCTAssertEqual(command["status"], .string("open"))
        XCTAssertEqual(command["limit"], .number(200))
    }

    func testResolveSendsTypedAnswersAndReturnsResumedRun() async throws {
        let host = RequirementSurveyHostStub()
        let client = NativeLocalAgentRequirementSurveyClient(host: host)

        let resolution = try await client.resolve(
            ownerUserID: "user-1",
            surveyID: "survey-1",
            expectedVersion: 1,
            answers: [
                "details": .string("Keep it local"),
                "approved": .bool(true),
                "scope": .string("client"),
                "targets": .array([.string("macOS"), .string("Windows")]),
            ]
        )

        XCTAssertEqual(resolution.survey.status, .resolved)
        XCTAssertEqual(resolution.survey.version, 2)
        XCTAssertEqual(resolution.resumedRun.runID, "run-1")
        XCTAssertEqual(resolution.resumedRun.status, "continuation_ready")
        let command = try await host.lastCommand()
        XCTAssertEqual(command["type"], .string("resolve_requirement_survey"))
        XCTAssertEqual(command["expected_version"], .number(1))
        guard case let .object(answers) = command["answers"] else {
            return XCTFail("answers must be an object")
        }
        XCTAssertEqual(answers["approved"], .bool(true))
        XCTAssertEqual(answers["details"], .string("Keep it local"))
    }
}

private actor RequirementSurveyHostStub: LocalAgentHostClientServicing {
    private var commands: [Data] = []
    private let main: Bool
    private let runStatus: String
    private let mismatchedConversation: Bool

    init(main: Bool = false, runStatus: String = "continuation_ready", mismatchedConversation: Bool = false) {
        self.main = main
        self.runStatus = runStatus
        self.mismatchedConversation = mismatchedConversation
    }

    func start(ownerUserID: String) async throws {}
    func stop() async {}

    func request(command: Data) async throws -> Data {
        commands.append(command)
        let object = try JSONSerialization.jsonObject(with: command) as? [String: Any]
        switch object?["type"] as? String {
        case "get_run":
            return try json(["type": "run", "run": [
                "run_id": "run-1", "owner_user_id": "user-1",
                "owner_entity_type": main ? "conversation_turn" : "task",
                "owner_entity_id": main ? "turn-1" : "task-1",
                "profile_key": main ? "main_chat" : "task_execution",
                "capability_policy_revision": "policy-1",
                "input": [main ? "conversation_id" : "source_conversation_id": mismatchedConversation ? "other-conversation" : "conversation-1"],
                "status": runStatus, "version": 4,
                "terminal_outcome": [
                    "content": "## 需求方案\n交付虚构原型，分阶段验收。",
                    "request_input_items": [["content": "INTERNAL_TRACE_DO_NOT_DISPLAY"]],
                    "error": "INTERNAL_ERROR_DO_NOT_DISPLAY",
                ],
                "created_at_unix_ms": 1, "updated_at_unix_ms": 4,
            ]])
        case "list_requirement_surveys":
            return try json(["type": "requirement_surveys", "surveys": [survey(resolved: false)]])
        case "get_requirement_survey":
            return try json(["type": "requirement_survey", "survey": survey(resolved: false)])
        case "resolve_requirement_survey":
            return try json([
                "type": "requirement_survey_resolved",
                "resolution": [
                    "survey": survey(resolved: true),
                    "resumed_run": [
                        "run_id": "run-1",
                        "owner_user_id": "user-1",
                        "owner_entity_type": "task",
                        "owner_entity_id": "task-1",
                        "profile_key": "task_execution",
                        "capability_policy_revision": "policy-1",
                        "input": ["continuation": "survey-1"],
                        "status": "continuation_ready",
                        "version": 3,
                        "terminal_outcome": NSNull(),
                        "created_at_unix_ms": 1,
                        "updated_at_unix_ms": 3,
                    ],
                ],
            ])
        default:
            throw NativeLocalAgentHostError.invalidCommand
        }
    }

    func lastCommand() throws -> LocalAgentJSONValue {
        guard let data = commands.last else { throw NativeLocalAgentHostError.invalidCommand }
        return try JSONDecoder().decode(LocalAgentJSONValue.self, from: data)
    }

    private func survey(resolved: Bool) -> [String: Any] {
        [
            "survey_id": "survey-1",
            "owner_user_id": "user-1",
            "project_resource_id": "project-1",
            "source_conversation_id": "conversation-1",
            "source_run_id": "run-1",
            "source_task_id": main ? NSNull() : "task-1",
            "title": "Confirm local scope",
            "description": "Choose how to continue",
            "questions": [
                question("details", "text", []),
                question("scope", "single_choice", ["client", "server"]),
                question("targets", "multiple_choice", ["macOS", "Windows"]),
                question("approved", "boolean", []),
            ],
            "answers": resolved ? [
                "details": "Keep it local",
                "scope": "client",
                "targets": ["macOS", "Windows"],
                "approved": true,
            ] : NSNull(),
            "status": resolved ? "resolved" : "open",
            "version": resolved ? 2 : 1,
            "created_at_unix_ms": 1,
            "updated_at_unix_ms": resolved ? 3 : 1,
            "resolved_at_unix_ms": resolved ? 3 : NSNull(),
        ]
    }

    private func question(_ id: String, _ kind: String, _ options: [String]) -> [String: Any] {
        [
            "question_id": id,
            "prompt": "Question \(id)",
            "response_kind": kind,
            "required": true,
            "options": options,
        ]
    }

    private func json(_ value: Any) throws -> Data {
        try JSONSerialization.data(withJSONObject: value)
    }
}

private extension LocalAgentJSONValue {
    subscript(key: String) -> LocalAgentJSONValue? {
        guard case let .object(values) = self else { return nil }
        return values[key]
    }
}
