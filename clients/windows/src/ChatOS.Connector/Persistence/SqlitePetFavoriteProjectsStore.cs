using System.Text.Json;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Persistence;

internal sealed class SqlitePetFavoriteProjectsStore(LocalStateDatabase database) : IPetFavoriteProjectsStore
{
    private const string FavoritesKey = "favorite_project_ids_v1";
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public async Task<IReadOnlyList<string>> LoadAsync(CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "SELECT value FROM pet_preferences WHERE key = $key LIMIT 1;";
        command.Parameters.AddWithValue("$key", FavoritesKey);
        var value = await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false);
        if (value is not string json || string.IsNullOrWhiteSpace(json)) return [];
        try
        {
            return JsonSerializer.Deserialize<string[]>(json, JsonOptions) ?? [];
        }
        catch (JsonException)
        {
            return [];
        }
    }

    public async Task SaveAsync(
        IReadOnlyCollection<string> projectIds,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(projectIds);
        var normalized = projectIds.Select(static value => value.Trim())
            .Where(static value => value.Length > 0)
            .Distinct(StringComparer.Ordinal)
            .Order(StringComparer.Ordinal)
            .ToArray();
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
        command.Parameters.AddWithValue("$key", FavoritesKey);
        command.Parameters.AddWithValue("$value", JsonSerializer.Serialize(normalized, JsonOptions));
        command.Parameters.AddWithValue("$updated_at", DateTimeOffset.UtcNow.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
