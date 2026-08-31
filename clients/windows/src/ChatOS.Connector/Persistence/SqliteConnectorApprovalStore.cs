using ChatOS.Connector.Approval;
using Microsoft.Data.Sqlite;

namespace ChatOS.Connector.Persistence;

public sealed class SqliteConnectorApprovalStore(LocalStateDatabase database)
    : IConnectorApprovalStore
{
    public async Task<ConnectorApprovalMode?> ReadModeAsync(
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "SELECT mode FROM connector_approval_settings WHERE singleton_id = 1;";
        var value = await command.ExecuteScalarAsync(cancellationToken).ConfigureAwait(false) as string;
        return value is null ? null : ParseMode(value);
    }

    public async Task SaveModeAsync(
        ConnectorApprovalMode mode,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO connector_approval_settings(singleton_id, mode, updated_at)
            VALUES (1, $mode, $updated_at)
            ON CONFLICT(singleton_id) DO UPDATE SET
                mode = excluded.mode,
                updated_at = excluded.updated_at;
            """;
        command.Parameters.AddWithValue("$mode", Format(mode));
        command.Parameters.AddWithValue("$updated_at", DateTimeOffset.UtcNow.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task AppendAsync(
        ConnectorApprovalHistoryEntry entry,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO connector_approval_history(
                id, approval_id, request_id, workspace_id, command, working_directory,
                source, mode, approved, reviewer, risk, risk_reason, reason, created_at)
            VALUES (
                $id, $approval_id, $request_id, $workspace_id, $command, $working_directory,
                $source, $mode, $approved, $reviewer, $risk, $risk_reason, $reason, $created_at);
            """;
        command.Parameters.AddWithValue("$id", entry.Id);
        command.Parameters.AddWithValue("$approval_id", entry.ApprovalId);
        command.Parameters.AddWithValue("$request_id", entry.RequestId);
        command.Parameters.AddWithValue("$workspace_id", entry.WorkspaceId);
        command.Parameters.AddWithValue("$command", entry.Command);
        command.Parameters.AddWithValue("$working_directory", entry.WorkingDirectory);
        command.Parameters.AddWithValue("$source", entry.Source);
        command.Parameters.AddWithValue("$mode", Format(entry.Mode));
        command.Parameters.AddWithValue("$approved", entry.Approved ? 1 : 0);
        command.Parameters.AddWithValue("$reviewer", Format(entry.Reviewer));
        command.Parameters.AddWithValue("$risk", Format(entry.Risk));
        command.Parameters.AddWithValue("$risk_reason", (object?)entry.RiskReason ?? DBNull.Value);
        command.Parameters.AddWithValue("$reason", entry.Reason);
        command.Parameters.AddWithValue("$created_at", entry.CreatedAt.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task<IReadOnlyList<ConnectorApprovalHistoryEntry>> ReadHistoryAsync(
        int limit = 1_000,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT id, approval_id, request_id, workspace_id, command, working_directory,
                   source, mode, approved, reviewer, risk, risk_reason, reason, created_at
            FROM connector_approval_history
            ORDER BY created_at DESC
            LIMIT $limit;
            """;
        command.Parameters.AddWithValue("$limit", Math.Clamp(limit, 1, 1_000));
        var entries = new List<ConnectorApprovalHistoryEntry>();
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            entries.Add(new ConnectorApprovalHistoryEntry(
                reader.GetString(0),
                reader.GetString(1),
                reader.GetString(2),
                reader.GetString(3),
                reader.GetString(4),
                reader.GetString(5),
                reader.GetString(6),
                ParseMode(reader.GetString(7)),
                reader.GetInt64(8) != 0,
                ParseReviewer(reader.GetString(9)),
                ParseRisk(reader.GetString(10)),
                reader.IsDBNull(11) ? null : reader.GetString(11),
                reader.GetString(12),
                DateTimeOffset.Parse(reader.GetString(13))));
        }

        return entries;
    }

    private static string Format(ConnectorApprovalMode value) => value switch
    {
        ConnectorApprovalMode.RequestApproval => "request_approval",
        ConnectorApprovalMode.AutoApproval => "auto_approval",
        ConnectorApprovalMode.FullControl => "full_control",
        _ => throw new ArgumentOutOfRangeException(nameof(value)),
    };

    private static string Format(ConnectorApprovalReviewer value) =>
        value.ToString().ToLowerInvariant();

    private static string Format(ConnectorApprovalRiskLevel value) =>
        value.ToString().ToLowerInvariant();

    private static ConnectorApprovalMode ParseMode(string value) => value switch
    {
        "request_approval" => ConnectorApprovalMode.RequestApproval,
        "auto_approval" => ConnectorApprovalMode.AutoApproval,
        "full_control" => ConnectorApprovalMode.FullControl,
        _ => ConnectorApprovalMode.RequestApproval,
    };

    private static ConnectorApprovalReviewer ParseReviewer(string value) =>
        Enum.TryParse<ConnectorApprovalReviewer>(value, ignoreCase: true, out var result)
            ? result
            : ConnectorApprovalReviewer.System;

    private static ConnectorApprovalRiskLevel ParseRisk(string value) =>
        Enum.TryParse<ConnectorApprovalRiskLevel>(value, ignoreCase: true, out var result)
            ? result
            : ConnectorApprovalRiskLevel.Low;
}
