import ChatOSCore
import Foundation
import SQLite3

enum AgentTeamAssetReviewRepository {
    static let columns = """
        id, team_room_id, todo_id, todo_event_id, suggestion_index, source_agent_id,
        category, title, markdown, rationale, status, resolved_asset_id,
        resolved_asset_revision, reviewed_by_agent_id, review_reason,
        created_at_unix_ms, updated_at_unix_ms
        """

    static func insert(
        ownerUserID: String,
        reviews: [LocalAgentTeamAssetReview],
        execute: (String, [AgentGroupChatDatabase.Value]) throws -> Void
    ) throws {
        for review in reviews {
            try review.validate()
            guard review.ownerUserID == ownerUserID else {
                throw AgentGroupChatError.invalidField("ownerUserID")
            }
            try execute(
                """
                INSERT INTO local_agent_team_asset_reviews (
                    owner_user_id, id, team_room_id, todo_id, todo_event_id,
                    suggestion_index, source_agent_id, category, title, markdown,
                    rationale, status, resolved_asset_id, resolved_asset_revision,
                    reviewed_by_agent_id, review_reason, created_at_unix_ms, updated_at_unix_ms
                )
                SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?,
                       'pending', NULL, NULL, NULL, '', ?, ?
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM local_agent_team_asset_reviews
                    WHERE owner_user_id = ? AND todo_id = ? AND category = ?
                      AND title = ? AND markdown = ? AND rationale = ?
                )
                """,
                [
                    .text(ownerUserID), .text(review.id), .text(review.teamRoomID),
                    .text(review.todoID), .text(review.todoEventID),
                    .integer(Int64(review.suggestionIndex)), .text(review.sourceAgentID),
                    .text(review.suggestion.category.rawValue), .text(review.suggestion.title),
                    .text(review.suggestion.markdown), .text(review.suggestion.rationale),
                    .integer(review.createdAtUnixMs), .integer(review.updatedAtUnixMs),
                    .text(ownerUserID), .text(review.todoID),
                    .text(review.suggestion.category.rawValue),
                    .text(review.suggestion.title), .text(review.suggestion.markdown),
                    .text(review.suggestion.rationale),
                ]
            )
        }
    }

    static func find(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        reviewID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentTeamAssetReview? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(columns) FROM local_agent_team_asset_reviews WHERE owner_user_id = ? AND id = ?",
            [.text(ownerUserID), .text(reviewID)],
            row: { try row($0, ownerUserID: ownerUserID) }
        ).first
    }

    static func list(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        teamRoomID: String?,
        reviewerAgentID: String?,
        status: LocalAgentTeamAssetReviewStatus?,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTeamAssetReview] {
        var clauses = ["review.owner_user_id = ?"]
        var values: [AgentGroupChatDatabase.Value] = [.text(ownerUserID)]
        var joins = ""
        if let teamRoomID {
            clauses.append("review.team_room_id = ?")
            values.append(.text(teamRoomID))
        }
        if let reviewerAgentID {
            joins = """
                 JOIN project_agent_rooms room
                   ON room.owner_user_id = review.owner_user_id
                  AND room.id = review.team_room_id
                """
            clauses.append("room.project_manager_agent_id = ?")
            clauses.append("room.status = 'active'")
            values.append(.text(reviewerAgentID))
        }
        if let status {
            clauses.append("review.status = ?")
            values.append(.text(status.rawValue))
        }
        values.append(.integer(Int64(limit)))
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT \(columns.split(separator: ",").map { "review.\($0.trimmingCharacters(in: .whitespacesAndNewlines))" }.joined(separator: ", "))
            FROM local_agent_team_asset_reviews review
            \(joins)
            WHERE \(clauses.joined(separator: " AND "))
            ORDER BY review.created_at_unix_ms, review.id
            LIMIT ?
            """,
            values,
            row: { try row($0, ownerUserID: ownerUserID) }
        )
    }

    private static func row(
        _ statement: OpaquePointer?,
        ownerUserID: String
    ) throws -> LocalAgentTeamAssetReview {
        guard let category = LocalAgentTeamAssetCategory(rawValue: string(statement, 6)),
              let status = LocalAgentTeamAssetReviewStatus(rawValue: string(statement, 10)) else {
            throw AgentGroupChatError.storage("invalid team asset review")
        }
        let review = LocalAgentTeamAssetReview(
            id: string(statement, 0),
            ownerUserID: ownerUserID,
            teamRoomID: string(statement, 1),
            todoID: string(statement, 2),
            todoEventID: string(statement, 3),
            suggestionIndex: Int(sqlite3_column_int64(statement, 4)),
            sourceAgentID: string(statement, 5),
            suggestion: .init(
                category: category,
                title: string(statement, 7),
                markdown: string(statement, 8),
                rationale: string(statement, 9)
            ),
            status: status,
            resolvedAssetID: optionalString(statement, 11),
            resolvedAssetRevision: optionalInteger(statement, 12).map(Int.init),
            reviewedByAgentID: optionalString(statement, 13),
            reviewReason: string(statement, 14),
            createdAtUnixMs: sqlite3_column_int64(statement, 15),
            updatedAtUnixMs: sqlite3_column_int64(statement, 16)
        )
        try review.validate()
        return review
    }

    private static func string(_ statement: OpaquePointer?, _ index: Int32) -> String {
        guard let pointer = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: pointer)
    }

    private static func optionalString(_ statement: OpaquePointer?, _ index: Int32) -> String? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL else { return nil }
        return string(statement, index)
    }

    private static func optionalInteger(_ statement: OpaquePointer?, _ index: Int32) -> Int64? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL else { return nil }
        return sqlite3_column_int64(statement, index)
    }
}
