using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Persistence;

public sealed class SqliteQuickSearchUsageStore(LocalStateDatabase database) : IQuickSearchUsageStore
{
    public async Task<IReadOnlyDictionary<string, QuickSearchUsage>> LoadAsync(
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "SELECT result_id, use_count, last_used_at FROM quick_search_usage;";
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var values = new Dictionary<string, QuickSearchUsage>(StringComparer.Ordinal);
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            values[reader.GetString(0)] = new(reader.GetInt32(1), DateTimeOffset.Parse(reader.GetString(2)));
        }
        return values;
    }

    public async Task RecordAsync(string resultId, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(resultId)) return;
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO quick_search_usage(result_id, use_count, last_used_at)
            VALUES($id, 1, $now)
            ON CONFLICT(result_id) DO UPDATE SET
                use_count = MIN(1000, quick_search_usage.use_count + 1),
                last_used_at = excluded.last_used_at;
            """;
        command.Parameters.AddWithValue("$id", resultId.Trim());
        command.Parameters.AddWithValue("$now", DateTimeOffset.UtcNow.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
