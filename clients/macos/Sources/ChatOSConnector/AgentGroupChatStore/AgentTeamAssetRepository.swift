import ChatOSCore
import SQLite3

enum AgentTeamAssetRepository {
    static func list(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        teamRoomID: String,
        includeArchived: Bool,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTeamAsset] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(assetColumns) FROM local_agent_team_assets WHERE owner_user_id = ? AND team_room_id = ?"
                + (includeArchived ? "" : " AND status = 'active'")
                + " ORDER BY category, updated_at_unix_ms DESC, id",
            [.text(ownerUserID), .text(teamRoomID)],
            row: AgentGroupChatRowMapper.teamAsset
        )
    }

    static func find(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        teamRoomID: String,
        assetID: String,
        preparedStatement: () -> Void
    ) throws -> LocalAgentTeamAsset? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            "SELECT \(assetColumns) FROM local_agent_team_assets WHERE owner_user_id = ? AND team_room_id = ? AND id = ? LIMIT 1",
            [.text(ownerUserID), .text(teamRoomID), .text(assetID)],
            row: AgentGroupChatRowMapper.teamAsset
        ).first
    }

    static func listRevisions(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        assetID: String,
        limit: Int,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTeamAssetRevision] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT asset_id, revision, title, markdown, editor_agent_id, created_at_unix_ms
            FROM local_agent_team_asset_revisions
            WHERE owner_user_id = ? AND asset_id = ?
            ORDER BY revision DESC LIMIT ?
            """,
            [.text(ownerUserID), .text(assetID), .integer(Int64(limit))],
            row: AgentGroupChatRowMapper.teamAssetRevision
        )
    }

    static func listTodoSnapshots(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoID: String,
        preparedStatement: () -> Void
    ) throws -> [LocalAgentTodoTeamAssetSnapshot] {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT todo_id, asset_id, team_room_id, category, title, markdown,
                   revision, captured_at_unix_ms
            FROM local_agent_todo_asset_snapshots
            WHERE owner_user_id = ? AND todo_id = ?
            ORDER BY category, asset_id
            """,
            [.text(ownerUserID), .text(todoID)],
            row: AgentGroupChatRowMapper.todoTeamAssetSnapshot
        )
    }

    static func todoSnapshot(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        todoID: String,
        assetID: String,
        revision: Int,
        preparedStatement: () -> Void
    ) throws -> LocalAgentTodoTeamAssetSnapshot? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT todo_id, asset_id, team_room_id, category, title, markdown,
                   revision, captured_at_unix_ms
            FROM local_agent_todo_asset_snapshots
            WHERE owner_user_id = ? AND todo_id = ? AND asset_id = ? AND revision = ?
            LIMIT 1
            """,
            [.text(ownerUserID), .text(todoID), .text(assetID), .integer(Int64(revision))],
            row: AgentGroupChatRowMapper.todoTeamAssetSnapshot
        ).first
    }

    private static let assetColumns = "owner_user_id, id, team_room_id, category, title, markdown, revision, status, created_by_agent_id, updated_by_agent_id, created_at_unix_ms, updated_at_unix_ms"
}
