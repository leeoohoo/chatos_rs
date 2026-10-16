@testable import ChatOSConnector
import ChatOSCore
import Foundation
import XCTest

final class NativeLocalAgentRequirementPlanClientTests: XCTestCase {
    private func survey() throws -> LocalAgentHostRequirementSurvey {
        try JSONDecoder().decode(LocalAgentHostRequirementSurvey.self, from: JSONSerialization.data(withJSONObject: [
            "survey_id": "survey-1", "owner_user_id": "user-1", "project_resource_id": "project-1",
            "source_conversation_id": "chat-1", "source_run_id": "run-1", "title": "Fictional finance",
            "questions": [], "answers": [:], "status": "resolved", "version": 2,
            "created_at_unix_ms": 1, "updated_at_unix_ms": 2, "resolved_at_unix_ms": 2,
        ]))
    }

    func testGetAndHumanConfirmSendExactRevisionWithoutStartingWork() async throws {
        let host = RequirementPlanHostStub()
        let client = NativeLocalAgentRequirementPlanClient(host: host)
        let fetched = try await client.get(ownerUserID: "user-1", survey: survey(), revision: 1)
        let plan = try XCTUnwrap(fetched)
        let confirmed = try await client.confirm(ownerUserID: "user-1", survey: survey(), plan: plan)
        XCTAssertEqual(confirmed.markdown, plan.markdown)
        XCTAssertNotNil(confirmed.confirmedAtUnixMs)
        let recorded = try await host.recorded()
        let commands = try JSONSerialization.jsonObject(with: Data(recorded.utf8)) as! [[String: Any]]
        XCTAssertEqual(commands.map { $0["type"] as? String }, ["get_requirement_plan", "confirm_requirement_plan"])
        XCTAssertEqual(commands.last?["expected_revision"] as? Int, 1)
        XCTAssertEqual(commands.last?["owner_user_id"] as? String, "user-1")
    }

    func testRejectsWrongOwnerProjectRevisionAndConfirmationBody() async throws {
        for field in ["owner_user_id", "project_resource_id", "revision", "markdown", "source_run_id"] {
            let host = RequirementPlanHostStub(corruptConfirmation: field)
            let client = NativeLocalAgentRequirementPlanClient(host: host)
            let fetched = try await client.get(ownerUserID: "user-1", survey: survey())
            let plan = try XCTUnwrap(fetched)
            do { _ = try await client.confirm(ownerUserID: "user-1", survey: survey(), plan: plan); XCTFail("must reject \(field)") }
            catch { XCTAssertTrue(error is NativeLocalAgentHostError) }
        }
        let host = RequirementPlanHostStub()
        do { _ = try await NativeLocalAgentRequirementPlanClient(host: host).get(ownerUserID: "other", survey: survey()); XCTFail("owner") }
        catch { XCTAssertTrue(error is NativeLocalAgentHostError) }
        let recorded = try await host.recorded()
        let commands = try JSONSerialization.jsonObject(with: Data(recorded.utf8)) as! [[String: Any]]
        XCTAssertTrue(commands.isEmpty)
    }

    func testLegacyReadDoesNotSaveAndExplicitAdoptionUsesExactSuccessfulReply() async throws {
        let host = RequirementPlanHostStub(missingPlan: true)
        let client = NativeLocalAgentRequirementPlanClient(host: host)
        let legacy = try await client.get(ownerUserID: "user-1", survey: survey())
        XCTAssertNil(legacy)
        let adopted = try await client.saveSourceReply(ownerUserID: "user-1", survey: survey(), markdown: "Fictional proposal")
        XCTAssertEqual(adopted.revision, 1)
        XCTAssertNil(adopted.confirmedAtUnixMs)
        let recorded = try await host.recorded()
        let commands = try JSONSerialization.jsonObject(with: Data(recorded.utf8)) as! [[String: Any]]
        XCTAssertEqual(commands.map { $0["type"] as? String }, ["get_requirement_plan", "get_run", "save_requirement_plan"])
        XCTAssertEqual(commands.last?["expected_revision"] as? Int, 0)
        XCTAssertEqual(commands.last?["source_run_id"] as? String, "run-1")
    }

    func testFailedGenerationCannotBeAdoptedOrShownAsComplete() async throws {
        let host = RequirementPlanHostStub(runStatus: "failed")
        let client = NativeLocalAgentRequirementPlanClient(host: host)
        do { _ = try await client.saveSourceReply(ownerUserID: "user-1", survey: survey(), markdown: "Fictional proposal"); XCTFail("failed generation") }
        catch { XCTAssertTrue(error is NativeLocalAgentHostError) }
        let fetched = try await client.get(ownerUserID: "user-1", survey: survey())
        let plan = try XCTUnwrap(fetched)
        let followUp = try await client.sourceFollowUp(ownerUserID: "user-1", plan: plan)
        XCTAssertEqual(followUp, .failed)
        let recorded = try await host.recorded()
        let commands = try JSONSerialization.jsonObject(with: Data(recorded.utf8)) as! [[String: Any]]
        XCTAssertFalse(commands.contains { $0["type"] as? String == "save_requirement_plan" })
    }
}

private actor RequirementPlanHostStub: LocalAgentHostClientServicing {
    private var commands: [[String: Any]] = []
    private let corruptConfirmation: String?
    private let missingPlan: Bool
    private let runStatus: String
    init(corruptConfirmation: String? = nil, missingPlan: Bool = false, runStatus: String = "succeeded") {
        self.corruptConfirmation = corruptConfirmation; self.missingPlan = missingPlan; self.runStatus = runStatus
    }
    func start(ownerUserID: String) async throws {}
    func stop() async {}
    func recorded() throws -> String { String(decoding: try JSONSerialization.data(withJSONObject: commands), as: UTF8.self) }
    func request(command: Data) async throws -> Data {
        let object = try JSONSerialization.jsonObject(with: command) as! [String: Any]
        commands.append(object)
        let type = object["type"] as? String
        if type == "get_run" {
            return try JSONSerialization.data(withJSONObject: ["type": "run", "run": [
                "run_id": "run-1", "owner_user_id": "user-1", "owner_entity_type": "conversation_turn",
                "owner_entity_id": "turn-1", "profile_key": "main_chat", "input": ["conversation_id": "chat-1"],
                "status": runStatus, "version": 4, "terminal_outcome": ["content": "Fictional proposal"],
                "created_at_unix_ms": 1, "updated_at_unix_ms": 4,
            ]])
        }
        if type == "get_requirement_plan", missingPlan {
            return try JSONSerialization.data(withJSONObject: ["type": "requirement_plan", "plan": NSNull()])
        }
        var plan: [String: Any] = [
            "plan_id": "requirement-plan:survey-1", "revision": 1, "owner_user_id": "user-1", "project_resource_id": "project-1",
            "source_conversation_id": "chat-1", "source_survey_id": "survey-1", "source_run_id": "run-1",
            "title": "Fictional finance", "markdown": "Fictional proposal", "created_at_unix_ms": 2,
        ]
        if type == "confirm_requirement_plan" {
            plan["confirmed_at_unix_ms"] = 3
            if let field = corruptConfirmation { plan[field] = field == "revision" ? 2 : "wrong" }
        }
        return try JSONSerialization.data(withJSONObject: ["type": "requirement_plan", "plan": plan])
    }
}
