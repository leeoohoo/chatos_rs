using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.Persistence;

public sealed class SqliteClipboardHistoryStore(LocalStateDatabase database) : IClipboardHistoryStore
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public async Task<ClipboardHistoryEntry> StoreAsync(
        ClipboardHistoryPayload payload,
        string? sourceApplication,
        CancellationToken cancellationToken = default)
    {
        var normalized = Normalize(payload);
        Validate(normalized);
        var hash = Convert.ToHexString(SHA256.HashData(CanonicalBytes(normalized))).ToLowerInvariant();
        var now = DateTimeOffset.UtcNow;
        var id = Guid.NewGuid();
        var preview = Preview(normalized);
        var (payloadText, payloadBlob) = Encode(normalized);

        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO clipboard_history(
                id, kind, preview, content_hash, source_application,
                created_at, updated_at, is_pinned, byte_count, payload_text, payload_blob)
            VALUES($id, $kind, $preview, $hash, $source, $created, $updated, 0, $bytes, $text, $blob)
            ON CONFLICT(content_hash) DO UPDATE SET
                kind = excluded.kind,
                preview = excluded.preview,
                source_application = excluded.source_application,
                updated_at = excluded.updated_at,
                byte_count = excluded.byte_count,
                payload_text = excluded.payload_text,
                payload_blob = excluded.payload_blob
            RETURNING id, kind, preview, content_hash, source_application,
                created_at, updated_at, is_pinned, byte_count;
            """;
        command.Parameters.AddWithValue("$id", id.ToString("D"));
        command.Parameters.AddWithValue("$kind", KindValue(normalized.Kind));
        command.Parameters.AddWithValue("$preview", preview);
        command.Parameters.AddWithValue("$hash", hash);
        command.Parameters.AddWithValue("$source", (object?)NormalizeOptional(sourceApplication) ?? DBNull.Value);
        command.Parameters.AddWithValue("$created", now.ToString("O"));
        command.Parameters.AddWithValue("$updated", now.ToString("O"));
        command.Parameters.AddWithValue("$bytes", normalized.ByteCount);
        command.Parameters.AddWithValue("$text", (object?)payloadText ?? DBNull.Value);
        command.Parameters.Add("$blob", SqliteType.Blob).Value = (object?)payloadBlob ?? DBNull.Value;
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        await reader.ReadAsync(cancellationToken).ConfigureAwait(false);
        return ReadEntry(reader);
    }

    public async Task<IReadOnlyList<ClipboardHistoryEntry>> ListAsync(
        int limit = 500,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT id, kind, preview, content_hash, source_application,
                created_at, updated_at, is_pinned, byte_count
            FROM clipboard_history
            ORDER BY is_pinned DESC, updated_at DESC
            LIMIT $limit;
            """;
        command.Parameters.AddWithValue("$limit", Math.Clamp(limit, 1, 500));
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var entries = new List<ClipboardHistoryEntry>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
            entries.Add(ReadEntry(reader));
        return entries;
    }

    public async Task<ClipboardHistoryPayload?> ReadPayloadAsync(
        Guid id,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "SELECT kind, payload_text, payload_blob FROM clipboard_history WHERE id=$id;";
        command.Parameters.AddWithValue("$id", id.ToString("D"));
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        if (!await reader.ReadAsync(cancellationToken).ConfigureAwait(false)) return null;
        var kind = ParseKind(reader.GetString(0));
        var text = reader.IsDBNull(1) ? null : reader.GetString(1);
        var blob = reader.IsDBNull(2) ? null : (byte[])reader[2];
        try
        {
            return kind switch
            {
                ClipboardHistoryKind.Files => new(kind, FilePaths: JsonSerializer.Deserialize<string[]>(text ?? "[]", JsonOptions)),
                ClipboardHistoryKind.Image => blob is { Length: > 0 } ? new(kind, ImageBytes: blob) : null,
                _ => string.IsNullOrWhiteSpace(text) ? null : new(kind, Text: text),
            };
        }
        catch (JsonException)
        {
            return null;
        }
    }

    public Task SetPinnedAsync(Guid id, bool pinned, CancellationToken cancellationToken = default) =>
        ExecuteAsync("UPDATE clipboard_history SET is_pinned=$value WHERE id=$id;", id, pinned ? 1 : 0, cancellationToken);

    public Task DeleteAsync(Guid id, CancellationToken cancellationToken = default) =>
        ExecuteAsync("DELETE FROM clipboard_history WHERE id=$id;", id, null, cancellationToken);

    public async Task PruneAsync(
        DateTimeOffset cutoff,
        int unpinnedLimit = 500,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            DELETE FROM clipboard_history
            WHERE is_pinned=0 AND (
                updated_at < $cutoff OR id IN (
                    SELECT id FROM clipboard_history WHERE is_pinned=0
                    ORDER BY updated_at DESC LIMIT -1 OFFSET $limit
                )
            );
            """;
        command.Parameters.AddWithValue("$cutoff", cutoff.ToUniversalTime().ToString("O"));
        command.Parameters.AddWithValue("$limit", Math.Clamp(unpinnedLimit, 1, 500));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    private async Task ExecuteAsync(
        string sql,
        Guid id,
        int? value,
        CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = sql;
        command.Parameters.AddWithValue("$id", id.ToString("D"));
        if (value is not null) command.Parameters.AddWithValue("$value", value.Value);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    private static ClipboardHistoryEntry ReadEntry(SqliteDataReader reader) => new(
        Guid.Parse(reader.GetString(0)),
        ParseKind(reader.GetString(1)),
        reader.GetString(2),
        reader.GetString(3),
        reader.IsDBNull(4) ? null : reader.GetString(4),
        DateTimeOffset.Parse(reader.GetString(5)),
        DateTimeOffset.Parse(reader.GetString(6)),
        reader.GetInt32(7) != 0,
        reader.GetInt32(8));

    private static void Validate(ClipboardHistoryPayload payload)
    {
        if (payload.ByteCount <= 0) throw new ArgumentException("Clipboard payload is empty.", nameof(payload));
        if (payload.ByteCount > 20 * 1024 * 1024)
            throw new ArgumentException("Clipboard payload exceeds 20 MB.", nameof(payload));
    }

    private static ClipboardHistoryPayload Normalize(ClipboardHistoryPayload payload) => payload.Kind switch
    {
        ClipboardHistoryKind.Files => payload with
        {
            FilePaths = payload.FilePaths?.Where(static value => !string.IsNullOrWhiteSpace(value))
                .Select(static value => Path.GetFullPath(value.Trim()))
                .Distinct(StringComparer.OrdinalIgnoreCase).ToArray(),
        },
        ClipboardHistoryKind.Image => payload with { ImageBytes = payload.ImageBytes?.ToArray() },
        _ => payload with { Text = payload.Text?.Trim() },
    };

    private static byte[] CanonicalBytes(ClipboardHistoryPayload payload) => payload.Kind switch
    {
        ClipboardHistoryKind.Image => payload.ImageBytes ?? [],
        ClipboardHistoryKind.Files => Encoding.UTF8.GetBytes(string.Join('\n', payload.FilePaths ?? [])),
        _ => Encoding.UTF8.GetBytes(payload.Text ?? string.Empty),
    };

    private static (string? Text, byte[]? Blob) Encode(ClipboardHistoryPayload payload) => payload.Kind switch
    {
        ClipboardHistoryKind.Files => (JsonSerializer.Serialize(payload.FilePaths ?? [], JsonOptions), null),
        ClipboardHistoryKind.Image => (null, payload.ImageBytes),
        _ => (payload.Text, null),
    };

    private static string Preview(ClipboardHistoryPayload payload)
    {
        if (payload.Kind == ClipboardHistoryKind.Image)
            return $"Image · {payload.ByteCount / 1024d:0.#} KB";
        if (payload.Kind == ClipboardHistoryKind.Files)
            return string.Join(", ", (payload.FilePaths ?? []).Select(Path.GetFileName).Take(3));
        var preview = (payload.Text ?? string.Empty).ReplaceLineEndings(" ");
        return preview[..Math.Min(240, preview.Length)];
    }

    private static string? NormalizeOptional(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private static string KindValue(ClipboardHistoryKind kind) => kind.ToString().ToLowerInvariant();

    private static ClipboardHistoryKind ParseKind(string value) =>
        Enum.Parse<ClipboardHistoryKind>(value, ignoreCase: true);
}
