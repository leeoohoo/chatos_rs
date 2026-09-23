using ChatOS.Connector.AgentTeams;
using ChatOS.Connector.Persistence;
using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.Tests;

public sealed class LocalStateDatabaseTests
{
    [Fact]
    public async Task CustomDatabaseReleasesFileHandleAfterConnectionsAreDisposed()
    {
        var directory = Path.Combine(
            Path.GetTempPath(),
            "chatos-local-state-database-tests",
            Guid.NewGuid().ToString("N"));
        var databasePath = Path.Combine(directory, "state.db");
        Directory.CreateDirectory(directory);

        try
        {
            var database = new LocalStateDatabase(databasePath);
            await database.InitializeAsync();
            await using (var connection = await database.OpenConnectionAsync())
            {
                var command = connection.CreateCommand();
                command.CommandText = "SELECT 1;";
                Assert.Equal(1L, await command.ExecuteScalarAsync());
            }

            File.Delete(databasePath);
            Assert.False(File.Exists(databasePath));
        }
        finally
        {
            SqliteConnection.ClearAllPools();
            if (Directory.Exists(directory))
            {
                Directory.Delete(directory, recursive: true);
            }
        }
    }

    [Fact]
    public async Task RequirementSurveyMigrationMovesLegacyRoomOwnershipToProject()
    {
        var directory = Path.Combine(Path.GetTempPath(), "chatos-local-state-database-tests",
            Guid.NewGuid().ToString("N"));
        var databasePath = Path.Combine(directory, "state.db");
        Directory.CreateDirectory(directory);
        try
        {
            var database = new LocalStateDatabase(databasePath);
            await database.InitializeAsync();
            var store = new SqliteAgentTeamStore(database);
            var manager = await store.CreateAgentAsync("alice",
                new AgentProfileDraft("经理", "", "负责项目", "model-1"));
            var room = await store.CreateRoomAsync("alice", "project-legacy",
                new AgentRoomDraft("旧团队", "迁移调研"), manager.Id);

            await using (var connection = await database.OpenConnectionAsync())
            {
                var legacy = connection.CreateCommand();
                legacy.CommandText = """
                    DROP INDEX ix_agent_requirement_surveys_project;
                    DROP TABLE agent_requirement_surveys;
                    CREATE TABLE agent_requirement_surveys (
                        owner_user_id TEXT NOT NULL, id TEXT NOT NULL,
                        team_room_id TEXT NOT NULL, creator_agent_id TEXT NOT NULL,
                        source_delivery_id TEXT NOT NULL, request_key TEXT NOT NULL,
                        draft_json TEXT NOT NULL, status TEXT NOT NULL,
                        submission_json TEXT, resolution_json TEXT,
                        created_at_unix_ms INTEGER NOT NULL,
                        submitted_at_unix_ms INTEGER, resolved_at_unix_ms INTEGER,
                        PRIMARY KEY(owner_user_id, id)
                    );
                    INSERT INTO agent_requirement_surveys (
                        owner_user_id, id, team_room_id, creator_agent_id,
                        source_delivery_id, request_key, draft_json, status,
                        created_at_unix_ms)
                    VALUES ('alice', 'survey-legacy', @room, @agent,
                        'delivery-legacy', 'legacy', '{}', 'Pending', 1);
                    DELETE FROM schema_migrations WHERE version = 14;
                    """;
                legacy.Parameters.AddWithValue("@room", room.Id);
                legacy.Parameters.AddWithValue("@agent", manager.Id);
                await legacy.ExecuteNonQueryAsync();
            }

            await database.InitializeAsync();
            await using var verify = await database.OpenConnectionAsync();
            var command = verify.CreateCommand();
            command.CommandText = """
                SELECT project_id FROM agent_requirement_surveys
                WHERE owner_user_id = 'alice' AND id = 'survey-legacy'
                """;
            Assert.Equal("project-legacy", await command.ExecuteScalarAsync());
            command.CommandText = "SELECT COUNT(*) FROM schema_migrations WHERE version = 14";
            Assert.Equal(1L, await command.ExecuteScalarAsync());
        }
        finally
        {
            SqliteConnection.ClearAllPools();
            if (Directory.Exists(directory)) Directory.Delete(directory, recursive: true);
        }
    }
}
