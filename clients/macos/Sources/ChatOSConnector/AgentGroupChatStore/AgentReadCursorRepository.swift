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
}
