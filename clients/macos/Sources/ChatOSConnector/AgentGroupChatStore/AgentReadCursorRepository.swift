import ChatOSCore

enum AgentReadCursorRepository {
    static func find(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        roomID: String,
        agentID: String,
        preparedStatement: () -> Void
    ) throws -> ProjectAgentReadCursor? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT owner_user_id, room_id, agent_id, message_id,
                   message_created_at_unix_ms, updated_at_unix_ms
            FROM project_agent_read_cursors
            WHERE owner_user_id = ? AND room_id = ? AND agent_id = ? LIMIT 1
            """,
            [.text(ownerUserID), .text(roomID), .text(agentID)],
            row: AgentGroupChatRowMapper.readCursor
        ).first
    }

    static func upsert(
        _ handle: OpaquePointer?,
        cursors: [ProjectAgentReadCursor],
        preparedStatement: () -> Void
    ) throws {
        guard !cursors.isEmpty else { return }
        let rowPlaceholders = Array(repeating: "(?, ?, ?, ?, ?, ?)", count: cursors.count)
            .joined(separator: ",")
        let values = cursors.flatMap { cursor in
            [
                AgentGroupChatDatabase.Value.text(cursor.ownerUserID),
                .text(cursor.roomID),
                .text(cursor.agentID),
                .text(cursor.messageID),
                .integer(cursor.messageCreatedAtUnixMs),
                .integer(cursor.updatedAtUnixMs),
            ]
        }
        preparedStatement()
        try AgentGroupChatDatabase.execute(
            handle,
            """
            INSERT INTO project_agent_read_cursors (
                owner_user_id, room_id, agent_id, message_id,
                message_created_at_unix_ms, updated_at_unix_ms
            ) VALUES \(rowPlaceholders)
            ON CONFLICT(owner_user_id, room_id, agent_id) DO UPDATE SET
                message_id = excluded.message_id,
                message_created_at_unix_ms = excluded.message_created_at_unix_ms,
                updated_at_unix_ms = excluded.updated_at_unix_ms
            """,
            values
        )
    }
}
