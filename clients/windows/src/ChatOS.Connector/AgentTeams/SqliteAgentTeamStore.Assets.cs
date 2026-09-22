using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.AgentTeams;

public sealed partial class SqliteAgentTeamStore
{
    private const string AssetColumns = """
        owner_user_id, id, room_id, category, title, markdown, status, revision,
        created_by_agent_id, updated_by_agent_id, created_at_unix_ms, updated_at_unix_ms
        """;

    public async Task<IReadOnlyList<AgentTeamAsset>> ListAssetsAsync(
        string ownerUserId,
        string roomId,
        bool includeArchived = false,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null,
            $"SELECT {AssetColumns} FROM agent_team_assets WHERE owner_user_id = @p0 AND room_id = @p1" +
            (includeArchived ? string.Empty : " AND status = 'Active'") +
            " ORDER BY updated_at_unix_ms DESC, id", ownerUserId, roomId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var output = new List<AgentTeamAsset>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            output.Add(ReadAsset(reader));
        }

        return output;
    }

    public async Task<AgentTeamAsset> UpsertAssetAsync(
        string ownerUserId,
        string roomId,
        string? assetId,
        string? editorAgentId,
        AgentTeamAssetCategory category,
        string title,
        string markdown,
        int? expectedRevision,
        CancellationToken cancellationToken = default)
    {
        AgentTeamValidation.Text(title, nameof(title), 240);
        AgentTeamValidation.OptionalText(markdown, nameof(markdown), 256_000);
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        await RequireRoomAsync(connection, transaction, ownerUserId, roomId, requireActive: true,
            cancellationToken).ConfigureAwait(false);
        if (editorAgentId is not null)
        {
            await RequireActiveMemberAsync(connection, transaction, ownerUserId, roomId,
                editorAgentId, cancellationToken).ConfigureAwait(false);
        }

        var now = Now();
        AgentTeamAsset asset;
        if (assetId is null)
        {
            if (expectedRevision is not null)
            {
                throw Conflict("A new asset cannot have an expected revision.");
            }

            asset = new AgentTeamAsset(NewId(), ownerUserId, roomId, category, title, markdown,
                AgentTeamAssetStatus.Active, 1, editorAgentId, editorAgentId, now, now);
            asset.Validate();
            using var insert = Command(connection, transaction, $"""
                INSERT INTO agent_team_assets ({AssetColumns})
                VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7, @p8, @p9, @p10, @p11)
                """, ownerUserId, asset.Id, roomId, category.ToString(), title, markdown,
                asset.Status.ToString(), 1, DbValue(editorAgentId), DbValue(editorAgentId), now, now);
            await insert.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
        }
        else
        {
            var current = await ReadAssetAsync(
                connection, transaction, ownerUserId, roomId, assetId, cancellationToken).ConfigureAwait(false)
                ?? throw NotFound("Team asset");
            if (expectedRevision != current.Revision)
            {
                throw Conflict("Team asset changed before the update was applied.");
            }

            asset = current with
            {
                Category = category,
                Title = title,
                Markdown = markdown,
                Status = AgentTeamAssetStatus.Active,
                Revision = current.Revision + 1,
                UpdatedByAgentId = editorAgentId,
                UpdatedAtUnixMs = Math.Max(now, current.UpdatedAtUnixMs),
            };
            using var update = Command(connection, transaction, """
                UPDATE agent_team_assets SET category = @p0, title = @p1, markdown = @p2,
                    status = 'Active', revision = @p3, updated_by_agent_id = @p4,
                    updated_at_unix_ms = @p5
                WHERE owner_user_id = @p6 AND id = @p7 AND revision = @p8
                """, category.ToString(), title, markdown, asset.Revision, DbValue(editorAgentId),
                asset.UpdatedAtUnixMs, ownerUserId, assetId, current.Revision);
            if (await update.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
            {
                throw Conflict("Team asset changed before the update was applied.");
            }
        }

        await InsertAssetRevisionAsync(connection, transaction, asset, editorAgentId,
            cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return asset;
    }

    public async Task<AgentTeamAsset> ArchiveAssetAsync(
        string ownerUserId,
        string roomId,
        string assetId,
        string? editorAgentId,
        int expectedRevision,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        if (editorAgentId is not null)
        {
            await RequireActiveMemberAsync(connection, transaction, ownerUserId, roomId,
                editorAgentId, cancellationToken).ConfigureAwait(false);
        }

        var current = await ReadAssetAsync(
            connection, transaction, ownerUserId, roomId, assetId, cancellationToken).ConfigureAwait(false)
            ?? throw NotFound("Team asset");
        if (current.Revision != expectedRevision)
        {
            throw Conflict("Team asset changed before the archive was applied.");
        }

        var asset = current with
        {
            Status = AgentTeamAssetStatus.Archived,
            Revision = current.Revision + 1,
            UpdatedByAgentId = editorAgentId,
            UpdatedAtUnixMs = Math.Max(Now(), current.UpdatedAtUnixMs),
        };
        using var update = Command(connection, transaction, """
            UPDATE agent_team_assets SET status = 'Archived', revision = @p0,
                updated_by_agent_id = @p1, updated_at_unix_ms = @p2
            WHERE owner_user_id = @p3 AND id = @p4 AND revision = @p5
            """, asset.Revision, DbValue(editorAgentId), asset.UpdatedAtUnixMs,
            ownerUserId, assetId, current.Revision);
        if (await update.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
        {
            throw Conflict("Team asset changed before the archive was applied.");
        }

        await InsertAssetRevisionAsync(connection, transaction, asset, editorAgentId,
            cancellationToken).ConfigureAwait(false);
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return asset;
    }

    public async Task<IReadOnlyList<AgentTeamAssetRevision>> ListAssetRevisionsAsync(
        string ownerUserId,
        string assetId,
        int limit = 100,
        CancellationToken cancellationToken = default)
    {
        if (limit is < 1 or > 1_000)
        {
            throw AgentTeamValidation.Invalid(nameof(limit));
        }

        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null, """
            SELECT revision, title, markdown, status, editor_agent_id, created_at_unix_ms
            FROM agent_team_asset_revisions
            WHERE owner_user_id = @p0 AND asset_id = @p1
            ORDER BY revision DESC LIMIT @p2
            """, ownerUserId, assetId, limit);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var output = new List<AgentTeamAssetRevision>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            output.Add(new AgentTeamAssetRevision(assetId, reader.GetInt32(0),
                reader.GetString(1), reader.GetString(2),
                ParseEnum<AgentTeamAssetStatus>(reader.GetString(3)),
                reader.IsDBNull(4) ? null : reader.GetString(4), reader.GetInt64(5)));
        }

        return output;
    }

    private static AgentTeamAsset ReadAsset(SqliteDataReader reader)
    {
        var asset = new AgentTeamAsset(reader.GetString(1), reader.GetString(0),
            reader.GetString(2), ParseEnum<AgentTeamAssetCategory>(reader.GetString(3)),
            reader.GetString(4), reader.GetString(5),
            ParseEnum<AgentTeamAssetStatus>(reader.GetString(6)), reader.GetInt32(7),
            reader.IsDBNull(8) ? null : reader.GetString(8),
            reader.IsDBNull(9) ? null : reader.GetString(9), reader.GetInt64(10),
            reader.GetInt64(11));
        asset.Validate();
        return asset;
    }

    private static async Task<AgentTeamAsset?> ReadAssetAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        string ownerUserId,
        string roomId,
        string assetId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            $"SELECT {AssetColumns} FROM agent_team_assets " +
            "WHERE owner_user_id = @p0 AND room_id = @p1 AND id = @p2",
            ownerUserId, roomId, assetId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false) ? ReadAsset(reader) : null;
    }

    private static async Task InsertAssetRevisionAsync(
        SqliteConnection connection,
        SqliteTransaction transaction,
        AgentTeamAsset asset,
        string? editorAgentId,
        CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction, """
            INSERT INTO agent_team_asset_revisions (
                owner_user_id, asset_id, revision, title, markdown, status,
                editor_agent_id, created_at_unix_ms)
            VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7)
            """, asset.OwnerUserId, asset.Id, asset.Revision, asset.Title, asset.Markdown,
            asset.Status.ToString(), DbValue(editorAgentId), asset.UpdatedAtUnixMs);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
