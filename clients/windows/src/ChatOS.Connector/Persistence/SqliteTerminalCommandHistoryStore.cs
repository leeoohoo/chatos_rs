using ChatOS.Connector.Terminal;

namespace ChatOS.Connector.Persistence;

public sealed class SqliteTerminalCommandHistoryStore(LocalStateDatabase database)
    : ITerminalCommandHistoryStore
{
    public async Task AppendAsync(
        TerminalCommandHistoryEntry entry,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO connector_command_history(
                id, request_id, workspace_id, source, command, working_directory,
                success, exit_code, timed_out, timeout_ms, stdout_preview, stderr_preview,
                stdout_bytes, stderr_bytes, stdout_truncated, stderr_truncated,
                approval_decision, approval_reason, error, created_at)
            VALUES (
                $id, $request_id, $workspace_id, $source, $command, $working_directory,
                $success, $exit_code, $timed_out, $timeout_ms, $stdout_preview, $stderr_preview,
                $stdout_bytes, $stderr_bytes, $stdout_truncated, $stderr_truncated,
                $approval_decision, $approval_reason, $error, $created_at);
            """;
        command.Parameters.AddWithValue("$id", entry.Id);
        command.Parameters.AddWithValue("$request_id", entry.RequestId);
        command.Parameters.AddWithValue("$workspace_id", entry.WorkspaceId);
        command.Parameters.AddWithValue("$source", entry.Source);
        command.Parameters.AddWithValue("$command", entry.Command);
        command.Parameters.AddWithValue("$working_directory", entry.WorkingDirectory);
        command.Parameters.AddWithValue("$success", entry.Success ? 1 : 0);
        command.Parameters.AddWithValue("$exit_code", (object?)entry.ExitCode ?? DBNull.Value);
        command.Parameters.AddWithValue("$timed_out", entry.TimedOut ? 1 : 0);
        command.Parameters.AddWithValue("$timeout_ms", entry.TimeoutMilliseconds);
        command.Parameters.AddWithValue("$stdout_preview", entry.StandardOutputPreview);
        command.Parameters.AddWithValue("$stderr_preview", entry.StandardErrorPreview);
        command.Parameters.AddWithValue("$stdout_bytes", entry.StandardOutputBytes);
        command.Parameters.AddWithValue("$stderr_bytes", entry.StandardErrorBytes);
        command.Parameters.AddWithValue("$stdout_truncated", entry.StandardOutputTruncated ? 1 : 0);
        command.Parameters.AddWithValue("$stderr_truncated", entry.StandardErrorTruncated ? 1 : 0);
        command.Parameters.AddWithValue("$approval_decision", entry.ApprovalDecision);
        command.Parameters.AddWithValue("$approval_reason", entry.ApprovalReason);
        command.Parameters.AddWithValue("$error", (object?)entry.Error ?? DBNull.Value);
        command.Parameters.AddWithValue("$created_at", entry.CreatedAt.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task<IReadOnlyList<TerminalCommandHistoryEntry>> ReadAsync(
        int limit = 1_000,
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT id, request_id, workspace_id, source, command, working_directory,
                   success, exit_code, timed_out, timeout_ms, stdout_preview, stderr_preview,
                   stdout_bytes, stderr_bytes, stdout_truncated, stderr_truncated,
                   approval_decision, approval_reason, error, created_at
            FROM connector_command_history
            ORDER BY created_at DESC
            LIMIT $limit;
            """;
        command.Parameters.AddWithValue("$limit", Math.Clamp(limit, 1, 1_000));
        var entries = new List<TerminalCommandHistoryEntry>();
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            entries.Add(new TerminalCommandHistoryEntry(
                reader.GetString(0),
                reader.GetString(1),
                reader.GetString(2),
                reader.GetString(3),
                reader.GetString(4),
                reader.GetString(5),
                reader.GetInt64(6) != 0,
                reader.IsDBNull(7) ? null : reader.GetInt32(7),
                reader.GetInt64(8) != 0,
                reader.GetInt32(9),
                reader.GetString(10),
                reader.GetString(11),
                reader.GetInt64(12),
                reader.GetInt64(13),
                reader.GetInt64(14) != 0,
                reader.GetInt64(15) != 0,
                reader.GetString(16),
                reader.GetString(17),
                reader.IsDBNull(18) ? null : reader.GetString(18),
                DateTimeOffset.Parse(reader.GetString(19))));
        }

        return entries;
    }
}
