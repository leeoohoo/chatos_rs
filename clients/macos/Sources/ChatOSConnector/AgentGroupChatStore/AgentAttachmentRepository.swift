import SQLite3

struct AgentArtifactUploadCandidate {
    let id: String
    let roomID: String
    let name: String
    let mimeType: String
    let size: Int
    let sha256: String
    let relativePath: String
    let attempt: Int
}

enum AgentAttachmentRepository {
    static func nextUploadCandidate(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        nowUnixMs: Int64,
        preparedStatement: () -> Void
    ) throws -> AgentArtifactUploadCandidate? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT a.id, m.room_id, a.name, a.mime_type, a.size_bytes, a.sha256,
                   a.relative_path, a.upload_attempt
            FROM project_agent_message_attachments a
            JOIN project_agent_messages m
              ON m.owner_user_id = a.owner_user_id AND m.id = a.message_id
            WHERE a.owner_user_id = ? AND m.sender_kind = 'agent'
              AND a.sync_status IN ('queued', 'failed', 'uploading')
              AND a.next_retry_at_unix_ms <= ? AND a.sha256 IS NOT NULL
            ORDER BY a.next_retry_at_unix_ms, a.id
            LIMIT 1
            """,
            [.text(ownerUserID), .integer(nowUnixMs)]
        ) { statement in
            AgentArtifactUploadCandidate(
                id: string(statement, 0),
                roomID: string(statement, 1),
                name: string(statement, 2),
                mimeType: string(statement, 3),
                size: Int(sqlite3_column_int64(statement, 4)),
                sha256: string(statement, 5),
                relativePath: string(statement, 6),
                attempt: Int(sqlite3_column_int64(statement, 7)) + 1
            )
        }.first
    }

    private static func string(_ statement: OpaquePointer, _ index: Int32) -> String {
        guard let value = sqlite3_column_text(statement, index) else { return "" }
        return String(cString: value)
    }
}
