using System.Text.Json;
using ChatOS.Connector.Runtime;

namespace ChatOS.Connector.Persistence;

public sealed class SqliteConnectorPersistentStateStore : IConnectorPersistentStateStore
{
    private const string StateKey = "runtime-v1";
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    private readonly LocalStateDatabase _database;

    public SqliteConnectorPersistentStateStore(LocalStateDatabase database)
    {
        _database = database;
    }

    public async Task<ConnectorPersistentState?> LoadAsync(
        CancellationToken cancellationToken = default)
    {
        await using var connection = await _database
            .OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "SELECT value FROM connector_state WHERE key = $key LIMIT 1;";
        command.Parameters.AddWithValue("$key", StateKey);
        var value = await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) as string;
        if (string.IsNullOrWhiteSpace(value))
        {
            return null;
        }

        try
        {
            return JsonSerializer.Deserialize<ConnectorPersistentState>(value, JsonOptions);
        }
        catch (JsonException exception)
        {
            throw new InvalidDataException("Stored connector state is invalid.", exception);
        }
    }

    public async Task SaveAsync(
        ConnectorPersistentState? state,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await _database
            .OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        if (state is null)
        {
            command.CommandText = "DELETE FROM connector_state WHERE key = $key;";
            command.Parameters.AddWithValue("$key", StateKey);
        }
        else
        {
            command.CommandText = """
                INSERT INTO connector_state(key, value, updated_at)
                VALUES ($key, $value, $updated_at)
                ON CONFLICT(key) DO UPDATE SET
                    value = excluded.value,
                    updated_at = excluded.updated_at;
                """;
            command.Parameters.AddWithValue("$key", StateKey);
            command.Parameters.AddWithValue("$value", JsonSerializer.Serialize(state, JsonOptions));
            command.Parameters.AddWithValue("$updated_at", DateTimeOffset.UtcNow.ToString("O"));
        }

        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
