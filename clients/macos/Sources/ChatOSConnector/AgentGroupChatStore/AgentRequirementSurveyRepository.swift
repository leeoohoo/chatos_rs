import ChatOSCore
import Foundation
import SQLite3

enum AgentRequirementSurveyRepository {
    static func list(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        projectID: String,
        status: LocalAgentRequirementSurveyStatus?,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentRequirementSurvey] {
        preparedStatement()
        let statusClause = status == nil ? "" : " AND status = ?"
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID), .text(projectID)]
        if let status { values.append(.text(status.rawValue)) }
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_requirement_surveys WHERE owner_user_id = ? AND project_id = ?\(statusClause) ORDER BY created_at_unix_ms DESC, id DESC",
            values,
            row: map
        )
    }

    static func find(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        projectID: String,
        surveyID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentRequirementSurvey? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_requirement_surveys WHERE owner_user_id = ? AND project_id = ? AND id = ? LIMIT 1",
            [.text(ownerUserID), .text(projectID), .text(surveyID)],
            row: map
        ).first
    }

    static func findByRequest(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        projectID: String,
        creatorAgentID: String,
        sourceDeliveryID: String,
        requestKey: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentRequirementSurvey? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_requirement_surveys WHERE owner_user_id = ? AND project_id = ? AND creator_agent_id = ? AND source_delivery_id = ? AND request_key = ? LIMIT 1",
            [
                .text(ownerUserID), .text(projectID), .text(creatorAgentID),
                .text(sourceDeliveryID), .text(requestKey),
            ],
            row: map
        ).first
    }

    private static func map(_ statement: OpaquePointer) throws -> LocalAgentRequirementSurvey {
        guard let status = LocalAgentRequirementSurveyStatus(
            rawValue: SQLiteAgentGroupChatStore.string(statement, 7)
        ) else {
            throw AgentGroupChatError.storage("invalid requirement survey status")
        }
        let decoder = JSONDecoder()
        let draft: LocalAgentRequirementSurveyDraft
        let submission: LocalAgentRequirementSurveySubmission?
        let resolution: LocalAgentRequirementSurveyResolution?
        do {
            draft = try decoder.decode(
                LocalAgentRequirementSurveyDraft.self,
                from: Data(SQLiteAgentGroupChatStore.string(statement, 6).utf8)
            )
            if let submissionJSON = SQLiteAgentGroupChatStore.optionalString(statement, 8) {
                submission = try decoder.decode(
                    LocalAgentRequirementSurveySubmission.self,
                    from: Data(submissionJSON.utf8)
                )
            } else {
                submission = nil
            }
            if let resolutionJSON = SQLiteAgentGroupChatStore.optionalString(statement, 9) {
                resolution = try decoder.decode(
                    LocalAgentRequirementSurveyResolution.self,
                    from: Data(resolutionJSON.utf8)
                )
            } else {
                resolution = nil
            }
        } catch {
            throw AgentGroupChatError.storage("invalid requirement survey data")
        }
        let survey = LocalAgentRequirementSurvey(
            id: SQLiteAgentGroupChatStore.string(statement, 1),
            ownerUserID: SQLiteAgentGroupChatStore.string(statement, 0),
            projectID: SQLiteAgentGroupChatStore.string(statement, 2),
            creatorAgentID: SQLiteAgentGroupChatStore.string(statement, 3),
            sourceDeliveryID: SQLiteAgentGroupChatStore.string(statement, 4),
            requestKey: SQLiteAgentGroupChatStore.string(statement, 5),
            draft: draft,
            status: status,
            submission: submission,
            resolution: resolution,
            createdAtUnixMs: sqlite3_column_int64(statement, 10),
            submittedAtUnixMs: SQLiteAgentGroupChatStore.optionalInt64(statement, 11),
            resolvedAtUnixMs: SQLiteAgentGroupChatStore.optionalInt64(statement, 12)
        )
        try survey.validate()
        return survey
    }

    private static let columns = "owner_user_id, id, project_id, creator_agent_id, source_delivery_id, request_key, draft_json, status, submission_json, resolution_json, created_at_unix_ms, submitted_at_unix_ms, resolved_at_unix_ms"
}
