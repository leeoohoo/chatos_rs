import ChatOSCore
import SQLite3

struct StoredMessageAttachment {
    let attachment: ProjectAgentMessageAttachment
    let relativePath: String
}

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
    static func messageAttachment(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        messageID: String,
        attachmentID: String,
        roomID: String,
        preparedStatement: () -> Void
    ) throws -> StoredMessageAttachment? {
        preparedStatement()
        return try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT a.id, a.name, a.mime_type, a.size_bytes, a.kind, a.origin,
                   a.relative_path, a.sha256, a.sync_status, a.artifact_id,
                   a.storage_provider, a.bucket, a.object_key, a.remote_view_path,
                   a.upload_error, a.synced_at_unix_ms
            FROM project_agent_message_attachments a
            JOIN project_agent_messages m
              ON m.owner_user_id = a.owner_user_id AND m.id = a.message_id
            WHERE a.owner_user_id = ? AND a.message_id = ? AND a.id = ? AND m.room_id = ?
            """,
            [.text(ownerUserID), .text(messageID), .text(attachmentID), .text(roomID)]
        ) { statement in
            guard let kind = ConversationAttachmentKind(rawValue: string(statement, 4)),
                  let origin = ConversationAttachmentOrigin(rawValue: string(statement, 5)) else {
                throw AgentGroupChatError.storage("invalid message attachment")
            }
            return StoredMessageAttachment(
                attachment: .init(
                    id: string(statement, 0),
                    name: string(statement, 1),
                    mimeType: string(statement, 2),
                    size: Int(sqlite3_column_int64(statement, 3)),
                    kind: kind,
                    origin: origin,
                    sha256: optionalString(statement, 7),
                    syncStatus: ProjectAgentMessageAttachmentSyncStatus(
                        rawValue: string(statement, 8)
                    ) ?? .localOnly,
                    artifactID: optionalString(statement, 9),
                    storageProvider: optionalString(statement, 10),
                    bucket: optionalString(statement, 11),
                    objectKey: optionalString(statement, 12),
                    remoteViewPath: optionalString(statement, 13),
                    uploadError: optionalString(statement, 14),
                    syncedAtUnixMs: optionalInt64(statement, 15)
                ),
                relativePath: string(statement, 6)
            )
        }.first
    }

    static func nextSyncDue(
        _ handle: OpaquePointer?,
        ownerUserID: String,
        preparedStatement: () -> Void
    ) throws -> Int64? {
        preparedStatement()
        let values: [Int64?] = try AgentGroupChatDatabase.query(
            handle,
            """
            SELECT MIN(next_retry_at_unix_ms)
            FROM project_agent_message_attachments
            WHERE owner_user_id = ? AND sync_status IN ('queued', 'failed', 'uploading')
            """,
            [.text(ownerUserID)]
        ) { statement in
            sqlite3_column_type(statement, 0) == SQLITE_NULL
                ? nil
                : sqlite3_column_int64(statement, 0)
        }
        return values.first ?? nil
    }

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

    private static func optionalString(_ statement: OpaquePointer, _ index: Int32) -> String? {
        guard sqlite3_column_type(statement, index) != SQLITE_NULL,
              let value = sqlite3_column_text(statement, index) else { return nil }
        return String(cString: value)
    }

    private static func optionalInt64(_ statement: OpaquePointer, _ index: Int32) -> Int64? {
        sqlite3_column_type(statement, index) == SQLITE_NULL
            ? nil
            : sqlite3_column_int64(statement, index)
    }
}
