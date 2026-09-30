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
        guard result.type == "requirement_survey" else {
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
