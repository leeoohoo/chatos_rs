using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Persistence;

public sealed class SqliteConnectorModelSettingsStore(LocalStateDatabase database)
    : IConnectorModelSettingsStore
{
    public async Task<ConnectorModelSettings> LoadAsync(
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT model_request_max_retries, command_approval_model_config_id
            FROM connector_model_settings
            WHERE singleton_id = 1;
            """;
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        if (!await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            return ConnectorModelSettings.Default;
        }

        return new ConnectorModelSettings(
            reader.GetInt32(0),
            reader.IsDBNull(1) ? null : reader.GetString(1)).Normalize();
    }

    public async Task SaveAsync(
        ConnectorModelSettings settings,
        CancellationToken cancellationToken = default)
    {
        settings = settings.Normalize();
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO connector_model_settings(
                singleton_id, model_request_max_retries,
                command_approval_model_config_id, updated_at)
            VALUES (1, $retries, $approval_model, $updated_at)
            ON CONFLICT(singleton_id) DO UPDATE SET
                model_request_max_retries = excluded.model_request_max_retries,
                command_approval_model_config_id = excluded.command_approval_model_config_id,
                updated_at = excluded.updated_at;
            """;
        command.Parameters.AddWithValue("$retries", settings.ModelRequestMaxRetries);
        command.Parameters.AddWithValue(
            "$approval_model",
            (object?)settings.CommandApprovalModelConfigId ?? DBNull.Value);
        command.Parameters.AddWithValue("$updated_at", DateTimeOffset.UtcNow.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
