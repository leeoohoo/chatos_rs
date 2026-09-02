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
}
