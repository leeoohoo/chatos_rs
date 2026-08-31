using System.Text.Json;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Persistence;

internal sealed class SqlitePetWindowPlacementStore(LocalStateDatabase database) : IPetWindowPlacementStore
{
    private const string PlacementKey = "window_placement_v1";
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public async Task<PetWindowPlacement?> LoadAsync(CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "SELECT value FROM pet_preferences WHERE key = $key LIMIT 1;";
        command.Parameters.AddWithValue("$key", PlacementKey);
        var value = await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false);
        if (value is not string json || string.IsNullOrWhiteSpace(json)) return null;
        try
        {
            return JsonSerializer.Deserialize<PetWindowPlacement>(json, JsonOptions);
        }
        catch (JsonException)
        {
            return null;
        }
    }

    public async Task SaveAsync(
        PetWindowPlacement placement,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(placement);
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO pet_preferences(key, value, updated_at)
            VALUES ($key, $value, $updated_at)
            ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at;
            """;
        command.Parameters.AddWithValue("$key", PlacementKey);
        command.Parameters.AddWithValue("$value", JsonSerializer.Serialize(placement, JsonOptions));
        command.Parameters.AddWithValue("$updated_at", DateTimeOffset.UtcNow.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
