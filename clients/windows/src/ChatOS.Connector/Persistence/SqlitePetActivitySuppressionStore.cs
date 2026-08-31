using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Persistence;

public sealed class SqlitePetActivitySuppressionStore : IPetActivitySuppressionStore
{
    private readonly LocalStateDatabase _database;

    public SqlitePetActivitySuppressionStore(LocalStateDatabase database)
    {
        _database = database;
    }

    public async Task<bool> IsSuppressedAsync(
        string stableIdentity,
        DateTimeOffset now,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await _database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT 1
            FROM pet_activity_suppression
            WHERE stable_identity = $identity
              AND (expires_at IS NULL OR expires_at > $now)
            LIMIT 1;
            """;
        command.Parameters.AddWithValue("$identity", stableIdentity);
        command.Parameters.AddWithValue("$now", now.ToString("O"));
        return await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) is not null;
    }

    public async Task SuppressAsync(
        string stableIdentity,
        PetActivityDisposition disposition,
        DateTimeOffset suppressedAt,
        DateTimeOffset? expiresAt,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await _database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO pet_activity_suppression(
                stable_identity,
                disposition,
                suppressed_at,
                expires_at
            ) VALUES ($identity, $disposition, $suppressed_at, $expires_at)
            ON CONFLICT(stable_identity) DO UPDATE SET
                disposition = excluded.disposition,
                suppressed_at = excluded.suppressed_at,
                expires_at = excluded.expires_at;
            """;
        command.Parameters.AddWithValue("$identity", stableIdentity);
        command.Parameters.AddWithValue("$disposition", disposition.ToString().ToLowerInvariant());
        command.Parameters.AddWithValue("$suppressed_at", suppressedAt.ToString("O"));
        command.Parameters.AddWithValue(
            "$expires_at",
            expiresAt is null ? DBNull.Value : expiresAt.Value.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task RemoveAsync(
        string stableIdentity,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await _database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "DELETE FROM pet_activity_suppression WHERE stable_identity = $identity;";
        command.Parameters.AddWithValue("$identity", stableIdentity);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task PruneExpiredAsync(
        DateTimeOffset now,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await _database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "DELETE FROM pet_activity_suppression WHERE expires_at IS NOT NULL AND expires_at <= $now;";
        command.Parameters.AddWithValue("$now", now.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
