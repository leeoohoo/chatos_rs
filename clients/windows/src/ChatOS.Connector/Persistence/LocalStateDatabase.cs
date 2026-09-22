using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.Persistence;

public sealed class LocalStateDatabase
{
    private readonly string _connectionString;

    public LocalStateDatabase()
        : this(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ChatOS",
            "WindowsClient",
            "chatos-client.db"), pooling: true)
    {
    }

    internal LocalStateDatabase(string databasePath)
        : this(databasePath, pooling: false)
    {
    }

    private LocalStateDatabase(string databasePath, bool pooling)
    {
        var stateDirectory = Path.GetDirectoryName(databasePath)
            ?? throw new ArgumentException("Database path must include a directory.", nameof(databasePath));
        Directory.CreateDirectory(stateDirectory);
        _connectionString = new SqliteConnectionStringBuilder
        {
            DataSource = databasePath,
            Mode = SqliteOpenMode.ReadWriteCreate,
            Cache = SqliteCacheMode.Shared,
            Pooling = pooling,
        }.ToString();
    }

    public async Task InitializeAsync(CancellationToken cancellationToken = default)
    {
        await using var connection = new SqliteConnection(_connectionString);
        await connection.OpenAsync(cancellationToken).ConfigureAwait(false);

        var command = connection.CreateCommand();
        command.CommandText = """
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS local_project_records (
                owner_user_id TEXT NOT NULL, id TEXT NOT NULL, name TEXT NOT NULL,
                description TEXT NOT NULL, workspace_id TEXT NOT NULL, relative_root TEXT NOT NULL,
                revision INTEGER NOT NULL CHECK(revision > 0),
                status TEXT NOT NULL CHECK(status IN ('active', 'archived', 'removed')),
                created_at_unix_ms INTEGER NOT NULL, updated_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, id)
            );
            CREATE TABLE IF NOT EXISTS local_project_schema_migrations (version INTEGER PRIMARY KEY NOT NULL);
            INSERT OR IGNORE INTO local_project_schema_migrations(version) VALUES (1);

            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY NOT NULL,
                applied_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS ui_state (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS pet_preferences (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS conversation_cursor (
                conversation_id TEXT PRIMARY KEY NOT NULL,
                cursor TEXT,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS conversation_cache (
                conversation_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                event_sequence INTEGER,
                payload_json TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (conversation_id, message_id)
            );

            CREATE TABLE IF NOT EXISTS pet_activity_suppression (
                stable_identity TEXT PRIMARY KEY NOT NULL,
                disposition TEXT NOT NULL,
                suppressed_at TEXT NOT NULL,
                expires_at TEXT
            );

            CREATE TABLE IF NOT EXISTS clipboard_history (
                id TEXT PRIMARY KEY NOT NULL,
                kind TEXT NOT NULL CHECK(kind IN ('text', 'url', 'files', 'image')),
                preview TEXT NOT NULL,
                content_hash TEXT NOT NULL UNIQUE,
                source_application TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                is_pinned INTEGER NOT NULL DEFAULT 0,
                byte_count INTEGER NOT NULL,
                payload_text TEXT,
                payload_blob BLOB
            );

            CREATE INDEX IF NOT EXISTS ix_clipboard_history_order
                ON clipboard_history(is_pinned DESC, updated_at DESC);

            CREATE TABLE IF NOT EXISTS quick_search_usage (
                result_id TEXT PRIMARY KEY NOT NULL,
                use_count INTEGER NOT NULL,
                last_used_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS connector_state (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS plugin_runtime_state (
                plugin_id TEXT NOT NULL,
                release_id TEXT NOT NULL,
                state_json TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                PRIMARY KEY (plugin_id, release_id)
            );

            CREATE TABLE IF NOT EXISTS plugin_credential_metadata (
                scope_hash TEXT PRIMARY KEY NOT NULL,
                owner_user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                plugin_id TEXT NOT NULL,
                release_id TEXT NOT NULL,
                component_key TEXT NOT NULL,
                secret_name TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE UNIQUE INDEX IF NOT EXISTS ux_plugin_credential_scope
                ON plugin_credential_metadata(
                    owner_user_id,
                    device_id,
                    plugin_id,
                    release_id,
                    component_key,
                    secret_name);

            CREATE TABLE IF NOT EXISTS plugin_oauth_connection (
                id TEXT PRIMARY KEY NOT NULL,
                owner_user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                plugin_id TEXT NOT NULL,
                release_id TEXT NOT NULL,
                component_key TEXT NOT NULL,
                provider TEXT NOT NULL,
                resource TEXT NOT NULL,
                scopes_json TEXT NOT NULL,
                connected INTEGER NOT NULL,
                needs_auth INTEGER NOT NULL,
                expires_at TEXT,
                account_display TEXT,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS ix_plugin_oauth_connection_owner
                ON plugin_oauth_connection(owner_user_id, device_id, plugin_id);

            CREATE TABLE IF NOT EXISTS terminal_session_snapshot (
                session_id TEXT PRIMARY KEY NOT NULL,
                workspace_id TEXT,
                shell_kind TEXT NOT NULL,
                working_directory TEXT,
                state_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS connector_approval_settings (
                singleton_id INTEGER PRIMARY KEY NOT NULL CHECK(singleton_id = 1),
                mode TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS connector_model_settings (
                singleton_id INTEGER PRIMARY KEY NOT NULL CHECK(singleton_id = 1),
                model_request_max_retries INTEGER NOT NULL,
                command_approval_model_config_id TEXT,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS connector_sandbox_settings (
                singleton_id INTEGER PRIMARY KEY NOT NULL CHECK(singleton_id = 1),
                enabled INTEGER NOT NULL,
                permission_profile TEXT NOT NULL,
                network_access TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS connector_approval_history (
                id TEXT PRIMARY KEY NOT NULL,
                approval_id TEXT NOT NULL,
                request_id TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                command TEXT NOT NULL,
                working_directory TEXT NOT NULL,
                source TEXT NOT NULL,
                mode TEXT NOT NULL,
                approved INTEGER NOT NULL,
                reviewer TEXT NOT NULL,
                risk TEXT NOT NULL,
                risk_reason TEXT,
                reason TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS ix_connector_approval_history_created_at
                ON connector_approval_history(created_at DESC);

            CREATE TABLE IF NOT EXISTS connector_command_history (
                id TEXT PRIMARY KEY NOT NULL,
                request_id TEXT NOT NULL,
                workspace_id TEXT NOT NULL,
                source TEXT NOT NULL,
                command TEXT NOT NULL,
                working_directory TEXT NOT NULL,
                success INTEGER NOT NULL,
                exit_code INTEGER,
                timed_out INTEGER NOT NULL,
                timeout_ms INTEGER NOT NULL,
                stdout_preview TEXT NOT NULL,
                stderr_preview TEXT NOT NULL,
                stdout_bytes INTEGER NOT NULL,
                stderr_bytes INTEGER NOT NULL,
                stdout_truncated INTEGER NOT NULL,
                stderr_truncated INTEGER NOT NULL,
                approval_decision TEXT NOT NULL,
                approval_reason TEXT NOT NULL,
                error TEXT,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS ix_connector_command_history_created_at
                ON connector_command_history(created_at DESC);

            CREATE TABLE IF NOT EXISTS diagnostic_event (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                level TEXT NOT NULL,
                category TEXT NOT NULL,
                message TEXT NOT NULL,
                correlation_id TEXT,
                occurred_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS ix_diagnostic_event_occurred_at
                ON diagnostic_event(occurred_at DESC);

            CREATE TABLE IF NOT EXISTS agent_profiles (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL,
                role_prompt TEXT NOT NULL,
                model_config_id TEXT NOT NULL,
                thinking_level TEXT,
                profession_key TEXT NOT NULL,
                default_plugin_ids_json TEXT NOT NULL,
                default_skill_ids_json TEXT NOT NULL,
                heartbeat_enabled INTEGER NOT NULL,
                heartbeat_interval_seconds INTEGER NOT NULL,
                heartbeat_prompt TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                last_heartbeat_at_unix_ms INTEGER,
                next_heartbeat_at_unix_ms INTEGER,
                PRIMARY KEY(owner_user_id, id)
            );

            CREATE INDEX IF NOT EXISTS ix_agent_profiles_owner_status
                ON agent_profiles(owner_user_id, status, name, id);

            CREATE TABLE IF NOT EXISTS agent_rooms (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                project_id TEXT NOT NULL,
                name TEXT NOT NULL,
                goal TEXT NOT NULL,
                default_agent_id TEXT,
                project_manager_agent_id TEXT,
                conversation_kind TEXT NOT NULL,
                direct_key TEXT,
                status TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, id)
            );

            CREATE UNIQUE INDEX IF NOT EXISTS ux_agent_rooms_direct
                ON agent_rooms(owner_user_id, direct_key)
                WHERE direct_key IS NOT NULL;

            CREATE INDEX IF NOT EXISTS ix_agent_rooms_project
                ON agent_rooms(owner_user_id, project_id, status, updated_at_unix_ms DESC);

            CREATE TABLE IF NOT EXISTS agent_room_members (
                owner_user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                role TEXT NOT NULL,
                responsibility TEXT NOT NULL,
                plugin_allowlist_json TEXT NOT NULL,
                status TEXT NOT NULL,
                joined_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, room_id, agent_id)
            );

            CREATE INDEX IF NOT EXISTS ix_agent_room_members_agent
                ON agent_room_members(owner_user_id, agent_id, status, room_id);

            CREATE TABLE IF NOT EXISTS agent_messages (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                sender_kind TEXT NOT NULL,
                sender_agent_id TEXT,
                content TEXT NOT NULL,
                reply_to_message_id TEXT,
                root_message_id TEXT NOT NULL,
                hop_count INTEGER NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, id)
            );

            CREATE INDEX IF NOT EXISTS ix_agent_messages_room
                ON agent_messages(owner_user_id, room_id, created_at_unix_ms, id);

            CREATE TABLE IF NOT EXISTS agent_message_mentions (
                owner_user_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                PRIMARY KEY(owner_user_id, message_id, agent_id)
            );

            CREATE TABLE IF NOT EXISTS agent_message_attachments (
                owner_user_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                id TEXT NOT NULL,
                name TEXT NOT NULL,
                mime_type TEXT NOT NULL,
                kind TEXT NOT NULL,
                byte_count INTEGER NOT NULL,
                payload BLOB NOT NULL,
                PRIMARY KEY(owner_user_id, message_id, id)
            );

            CREATE TABLE IF NOT EXISTS agent_message_attachment_payloads (
                owner_user_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                id TEXT NOT NULL,
                payload BLOB NOT NULL,
                PRIMARY KEY(owner_user_id, message_id, id)
            );

            INSERT OR IGNORE INTO agent_message_attachment_payloads (
                owner_user_id, message_id, id, payload)
            SELECT owner_user_id, message_id, id, payload
            FROM agent_message_attachments
            WHERE length(payload) > 0
              AND NOT EXISTS (SELECT 1 FROM schema_migrations WHERE version = 13);

            UPDATE agent_message_attachments
            SET payload = X''
            WHERE length(payload) > 0
              AND NOT EXISTS (SELECT 1 FROM schema_migrations WHERE version = 13)
              AND EXISTS (
                  SELECT 1 FROM agent_message_attachment_payloads stored
                  WHERE stored.owner_user_id = agent_message_attachments.owner_user_id
                    AND stored.message_id = agent_message_attachments.message_id
                    AND stored.id = agent_message_attachments.id);

            CREATE TABLE IF NOT EXISTS agent_read_cursors (
                owner_user_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                reader_id TEXT NOT NULL,
                through_message_id TEXT NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, room_id, reader_id)
            );

            CREATE TABLE IF NOT EXISTS agent_todos (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                title TEXT NOT NULL,
                detail TEXT NOT NULL,
                priority TEXT NOT NULL,
                dependency_ids_json TEXT NOT NULL,
                source_message_id TEXT,
                status TEXT NOT NULL,
                result TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                revision INTEGER NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, id)
            );

            CREATE INDEX IF NOT EXISTS ix_agent_todos_room
                ON agent_todos(owner_user_id, room_id, status, sort_order, created_at_unix_ms);

            CREATE INDEX IF NOT EXISTS ix_agent_todos_agent
                ON agent_todos(owner_user_id, agent_id, status, sort_order);

            CREATE TABLE IF NOT EXISTS agent_todo_progress (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                todo_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                sequence INTEGER NOT NULL,
                kind TEXT NOT NULL,
                stage TEXT NOT NULL,
                detail TEXT NOT NULL,
                created_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, id),
                UNIQUE(owner_user_id, todo_id, sequence)
            );

            CREATE TABLE IF NOT EXISTS agent_todo_progress_suggestions (
                owner_user_id TEXT NOT NULL,
                progress_id TEXT NOT NULL,
                position INTEGER NOT NULL,
                category TEXT NOT NULL,
                title TEXT NOT NULL,
                markdown TEXT NOT NULL,
                rationale TEXT NOT NULL,
                PRIMARY KEY(owner_user_id, progress_id, position)
            );

            CREATE TABLE IF NOT EXISTS agent_team_assets (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                category TEXT NOT NULL,
                title TEXT NOT NULL,
                markdown TEXT NOT NULL,
                status TEXT NOT NULL,
                revision INTEGER NOT NULL,
                created_by_agent_id TEXT,
                updated_by_agent_id TEXT,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, id)
            );

            CREATE INDEX IF NOT EXISTS ix_agent_team_assets_room
                ON agent_team_assets(owner_user_id, room_id, status, updated_at_unix_ms DESC);

            CREATE TABLE IF NOT EXISTS agent_team_asset_revisions (
                owner_user_id TEXT NOT NULL,
                asset_id TEXT NOT NULL,
                revision INTEGER NOT NULL,
                title TEXT NOT NULL,
                markdown TEXT NOT NULL,
                status TEXT NOT NULL,
                editor_agent_id TEXT,
                created_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, asset_id, revision)
            );

            CREATE TABLE IF NOT EXISTS agent_requirement_surveys (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                project_id TEXT NOT NULL,
                creator_agent_id TEXT NOT NULL,
                source_delivery_id TEXT NOT NULL,
                request_key TEXT NOT NULL,
                draft_json TEXT NOT NULL,
                status TEXT NOT NULL,
                submission_json TEXT,
                resolution_json TEXT,
                created_at_unix_ms INTEGER NOT NULL,
                submitted_at_unix_ms INTEGER,
                resolved_at_unix_ms INTEGER,
                PRIMARY KEY(owner_user_id, id),
                UNIQUE(owner_user_id, project_id, creator_agent_id, source_delivery_id, request_key)
            );

            CREATE TABLE IF NOT EXISTS agent_deliveries (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                message_id TEXT NOT NULL,
                root_message_id TEXT NOT NULL,
                target_agent_id TEXT NOT NULL,
                trigger_kind TEXT NOT NULL,
                status TEXT NOT NULL,
                attempt INTEGER NOT NULL,
                hop_count INTEGER NOT NULL,
                deduplication_key TEXT NOT NULL,
                response_message_id TEXT,
                last_error TEXT,
                claimed_at_unix_ms INTEGER,
                completed_at_unix_ms INTEGER,
                created_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, id),
                UNIQUE(owner_user_id, deduplication_key)
            );

            CREATE INDEX IF NOT EXISTS ix_agent_deliveries_pending
                ON agent_deliveries(owner_user_id, status, created_at_unix_ms, id);

            CREATE TABLE IF NOT EXISTS agent_runs (
                owner_user_id TEXT NOT NULL,
                id TEXT NOT NULL,
                delivery_id TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                room_id TEXT NOT NULL,
                status TEXT NOT NULL,
                model_calls INTEGER NOT NULL,
                last_error TEXT,
                created_at_unix_ms INTEGER NOT NULL,
                updated_at_unix_ms INTEGER NOT NULL,
                PRIMARY KEY(owner_user_id, id),
                UNIQUE(owner_user_id, delivery_id)
            );

            CREATE INDEX IF NOT EXISTS ix_agent_runs_room
                ON agent_runs(owner_user_id, room_id, updated_at_unix_ms DESC);

            INSERT OR IGNORE INTO agent_messages (
                owner_user_id, id, room_id, sender_kind, sender_agent_id, content,
                reply_to_message_id, root_message_id, hop_count, created_at_unix_ms)
            SELECT room.owner_user_id,
                   'asset-maintenance-message-' || room.id,
                   room.id,
                   'System',
                   NULL,
                   '你已被明确指定为“' || room.name || '”的项目经理。请读取 Human 消息、团队目标、成员和 Todo 状态，主动维护真实的团队共享资产。信息充分时建立或更新“项目概览”和“当前进度”；信息不足时先用 requirement_survey_create 发起需求调研，不要写空模板或臆测内容。完成本轮实际处理后再结束通讯周期。',
                   NULL,
                   'asset-maintenance-message-' || room.id,
                   0,
                   CAST(strftime('%s', 'now') AS INTEGER) * 1000
            FROM agent_rooms room
            JOIN agent_room_members member
              ON member.owner_user_id = room.owner_user_id
             AND member.room_id = room.id
             AND member.agent_id = room.project_manager_agent_id
             AND member.status = 'Active'
            WHERE room.status = 'Active'
              AND room.conversation_kind = 'ProjectTeam'
              AND room.project_manager_agent_id IS NOT NULL
              AND NOT EXISTS (SELECT 1 FROM schema_migrations WHERE version = 12)
              AND NOT EXISTS (
                  SELECT 1 FROM agent_team_assets asset
                  WHERE asset.owner_user_id = room.owner_user_id
                    AND asset.room_id = room.id
                    AND asset.status = 'Active'
                    AND asset.category IN ('Overview', 'CurrentProgress'));

            INSERT OR IGNORE INTO agent_message_mentions (
                owner_user_id, message_id, agent_id)
            SELECT room.owner_user_id,
                   'asset-maintenance-message-' || room.id,
                   room.project_manager_agent_id
            FROM agent_rooms room
            JOIN agent_messages message
              ON message.owner_user_id = room.owner_user_id
             AND message.id = 'asset-maintenance-message-' || room.id
            WHERE room.project_manager_agent_id IS NOT NULL
              AND NOT EXISTS (SELECT 1 FROM schema_migrations WHERE version = 12);

            INSERT OR IGNORE INTO agent_deliveries (
                owner_user_id, id, room_id, message_id, root_message_id, target_agent_id,
                trigger_kind, status, attempt, hop_count, deduplication_key,
                response_message_id, last_error, claimed_at_unix_ms,
                completed_at_unix_ms, created_at_unix_ms)
            SELECT room.owner_user_id,
                   'asset-maintenance-delivery-' || room.id,
                   room.id,
                   'asset-maintenance-message-' || room.id,
                   'asset-maintenance-message-' || room.id,
                   room.project_manager_agent_id,
                   'Mention',
                   'Pending',
                   0,
                   0,
                   'team-asset-maintenance:' || room.id || ':v1',
                   NULL,
                   NULL,
                   NULL,
                   NULL,
                   CAST(strftime('%s', 'now') AS INTEGER) * 1000
            FROM agent_rooms room
            JOIN agent_messages message
              ON message.owner_user_id = room.owner_user_id
             AND message.id = 'asset-maintenance-message-' || room.id
            WHERE room.project_manager_agent_id IS NOT NULL
              AND NOT EXISTS (SELECT 1 FROM schema_migrations WHERE version = 12);

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (3, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (5, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (7, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (8, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (9, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (10, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (11, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (12, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));

            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (13, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
            """;
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        await MigrateRequirementSurveysToProjectScopeAsync(connection, cancellationToken)
            .ConfigureAwait(false);
    }

    private static async Task MigrateRequirementSurveysToProjectScopeAsync(
        SqliteConnection connection,
        CancellationToken cancellationToken)
    {
        var hasProjectColumn = false;
        using (var columns = connection.CreateCommand())
        {
            columns.CommandText = "PRAGMA table_info(agent_requirement_surveys)";
            await using var reader = await columns.ExecuteReaderAsync(cancellationToken)
                .ConfigureAwait(false);
            while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            {
                if (string.Equals(reader.GetString(1), "project_id", StringComparison.Ordinal))
                {
                    hasProjectColumn = true;
                    break;
                }
            }
        }

        if (!hasProjectColumn)
        {
            using var transaction = connection.BeginTransaction();
            using var migration = connection.CreateCommand();
            migration.Transaction = transaction;
            migration.CommandText = """
                CREATE TABLE agent_requirement_surveys_v14 (
                    owner_user_id TEXT NOT NULL,
                    id TEXT NOT NULL,
                    project_id TEXT NOT NULL,
                    creator_agent_id TEXT NOT NULL,
                    source_delivery_id TEXT NOT NULL,
                    request_key TEXT NOT NULL,
                    draft_json TEXT NOT NULL,
                    status TEXT NOT NULL,
                    submission_json TEXT,
                    resolution_json TEXT,
                    created_at_unix_ms INTEGER NOT NULL,
                    submitted_at_unix_ms INTEGER,
                    resolved_at_unix_ms INTEGER,
                    PRIMARY KEY(owner_user_id, id),
                    UNIQUE(owner_user_id, project_id, creator_agent_id,
                        source_delivery_id, request_key)
                );

                INSERT INTO agent_requirement_surveys_v14 (
                    owner_user_id, id, project_id, creator_agent_id, source_delivery_id,
                    request_key, draft_json, status, submission_json, resolution_json,
                    created_at_unix_ms, submitted_at_unix_ms, resolved_at_unix_ms)
                SELECT survey.owner_user_id, survey.id,
                       COALESCE(room.project_id, survey.team_room_id),
                       survey.creator_agent_id, survey.source_delivery_id,
                       survey.request_key, survey.draft_json, survey.status,
                       survey.submission_json, survey.resolution_json,
                       survey.created_at_unix_ms, survey.submitted_at_unix_ms,
                       survey.resolved_at_unix_ms
                FROM agent_requirement_surveys survey
                LEFT JOIN agent_rooms room
                  ON room.owner_user_id = survey.owner_user_id
                 AND room.id = survey.team_room_id;

                DROP TABLE agent_requirement_surveys;
                ALTER TABLE agent_requirement_surveys_v14
                    RENAME TO agent_requirement_surveys;
                """;
            await migration.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
            await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        }

        using var finalize = connection.CreateCommand();
        finalize.CommandText = """
            CREATE INDEX IF NOT EXISTS ix_agent_requirement_surveys_project
                ON agent_requirement_surveys(
                    owner_user_id, project_id, status, created_at_unix_ms DESC);
            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (14, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'));
            """;
        await finalize.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    internal async Task<SqliteConnection> OpenConnectionAsync(
        CancellationToken cancellationToken = default)
    {
        var connection = new SqliteConnection(_connectionString);
        try
        {
            await connection.OpenAsync(cancellationToken).ConfigureAwait(false);
            return connection;
        }
        catch
        {
            await connection.DisposeAsync().ConfigureAwait(false);
            throw;
        }
    }
}
