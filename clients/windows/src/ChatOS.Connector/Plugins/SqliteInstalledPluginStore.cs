using System.Text.Json;
using ChatOS.Connector.Persistence;

namespace ChatOS.Connector.Plugins;

public sealed class SqliteInstalledPluginStore(LocalStateDatabase database) : IInstalledPluginStore
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    public async Task<IReadOnlyList<InstalledPluginRecord>> ListAsync(
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "SELECT state_json FROM plugin_runtime_state ORDER BY updated_at DESC;";
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var records = new List<InstalledPluginRecord>();
        var seen = new HashSet<string>(StringComparer.Ordinal);
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            var record = Deserialize(reader.GetString(0));
            if (seen.Add(record.PluginId))
            {
                records.Add(record);
            }
        }

        return records;
    }

    public async Task<InstalledPluginRecord?> GetAsync(
        string pluginId,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT state_json
            FROM plugin_runtime_state
            WHERE plugin_id = $plugin_id
            ORDER BY updated_at DESC
            LIMIT 1;
            """;
        command.Parameters.AddWithValue("$plugin_id", pluginId);
        var value = await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) as string;
        return string.IsNullOrWhiteSpace(value) ? null : Deserialize(value);
    }

    public async Task SaveAsync(
        InstalledPluginRecord record,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        await using var transaction = (Microsoft.Data.Sqlite.SqliteTransaction)await connection
            .BeginTransactionAsync(cancellationToken)
            .ConfigureAwait(false);
        var delete = connection.CreateCommand();
        delete.Transaction = transaction;
        delete.CommandText = "DELETE FROM plugin_runtime_state WHERE plugin_id = $plugin_id;";
        delete.Parameters.AddWithValue("$plugin_id", record.PluginId);
        await delete.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);

        var insert = connection.CreateCommand();
        insert.Transaction = transaction;
        insert.CommandText = """
            INSERT INTO plugin_runtime_state(plugin_id, release_id, state_json, updated_at)
            VALUES ($plugin_id, $release_id, $state_json, $updated_at);
            """;
        insert.Parameters.AddWithValue("$plugin_id", record.PluginId);
        insert.Parameters.AddWithValue("$release_id", record.ReleaseId);
        insert.Parameters.AddWithValue("$state_json", JsonSerializer.Serialize(record, JsonOptions));
        insert.Parameters.AddWithValue("$updated_at", record.InstalledAt.ToString("O"));
        await insert.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task DeleteAsync(
        string pluginId,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "DELETE FROM plugin_runtime_state WHERE plugin_id = $plugin_id;";
        command.Parameters.AddWithValue("$plugin_id", pluginId);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    private static InstalledPluginRecord Deserialize(string value)
    {
        try
        {
            return JsonSerializer.Deserialize<InstalledPluginRecord>(value, JsonOptions)
                ?? throw new JsonException("Plugin installation record is empty.");
        }
        catch (JsonException exception)
        {
            throw new InvalidDataException("Stored plugin installation record is invalid.", exception);
        }
    }
}
