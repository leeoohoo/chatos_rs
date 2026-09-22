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
        func createProjectRequirementSurveyTable(ifNotExists: Bool) throws {
            let guardClause = ifNotExists ? "IF NOT EXISTS " : ""
            try execute(
                """
                CREATE TABLE \(guardClause)local_agent_requirement_surveys (
                    owner_user_id TEXT NOT NULL,
                    id TEXT NOT NULL,
                    project_id TEXT NOT NULL,
                    creator_agent_id TEXT NOT NULL,
                    source_delivery_id TEXT NOT NULL,
                    request_key TEXT NOT NULL,
                    draft_json TEXT NOT NULL,
                    status TEXT NOT NULL CHECK(status IN ('pending', 'submitted')),
                    submission_json TEXT,
                    resolution_json TEXT,
                    created_at_unix_ms INTEGER NOT NULL,
                    submitted_at_unix_ms INTEGER,
                    resolved_at_unix_ms INTEGER,
                    PRIMARY KEY(owner_user_id, id),
                    UNIQUE(
                        owner_user_id, project_id, creator_agent_id,
                        source_delivery_id, request_key
                    ),
                    FOREIGN KEY(owner_user_id, creator_agent_id)
                        REFERENCES local_agent_profiles(owner_user_id, id),
                    FOREIGN KEY(owner_user_id, source_delivery_id)
                        REFERENCES project_agent_deliveries(owner_user_id, id)
                )
                """
            )
            try execute(
                """
                CREATE INDEX \(guardClause)local_agent_requirement_surveys_project
                ON local_agent_requirement_surveys(
                    owner_user_id, project_id, status, created_at_unix_ms, id
                )
                """
            )
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
        if !hasColumn("asset_update_suggestions_json", table: "local_agent_todo_events") {
            try execute(
                "ALTER TABLE local_agent_todo_events ADD COLUMN asset_update_suggestions_json TEXT NOT NULL DEFAULT '[]'"
            )
        }
        if !hasMigration(27) {
            let timestamp = "CAST(strftime('%s', 'now') AS INTEGER) * 1000"
            try execute(
                """
                INSERT OR IGNORE INTO project_agent_messages (
                    owner_user_id, id, room_id, sender_kind, sender_id, content,
                    reply_to_message_id, source_run_id, causation_id, root_message_id,
                    hop_count, created_at_unix_ms
                )
                SELECT room.owner_user_id,
                       'asset-maintenance-message-' || replace(room.id, '-', ''),
                       room.id, 'system', 'system',
                       '你是“' || room.name || '”的项目经理。请读取 Human 消息、团队目标、成员和 Todo 状态，主动维护真实的团队共享资产。信息充分时建立或更新“项目概览”和“当前进度”；信息不足时创建选择 requirement_survey_write 的 Todo（程序自动加入 requirement_survey_read）完成调研，不要在通讯层直接调用调研工具，也不要写空模板或臆测内容。完成本轮实际处理后再结束通讯周期。',
                       NULL, NULL, 'team-asset-maintenance:' || room.id || ':v1',
                       'asset-maintenance-message-' || replace(room.id, '-', ''),
                       0, \(timestamp)
                FROM project_agent_rooms room
                JOIN project_agent_room_members member
                  ON member.owner_user_id = room.owner_user_id
                 AND member.room_id = room.id
                 AND member.agent_id = room.project_manager_agent_id
                 AND member.status = 'active'
                WHERE room.status = 'active'
                  AND room.conversation_kind = 'project_team'
                  AND room.project_manager_agent_id IS NOT NULL
                  AND (
                    NOT EXISTS (
                        SELECT 1 FROM local_agent_team_assets asset
                        WHERE asset.owner_user_id = room.owner_user_id
                          AND asset.team_room_id = room.id
                          AND asset.status = 'active' AND asset.category = 'overview'
                    )
                    OR NOT EXISTS (
                        SELECT 1 FROM local_agent_team_assets asset
                        WHERE asset.owner_user_id = room.owner_user_id
                          AND asset.team_room_id = room.id
                          AND asset.status = 'active' AND asset.category = 'current_progress'
                    )
                  )
                """
            )
            try execute(
                """
                INSERT OR IGNORE INTO project_agent_message_mentions (
                    owner_user_id, message_id, agent_id, position
                )
                SELECT room.owner_user_id,
                       'asset-maintenance-message-' || replace(room.id, '-', ''),
                       room.project_manager_agent_id, 0
                FROM project_agent_rooms room
                JOIN project_agent_messages message
                  ON message.owner_user_id = room.owner_user_id
                 AND message.id = 'asset-maintenance-message-' || replace(room.id, '-', '')
                WHERE room.project_manager_agent_id IS NOT NULL
                """
            )
            try execute(
                """
                INSERT OR IGNORE INTO project_agent_deliveries (
                    owner_user_id, id, room_id, message_id, root_message_id,
                    target_agent_id, trigger_kind, status, attempt, hop_count,
                    deduplication_key, response_message_id, last_error,
                    claimed_at_unix_ms, completed_at_unix_ms, created_at_unix_ms
                )
                SELECT room.owner_user_id,
                       'asset-maintenance-delivery-' || replace(room.id, '-', ''),
                       room.id,
                       'asset-maintenance-message-' || replace(room.id, '-', ''),
                       'asset-maintenance-message-' || replace(room.id, '-', ''),
                       room.project_manager_agent_id, 'mention', 'pending', 0, 0,
                       'team-asset-maintenance:' || room.id || ':v1',
                       NULL, NULL, NULL, NULL, \(timestamp)
                FROM project_agent_rooms room
                JOIN project_agent_messages message
                  ON message.owner_user_id = room.owner_user_id
                 AND message.id = 'asset-maintenance-message-' || replace(room.id, '-', '')
                WHERE room.project_manager_agent_id IS NOT NULL
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (27)"
            )
        }
        if !hasMigration(28) {
            try createProjectRequirementSurveyTable(ifNotExists: true)
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (28)"
            )
        }
        if !hasColumn("resolution_json", table: "local_agent_requirement_surveys") {
            try execute(
                "ALTER TABLE local_agent_requirement_surveys ADD COLUMN resolution_json TEXT"
            )
        }
        if !hasColumn("resolved_at_unix_ms", table: "local_agent_requirement_surveys") {
            try execute(
                "ALTER TABLE local_agent_requirement_surveys ADD COLUMN resolved_at_unix_ms INTEGER"
            )
        }
        if !hasMigration(29) {
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (29)"
            )
        }
        if !hasMigration(30) {
            // Requirement surveys are project-owned. There is intentionally no legacy row
            // conversion: this feature has no production data, so the obsolete team-owned
            // table is replaced outright instead of preserving the wrong ownership model.
            try execute("DROP INDEX IF EXISTS local_agent_requirement_surveys_team")
            try execute("DROP TABLE IF EXISTS local_agent_requirement_surveys")
            try createProjectRequirementSurveyTable(ifNotExists: false)
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (30)"
            )
        }
        if !hasMigration(31) {
            // Requirement Survey capability is task-scoped. Task Runner runs do not have local
            // Agent profile or delivery rows, so provenance remains text without those FKs.
            // There was no released survey data; replace the preview table directly.
            try execute("DROP INDEX IF EXISTS local_agent_requirement_surveys_project")
            try execute("DROP TABLE IF EXISTS local_agent_requirement_surveys")
            try execute(
                """
                CREATE TABLE local_agent_requirement_surveys (
                    owner_user_id TEXT NOT NULL,
                    id TEXT NOT NULL,
                    project_id TEXT NOT NULL,
                    creator_agent_id TEXT NOT NULL CHECK(length(creator_agent_id) > 0),
                    source_delivery_id TEXT NOT NULL CHECK(length(source_delivery_id) > 0),
                    request_key TEXT NOT NULL,
                    draft_json TEXT NOT NULL,
                    status TEXT NOT NULL CHECK(status IN ('pending', 'submitted')),
                    submission_json TEXT,
                    resolution_json TEXT,
                    created_at_unix_ms INTEGER NOT NULL,
                    submitted_at_unix_ms INTEGER,
                    resolved_at_unix_ms INTEGER,
                    PRIMARY KEY(owner_user_id, id),
                    UNIQUE(
                        owner_user_id, project_id, creator_agent_id,
                        source_delivery_id, request_key
                    )
                )
                """
            )
            try execute(
                """
                CREATE INDEX local_agent_requirement_surveys_project
                ON local_agent_requirement_surveys(
                    owner_user_id, project_id, status, created_at_unix_ms, id
                )
                """
            )
            try execute(
                "INSERT INTO local_agent_group_chat_schema_migrations(version) VALUES (31)"
            )
        }
    }
}
