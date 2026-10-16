import ChatOSCore
import Foundation

public struct LocalAgentHostRequirementPlan: Codable, Sendable, Equatable {
    public let planID: String
    public let revision: UInt64
    public let ownerUserID: String
    public let projectResourceID: String
    public let sourceConversationID: String
    public let sourceSurveyID: String
    public let sourceRunID: String
    public let title: String
    public let markdown: String
    public let createdAtUnixMs: Int64
    public let confirmedAtUnixMs: Int64?

    private enum CodingKeys: String, CodingKey {
        case revision, title, markdown
        case planID = "plan_id", ownerUserID = "owner_user_id", projectResourceID = "project_resource_id"
        case sourceConversationID = "source_conversation_id", sourceSurveyID = "source_survey_id", sourceRunID = "source_run_id"
        case createdAtUnixMs = "created_at_unix_ms", confirmedAtUnixMs = "confirmed_at_unix_ms"
    }
}

public struct NativeLocalAgentRequirementPlanClient: Sendable {
    private let host: any LocalAgentHostClientServicing
    public init(host: any LocalAgentHostClientServicing) { self.host = host }

    public func get(ownerUserID: String, survey: LocalAgentHostRequirementSurvey, revision: UInt64? = nil) async throws -> LocalAgentHostRequirementPlan? {
        guard survey.ownerUserID == ownerUserID else { throw NativeLocalAgentHostError.invalidResponse }
        let result: PlanResult = try await host.request(PlanCommand(
            type: "get_requirement_plan", ownerUserID: ownerUserID, surveyID: survey.id, revision: revision
        ))
        guard result.type == "requirement_plan" else { throw NativeLocalAgentHostError.invalidResponse }
        if let plan = result.plan {
            try validate(plan, ownerUserID: ownerUserID, survey: survey)
            guard revision == nil || revision == plan.revision else { throw NativeLocalAgentHostError.invalidResponse }
        }
        return result.plan
    }

    /// Explicitly adopt an older successful survey reply. Reading never creates
    /// a proposal, and importing a proposal is not confirmation or execution.
    public func saveSourceReply(ownerUserID: String, survey: LocalAgentHostRequirementSurvey, markdown: String) async throws -> LocalAgentHostRequirementPlan {
        guard survey.ownerUserID == ownerUserID, survey.status == .resolved else { throw NativeLocalAgentHostError.invalidResponse }
        let reply = try await NativeLocalAgentRequirementSurveyClient(host: host).followUp(ownerUserID: ownerUserID, survey: survey)
        guard reply == .completed(markdown) else { throw NativeLocalAgentHostError.invalidResponse }
        let result: PlanResult = try await host.request(PlanCommand(
            type: "save_requirement_plan", ownerUserID: ownerUserID, surveyID: survey.id,
            sourceRunID: survey.sourceRunID, expectedRevision: 0, markdown: markdown
        ))
        guard result.type == "requirement_plan", let plan = result.plan else { throw NativeLocalAgentHostError.invalidResponse }
        try validate(plan, ownerUserID: ownerUserID, survey: survey)
        guard plan.revision == 1, plan.sourceRunID == survey.sourceRunID,
              plan.sourceConversationID == survey.sourceConversationID,
              plan.markdown == markdown.trimmingCharacters(in: .whitespacesAndNewlines), plan.confirmedAtUnixMs == nil else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return plan
    }

    /// Human-only UI command. No corresponding model tool is advertised.
    public func confirm(ownerUserID: String, survey: LocalAgentHostRequirementSurvey, plan: LocalAgentHostRequirementPlan) async throws -> LocalAgentHostRequirementPlan {
        try validate(plan, ownerUserID: ownerUserID, survey: survey)
        let result: PlanResult = try await host.request(PlanCommand(
            type: "confirm_requirement_plan", ownerUserID: ownerUserID, surveyID: survey.id, expectedRevision: plan.revision
        ))
        guard result.type == "requirement_plan", let confirmed = result.plan else { throw NativeLocalAgentHostError.invalidResponse }
        try validate(confirmed, ownerUserID: ownerUserID, survey: survey)
        guard confirmed.planID == plan.planID, confirmed.revision == plan.revision,
              confirmed.markdown == plan.markdown, confirmed.title == plan.title,
              confirmed.sourceRunID == plan.sourceRunID, confirmed.sourceConversationID == plan.sourceConversationID,
              confirmed.createdAtUnixMs == plan.createdAtUnixMs,
              confirmed.confirmedAtUnixMs != nil else { throw NativeLocalAgentHostError.invalidResponse }
        return confirmed
    }

    public func sourceFollowUp(ownerUserID: String, plan: LocalAgentHostRequirementPlan) async throws -> RequirementSurveyFollowUp {
        guard plan.ownerUserID == ownerUserID else { throw NativeLocalAgentHostError.invalidResponse }
        let run = try await NativeLocalAgentRuntimeClient(host: host).run(ownerUserID: ownerUserID, runID: plan.sourceRunID)
        guard run.ownerUserID == ownerUserID, run.runID == plan.sourceRunID,
              case let .object(input) = run.input,
              (run.profileKey == "main_chat" && run.ownerEntityType == "conversation_turn"
                && input["conversation_id"] == .string(plan.sourceConversationID))
                || (run.profileKey == "task_execution" && run.ownerEntityType == "task"
                    && input["source_conversation_id"] == .string(plan.sourceConversationID)) else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return RequirementSurveyFollowUp.project(run)
    }

    private func validate(_ plan: LocalAgentHostRequirementPlan, ownerUserID: String, survey: LocalAgentHostRequirementSurvey) throws {
        guard survey.ownerUserID == ownerUserID, plan.ownerUserID == ownerUserID,
              plan.projectResourceID == survey.projectResourceID, plan.sourceSurveyID == survey.id,
              plan.planID == "requirement-plan:\(survey.id)", plan.revision > 0,
              !plan.markdown.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            throw NativeLocalAgentHostError.invalidResponse
        }
    }
}

private struct PlanCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let surveyID: String
    var revision: UInt64? = nil
    var sourceRunID: String? = nil
    var expectedRevision: UInt64? = nil
    var markdown: String? = nil
    private enum CodingKeys: String, CodingKey {
        case type, revision, markdown
        case ownerUserID = "owner_user_id", surveyID = "survey_id", sourceRunID = "source_run_id", expectedRevision = "expected_revision"
    }
}

private struct PlanResult: Decodable, Sendable {
    let type: String
    let plan: LocalAgentHostRequirementPlan?
}
