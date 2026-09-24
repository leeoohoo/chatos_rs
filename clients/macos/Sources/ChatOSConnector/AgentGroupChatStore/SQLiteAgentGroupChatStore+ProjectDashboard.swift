import ChatOSCore
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func projectDashboard(
        ownerUserID: String,
        teamRoomID: String
    ) throws -> LocalAgentProjectDashboard? {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        return try query(
            """
            SELECT dashboard_json, revision, updated_by_agent_id, updated_at_unix_ms
            FROM local_agent_project_dashboards
            WHERE owner_user_id = ? AND team_room_id = ?
            LIMIT 1
            """,
            [.text(ownerUserID), .text(teamRoomID)]
        ) { statement in
            let update: LocalAgentProjectDashboardUpdate
            do {
                update = try JSONDecoder().decode(
                    LocalAgentProjectDashboardUpdate.self,
                    from: Data(Self.string(statement, 0).utf8)
                )
            } catch {
                throw AgentGroupChatError.storage("invalid project dashboard")
            }
            let dashboard = LocalAgentProjectDashboard(
                ownerUserID: ownerUserID,
                teamRoomID: teamRoomID,
                phase: update.phase,
                health: update.health,
                summary: update.summary,
                nextSteps: update.nextSteps,
                milestones: update.milestones,
                issues: update.issues,
                revision: Int(sqlite3_column_int64(statement, 1)),
                updatedByAgentID: Self.string(statement, 2),
                updatedAtUnixMs: sqlite3_column_int64(statement, 3)
            )
            try dashboard.validate()
            return dashboard
        }.first
    }

    public func upsertProjectDashboard(
        ownerUserID: String,
        teamRoomID: String,
        editorAgentID: String,
        expectedRevision: Int?,
        update: LocalAgentProjectDashboardUpdate,
        nowUnixMs: Int64
    ) throws -> LocalAgentProjectDashboard {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(teamRoomID, field: "teamRoomID")
        try AgentGroupChatValidation.identifier(editorAgentID, field: "dashboardEditorAgentID")
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }

        return try transaction {
            guard let room = try readRoom(ownerUserID: ownerUserID, roomID: teamRoomID),
                  room.status == .active,
                  room.conversationKind == .projectTeam,
                  room.projectManagerAgentID == editorAgentID,
                  try readMember(
                    ownerUserID: ownerUserID,
                    roomID: teamRoomID,
                    agentID: editorAgentID
                  )?.status == .active else {
                throw AgentGroupChatError.permissionDenied
            }

            let existing = try projectDashboard(
                ownerUserID: ownerUserID,
                teamRoomID: teamRoomID
            )
            let revision: Int
            if let existing {
                guard expectedRevision == existing.revision else {
                    throw AgentGroupChatError.conflict
                }
                revision = existing.revision + 1
            } else {
                guard expectedRevision == nil else { throw AgentGroupChatError.conflict }
                revision = 1
            }

            let dashboard = LocalAgentProjectDashboard(
                ownerUserID: ownerUserID,
                teamRoomID: teamRoomID,
                phase: update.phase,
                health: update.health,
                summary: update.summary,
                nextSteps: update.nextSteps,
                milestones: update.milestones,
                issues: update.issues,
                revision: revision,
                updatedByAgentID: editorAgentID,
                updatedAtUnixMs: nowUnixMs
            )
            try dashboard.validate()
            try validateDashboardTodoLinks(
                ownerUserID: ownerUserID,
                teamRoomID: teamRoomID,
                dashboard: dashboard
            )
            let encoded = try encodeJSON(update)

            if existing == nil {
                try execute(
                    """
                    INSERT INTO local_agent_project_dashboards (
                        owner_user_id, team_room_id, dashboard_json, revision,
                        updated_by_agent_id, updated_at_unix_ms
                    ) VALUES (?, ?, ?, 1, ?, ?)
                    """,
                    [
                        .text(ownerUserID), .text(teamRoomID), .text(encoded),
                        .text(editorAgentID), .integer(nowUnixMs),
                    ]
                )
            } else {
                try execute(
                    """
                    UPDATE local_agent_project_dashboards
                    SET dashboard_json = ?, revision = ?, updated_by_agent_id = ?,
                        updated_at_unix_ms = ?
                    WHERE owner_user_id = ? AND team_room_id = ? AND revision = ?
                    """,
                    [
                        .text(encoded), .integer(Int64(revision)), .text(editorAgentID),
                        .integer(nowUnixMs), .text(ownerUserID), .text(teamRoomID),
                        .integer(Int64(revision - 1)),
                    ]
                )
                guard sqlite3_changes(database) == 1 else { throw AgentGroupChatError.conflict }
            }
            return dashboard
        }
    }

    private func validateDashboardTodoLinks(
        ownerUserID: String,
        teamRoomID: String,
        dashboard: LocalAgentProjectDashboard
    ) throws {
        let todoIDs = Set(
            dashboard.milestones.flatMap(\.linkedTodoIDs)
                + dashboard.issues.compactMap(\.relatedTodoID)
        )
        guard !todoIDs.isEmpty else { return }
        let placeholders = Array(repeating: "?", count: todoIDs.count).joined(separator: ",")
        let values: [Value] = [.text(ownerUserID), .text(teamRoomID)]
            + todoIDs.sorted().map(Value.text)
        let sql = """
            SELECT COUNT(*) FROM local_agent_todos
            WHERE owner_user_id = ? AND team_room_id = ? AND id IN (
            """ + placeholders + ")"
        let count = try scalarInt64(sql, values)
        guard count == Int64(todoIDs.count) else {
            throw AgentGroupChatError.invalidField("dashboardTodoRefs")
        }
    }
}
