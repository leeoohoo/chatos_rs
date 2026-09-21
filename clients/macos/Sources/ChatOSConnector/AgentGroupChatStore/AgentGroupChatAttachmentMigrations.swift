import ChatOSCore
import SQLite3

extension AgentGroupChatMigrations {
    static func migrateAttachmentSyncAndMetrics(_ handle: OpaquePointer) throws {
        func hasColumn(_ name: String, table: String) -> Bool {
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(handle, "PRAGMA table_info(\(table))", -1, &statement, nil) == SQLITE_OK,
                  let statement else { return false }
            defer { sqlite3_finalize(statement) }
            while sqlite3_step(statement) == SQLITE_ROW {
                guard let value = sqlite3_column_text(statement, 1) else { continue }
                if String(cString: value) == name { return true }
            }
            return false
        }
        func execute(_ sql: String) throws {
            guard sqlite3_exec(handle, sql, nil, nil, nil) == SQLITE_OK else {
                throw AgentGroupChatError.storage(String(cString: sqlite3_errmsg(handle)))
            }
        }
        func hasMigration(_ version: Int) -> Bool {
            var statement: OpaquePointer?
            guard sqlite3_prepare_v2(
                handle,
                "SELECT 1 FROM local_agent_group_chat_schema_migrations WHERE version = ? LIMIT 1",
                -1,
                &statement,
                nil
            ) == SQLITE_OK, let statement else { return false }
            defer { sqlite3_finalize(statement) }
            guard sqlite3_bind_int64(statement, 1, Int64(version)) == SQLITE_OK else {
                return false
            }
            return sqlite3_step(statement) == SQLITE_ROW
        }

        if !hasColumn("sha256", table: "project_agent_message_attachments") {
            try execute("ALTER TABLE project_agent_message_attachments ADD COLUMN sha256 TEXT")
        }
        if !hasColumn("sync_status", table: "project_agent_message_attachments") {
            try execute(
                "ALTER TABLE project_agent_message_attachments ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'local_only' CHECK(sync_status IN ('local_only', 'queued', 'uploading', 'synced', 'failed'))"
            )
        }
        for column in [
            "artifact_id", "storage_provider", "bucket", "object_key", "remote_view_path",
            "upload_error",
        ] where !hasColumn(column, table: "project_agent_message_attachments") {
            try execute("ALTER TABLE project_agent_message_attachments ADD COLUMN \(column) TEXT")
        }
        if !hasColumn("synced_at_unix_ms", table: "project_agent_message_attachments") {
            try execute(
                "ALTER TABLE project_agent_message_attachments ADD COLUMN synced_at_unix_ms INTEGER"
            )
        }
        if !hasColumn("upload_attempt", table: "project_agent_message_attachments") {
            try execute(
                "ALTER TABLE project_agent_message_attachments ADD COLUMN upload_attempt INTEGER NOT NULL DEFAULT 0 CHECK(upload_attempt >= 0)"
            )
        }
        if !hasColumn("next_retry_at_unix_ms", table: "project_agent_message_attachments") {
            try execute(
                "ALTER TABLE project_agent_message_attachments ADD COLUMN next_retry_at_unix_ms INTEGER NOT NULL DEFAULT 0 CHECK(next_retry_at_unix_ms >= 0)"
            )
        }
        try execute(
            """
            CREATE INDEX IF NOT EXISTS project_agent_attachment_sync_outbox
            ON project_agent_message_attachments(
                owner_user_id, sync_status, next_retry_at_unix_ms, id
            )
            """
        )
        if !hasMigration(23) {
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (23)"
            )
        }
        if !hasMigration(24) {
            try execute(
                """
                CREATE TABLE IF NOT EXISTS local_agent_communication_metrics (
                    owner_user_id TEXT NOT NULL,
                    metric_name TEXT NOT NULL,
                    dimension TEXT NOT NULL,
                    event_count INTEGER NOT NULL CHECK(event_count > 0),
                    total_value INTEGER NOT NULL CHECK(total_value >= 0),
                    maximum_value INTEGER NOT NULL CHECK(maximum_value >= 0),
                    updated_at_unix_ms INTEGER NOT NULL CHECK(updated_at_unix_ms >= 0),
                    PRIMARY KEY(owner_user_id, metric_name, dimension)
                )
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (24)"
            )
        }
        if !hasMigration(25) {
            try execute(
                """
                CREATE TABLE IF NOT EXISTS local_agent_message_sequence_events (
                    owner_user_id TEXT NOT NULL,
                    run_fingerprint TEXT NOT NULL,
                    sequence INTEGER NOT NULL CHECK(sequence > 0),
                    character_count INTEGER NOT NULL CHECK(character_count >= 0),
                    document_count INTEGER NOT NULL CHECK(document_count >= 0),
                    created_at_unix_ms INTEGER NOT NULL CHECK(created_at_unix_ms >= 0),
                    PRIMARY KEY(owner_user_id, run_fingerprint, sequence)
                )
                """
            )
            try execute(
                """
                CREATE INDEX IF NOT EXISTS local_agent_message_sequence_events_window
                ON local_agent_message_sequence_events(
                    owner_user_id, run_fingerprint, created_at_unix_ms, sequence
                )
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (25)"
            )
        }
        if !hasColumn("avatar_data", table: "local_agent_profiles") {
            try execute("ALTER TABLE local_agent_profiles ADD COLUMN avatar_data BLOB")
        }
        if !hasMigration(26) {
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (26)"
            )
        }
    }
}
