import ChatOSCore
import Foundation

public enum LocalAgentHostRequirementSurveyStatus: String, Codable, Sendable, Equatable {
    case open
    case resolved
}

public enum LocalAgentHostRequirementSurveyResponseKind: String, Codable, Sendable, Equatable {
    case text
    case singleChoice = "single_choice"
    case multipleChoice = "multiple_choice"
    case boolean
}

public struct LocalAgentHostRequirementSurveyQuestion: Codable, Sendable, Equatable, Identifiable {
    public var id: String { questionID }
    public let questionID: String
    public let prompt: String
    public let responseKind: LocalAgentHostRequirementSurveyResponseKind
    public let required: Bool
    public let options: [String]

    private enum CodingKeys: String, CodingKey {
        case prompt, required, options
        case questionID = "question_id"
        case responseKind = "response_kind"
    }
}

public struct LocalAgentHostRequirementSurvey: Codable, Sendable, Equatable, Identifiable {
    public var id: String { surveyID }
    public let surveyID: String
    public let ownerUserID: String
    public let projectResourceID: String
    public let sourceConversationID: String
    public let sourceRunID: String
    public let sourceTaskID: String?
    public let title: String
    public let description: String?
    public let questions: [LocalAgentHostRequirementSurveyQuestion]
    public let answers: [String: LocalAgentJSONValue]?
    public let status: LocalAgentHostRequirementSurveyStatus
    public let version: UInt64
    public let createdAtUnixMs: Int64
    public let updatedAtUnixMs: Int64
    public let resolvedAtUnixMs: Int64?

    private enum CodingKeys: String, CodingKey {
        case title, description, questions, answers, status, version
        case surveyID = "survey_id"
        case ownerUserID = "owner_user_id"
        case projectResourceID = "project_resource_id"
        case sourceConversationID = "source_conversation_id"
        case sourceRunID = "source_run_id"
        case sourceTaskID = "source_task_id"
        case createdAtUnixMs = "created_at_unix_ms"
        case updatedAtUnixMs = "updated_at_unix_ms"
        case resolvedAtUnixMs = "resolved_at_unix_ms"
    }
}

public struct LocalAgentHostRequirementSurveyResolution: Decodable, Sendable, Equatable {
    public let survey: LocalAgentHostRequirementSurvey
    public let resumedRun: LocalAgentRunRecord

    private enum CodingKeys: String, CodingKey {
        case survey
        case resumedRun = "resumed_run"
    }
}

public struct NativeLocalAgentRequirementSurveyClient: Sendable {
    private let host: any LocalAgentHostClientServicing

    public init(host: any LocalAgentHostClientServicing) {
        self.host = host
    }

    public var planClient: NativeLocalAgentRequirementPlanClient { .init(host: host) }

    /// A submitted form is not evidence that its follow-up has finished. Read the
    /// exact source Run, and expose only the final user-facing reply, never traces.
    public func followUp(
        ownerUserID: String,
        survey: LocalAgentHostRequirementSurvey
    ) async throws -> RequirementSurveyFollowUp {
        guard survey.ownerUserID == ownerUserID else { throw NativeLocalAgentHostError.invalidResponse }
        guard survey.status == .resolved else { return .awaitingAnswers }
        let run = try await NativeLocalAgentRuntimeClient(host: host).run(
            ownerUserID: ownerUserID, runID: survey.sourceRunID
        )
        guard run.ownerUserID == ownerUserID, run.runID == survey.sourceRunID,
              case let .object(input) = run.input,
              input[survey.sourceTaskID == nil ? "conversation_id" : "source_conversation_id"]
                == .string(survey.sourceConversationID),
              survey.sourceTaskID == nil
                ? (run.profileKey == "main_chat" && run.ownerEntityType == "conversation_turn")
                : (run.profileKey == "task_execution" && run.ownerEntityType == "task"
                    && run.ownerEntityID == survey.sourceTaskID)
        else { throw NativeLocalAgentHostError.invalidResponse }
        return RequirementSurveyFollowUp.project(run)
    }

