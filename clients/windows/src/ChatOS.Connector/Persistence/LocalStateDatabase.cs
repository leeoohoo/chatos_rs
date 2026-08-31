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
            "chatos-client.db"))
    {
    }

    internal LocalStateDatabase(string databasePath)
    {
        var stateDirectory = Path.GetDirectoryName(databasePath)
            ?? throw new ArgumentException("Database path must include a directory.", nameof(databasePath));
        Directory.CreateDirectory(stateDirectory);
        _connectionString = new SqliteConnectionStringBuilder
        {
            DataSource = databasePath,
            Mode = SqliteOpenMode.ReadWriteCreate,
            Cache = SqliteCacheMode.Shared,
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
            """;
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
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
