using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.Persistence;

/// <summary>Local-only, account-scoped authority. Database failures propagate, never become an empty list.</summary>
public sealed class SqliteProjectRegistry(LocalStateDatabase database) : IProjectRegistry
{
    private const string Columns = "owner_user_id, id, name, description, workspace_id, relative_root, revision, status, created_at_unix_ms, updated_at_unix_ms";

    public async Task<IReadOnlyList<LocalProjectRecord>> ListAsync(
        string ownerUserId, bool includeInactive = false, CancellationToken cancellationToken = default)
    {
        ProjectRegistryValidation.Identifier(ownerUserId, nameof(ownerUserId));
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var command = Command(connection, null,
            $"SELECT {Columns} FROM local_project_records WHERE owner_user_id = @p0" +
            (includeInactive ? "" : " AND status = 'active'") + " ORDER BY name, id", ownerUserId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var records = new List<LocalProjectRecord>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false)) records.Add(ReadRecord(reader));
        return records;
    }

    public async Task<LocalProjectRecord?> GetAsync(
        string ownerUserId, string id, CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        return await ReadAsync(connection, null, ownerUserId, id, cancellationToken).ConfigureAwait(false);
    }

    public async Task<LocalProjectRecord> CreateAsync(
        string ownerUserId, LocalProjectDraft draft, CancellationToken cancellationToken = default)
    {
        var now = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var record = new LocalProjectRecord(Guid.NewGuid().ToString("D"), ownerUserId, draft, 1,
            LocalProjectStatus.Active, now, now);
        record.Validate();
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        await InsertAsync(connection, null, record, cancellationToken).ConfigureAwait(false);
        return record;
    }

    public async Task<LocalProjectRecord> UpdateAsync(
        string ownerUserId, string id, long expectedRevision, LocalProjectDraft draft,
        LocalProjectStatus status, CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        using var transaction = connection.BeginTransaction(deferred: false);
        var old = await ReadAsync(connection, transaction, ownerUserId, id, cancellationToken).ConfigureAwait(false)
            ?? throw new ProjectRegistryException(ProjectRegistryError.NotFound, "Local project not found.");
        if (old.Revision != expectedRevision)
            throw new ProjectRegistryException(ProjectRegistryError.RevisionConflict, "Project has changed. Refresh and retry.");
        if (old.Status == LocalProjectStatus.Removed)
            throw new ProjectRegistryException(ProjectRegistryError.Removed, "Removed projects cannot be updated or restored.");
        var record = old with
        {
            Draft = draft, Status = status, Revision = old.Revision + 1,
            UpdatedAtUnixMs = Math.Max(DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(), old.UpdatedAtUnixMs),
        };
        record.Validate();
        using var command = Command(connection, transaction, """
            UPDATE local_project_records SET name = @p0, description = @p1, workspace_id = @p2,
                relative_root = @p3, revision = @p4, status = @p5, updated_at_unix_ms = @p6
            WHERE owner_user_id = @p7 AND id = @p8 AND revision = @p9
            """, draft.Name, draft.Description, draft.WorkspaceId, draft.RelativeRoot,
            record.Revision, StatusText(status), record.UpdatedAtUnixMs, ownerUserId, id, expectedRevision);
        if (await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false) != 1)
            throw new ProjectRegistryException(ProjectRegistryError.RevisionConflict, "Project has changed.");
        await transaction.CommitAsync(cancellationToken).ConfigureAwait(false);
        return record;
    }

    private static async Task<LocalProjectRecord?> ReadAsync(
        SqliteConnection connection, SqliteTransaction? transaction,
        string ownerUserId, string id, CancellationToken cancellationToken)
    {
        ProjectRegistryValidation.Identifier(ownerUserId, nameof(ownerUserId));
        ProjectRegistryValidation.Identifier(id, nameof(id));
        using var command = Command(connection, transaction,
            $"SELECT {Columns} FROM local_project_records WHERE owner_user_id = @p0 AND id = @p1", ownerUserId, id);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        return await reader.ReadAsync(cancellationToken).ConfigureAwait(false) ? ReadRecord(reader) : null;
    }

    private static async Task InsertAsync(
        SqliteConnection connection, SqliteTransaction? transaction, LocalProjectRecord record, CancellationToken cancellationToken)
    {
        using var command = Command(connection, transaction,
            $"INSERT INTO local_project_records ({Columns}) VALUES (@p0, @p1, @p2, @p3, @p4, @p5, @p6, @p7, @p8, @p9)",
            record.OwnerUserId, record.Id, record.Draft.Name, record.Draft.Description,
            record.Draft.WorkspaceId, record.Draft.RelativeRoot, record.Revision, StatusText(record.Status),
            record.CreatedAtUnixMs, record.UpdatedAtUnixMs);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    private static LocalProjectRecord ReadRecord(SqliteDataReader reader)
    {
        var status = reader.GetString(7) switch
        {
            "active" => LocalProjectStatus.Active,
            "archived" => LocalProjectStatus.Archived,
            "removed" => LocalProjectStatus.Removed,
            _ => throw new InvalidDataException("Invalid project status."),
        };
        var record = new LocalProjectRecord(reader.GetString(1), reader.GetString(0),
            new(reader.GetString(2), reader.GetString(4), reader.GetString(5), reader.GetString(3)),
            reader.GetInt64(6), status, reader.GetInt64(8), reader.GetInt64(9));
        record.Validate();
        return record;
    }

    private static string StatusText(LocalProjectStatus status) => status.ToString().ToLowerInvariant();

    private static SqliteCommand Command(
        SqliteConnection connection, SqliteTransaction? transaction, string sql, params object[] values)
    {
        var command = connection.CreateCommand();
        command.Transaction = transaction;
        command.CommandText = sql;
        for (var index = 0; index < values.Length; index++) command.Parameters.AddWithValue($"@p{index}", values[index]);
        return command;
    }
}
