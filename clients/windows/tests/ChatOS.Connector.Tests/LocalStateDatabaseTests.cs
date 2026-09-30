using ChatOS.Connector.Persistence;
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

            // Microsoft.Data.Sqlite can retain a native handle until its pool is
            // cleared on Windows, even after the managed connection is disposed.
            SqliteConnection.ClearAllPools();
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
    public async Task LegacyRequirementSurveyTableIsDroppedWithoutMigratingData()
    {
        var directory = Path.Combine(Path.GetTempPath(), "chatos-local-state-database-tests",
            Guid.NewGuid().ToString("N"));
        var databasePath = Path.Combine(directory, "state.db");
        Directory.CreateDirectory(directory);
        try
        {
            var database = new LocalStateDatabase(databasePath);
            await database.InitializeAsync();
            await using (var connection = await database.OpenConnectionAsync())
            {
                var legacy = connection.CreateCommand();
                legacy.CommandText = """
                    CREATE TABLE agent_requirement_surveys (
                        owner_user_id TEXT NOT NULL,
                        id TEXT NOT NULL,
                        payload TEXT NOT NULL
                    );
                    INSERT INTO agent_requirement_surveys (
                        owner_user_id, id, payload)
                    VALUES ('alice', 'survey-legacy', '{}');
                    DELETE FROM schema_migrations WHERE version = 14;
                    """;
                await legacy.ExecuteNonQueryAsync();
            }

            await database.InitializeAsync();
            await using var verify = await database.OpenConnectionAsync();
            var command = verify.CreateCommand();
            command.CommandText = """
                SELECT COUNT(*) FROM sqlite_master
                WHERE type = 'table' AND name = 'agent_requirement_surveys'
                """;
            Assert.Equal(0L, await command.ExecuteScalarAsync());
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