    public func list(
        ownerUserID: String,
        projectResourceID: String? = nil,
        status: LocalAgentHostRequirementSurveyStatus? = nil,
        limit: UInt32 = 200
    ) async throws -> [LocalAgentHostRequirementSurvey] {
        let result: SurveysResult = try await host.request(ListCommand(
            type: "list_requirement_surveys",
            ownerUserID: ownerUserID,
            projectResourceID: projectResourceID,
            status: status,
            limit: limit
        ))
        guard result.type == "requirement_surveys" else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return result.surveys
    }

    public func get(
        ownerUserID: String,
        surveyID: String
    ) async throws -> LocalAgentHostRequirementSurvey {
        let result: SurveyResult = try await host.request(IdentityCommand(
            type: "get_requirement_survey",
            ownerUserID: ownerUserID,
            surveyID: surveyID
        ))
        guard result.type == "requirement_survey", result.survey.id == surveyID,
              result.survey.ownerUserID == ownerUserID else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return result.survey
    }

    public func resolve(
        ownerUserID: String,
        surveyID: String,
        expectedVersion: UInt64,
        answers: [String: LocalAgentJSONValue]
    ) async throws -> LocalAgentHostRequirementSurveyResolution {
        let result: ResolutionResult = try await host.request(ResolveCommand(
            type: "resolve_requirement_survey",
            ownerUserID: ownerUserID,
            surveyID: surveyID,
            expectedVersion: expectedVersion,
            answers: answers
        ))
        guard result.type == "requirement_survey_resolved",
              result.resolution.survey.surveyID == surveyID else {
            throw NativeLocalAgentHostError.invalidResponse
        }
        return result.resolution
    }
}

public enum RequirementSurveyFollowUp: Equatable, Sendable {
    case awaitingAnswers
    case processing
    case completed(String)
    case completedWithoutReply
    case waitingForInput
    case needsReview
    case failed
    case cancelled

    static func project(_ run: LocalAgentRunRecord) -> Self {
        switch run.status {
        case "succeeded":
            guard case let .object(outcome)? = run.terminalOutcome else { return .completedWithoutReply }
            for key in ["content", "answer", "text", "output"] {
                if case let .string(text)? = outcome[key],
                   !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    return .completed(text)
                }
            }
            return .completedWithoutReply
        case "failed": return .failed
        case "cancelled": return .cancelled
        case "waiting_user": return .waitingForInput
        case "paused", "needs_review": return .needsReview
        default: return .processing
        }
    }
}

private struct ListCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let projectResourceID: String?
    let status: LocalAgentHostRequirementSurveyStatus?
    let limit: UInt32

    enum CodingKeys: String, CodingKey {
        case type, status, limit
        case ownerUserID = "owner_user_id"
        case projectResourceID = "project_resource_id"
    }
}

private struct IdentityCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let surveyID: String

    enum CodingKeys: String, CodingKey {
        case type
        case ownerUserID = "owner_user_id"
        case surveyID = "survey_id"
    }
}

private struct ResolveCommand: Encodable, Sendable {
    let type: String
    let ownerUserID: String
    let surveyID: String
    let expectedVersion: UInt64
    let answers: [String: LocalAgentJSONValue]

    enum CodingKeys: String, CodingKey {
        case type, answers
        case ownerUserID = "owner_user_id"
        case surveyID = "survey_id"
        case expectedVersion = "expected_version"
    }
}

private struct SurveysResult: Decodable, Sendable {
    let type: String
    let surveys: [LocalAgentHostRequirementSurvey]
}

private struct SurveyResult: Decodable, Sendable {
    let type: String
    let survey: LocalAgentHostRequirementSurvey
}

private struct ResolutionResult: Decodable, Sendable {
    let type: String
    let resolution: LocalAgentHostRequirementSurveyResolution
}
