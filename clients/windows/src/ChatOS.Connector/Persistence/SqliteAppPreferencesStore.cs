using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.Persistence;

public sealed class SqliteAppPreferencesStore : IAppPreferencesStore
{
    private const string PreferencesKey = "app_preferences_v1";
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private readonly LocalStateDatabase _database;

    public SqliteAppPreferencesStore(LocalStateDatabase database)
    {
        _database = database;
    }

    public async Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default)
    {
        await using var connection = await _database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "SELECT value FROM ui_state WHERE key = $key LIMIT 1;";
        command.Parameters.AddWithValue("$key", PreferencesKey);
        var value = await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false);
        if (value is not string json || string.IsNullOrWhiteSpace(json))
        {
            return null;
        }

        try
        {
            return JsonSerializer.Deserialize<AppPreferences>(json, JsonOptions)?.Normalize();
        }
        catch (JsonException)
        {
            return null;
        }
    }

    public async Task SaveAsync(
        AppPreferences preferences,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(preferences);
        await using var connection = await _database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO ui_state(key, value, updated_at)
            VALUES ($key, $value, $updatedAt)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at;
            """;
        command.Parameters.AddWithValue("$key", PreferencesKey);
        command.Parameters.AddWithValue(
            "$value",
            JsonSerializer.Serialize(preferences.Normalize(), JsonOptions));
        command.Parameters.AddWithValue("$updatedAt", DateTimeOffset.UtcNow.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
