using ChatOS.Connector.NetworkGuard;
using ChatOS.Connector.Sandbox;

namespace ChatOS.Connector.Persistence;

public sealed class SqliteConnectorSandboxSettingsStore(
    LocalStateDatabase database,
    IControlledNetworkGuardClient? networkGuard = null)
    : IConnectorSandboxSettingsStore
{
    public async Task<ConnectorSandboxSettings> LoadAsync(
        CancellationToken cancellationToken = default)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT enabled, permission_profile, network_access
            FROM connector_sandbox_settings
            WHERE singleton_id = 1;
            """;
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        if (!await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            return ConnectorSandboxSettings.Default;
        }

        if (!Enum.TryParse<ConnectorSandboxPermissionProfile>(reader.GetString(1), out var profile) ||
            !Enum.TryParse<ConnectorSandboxNetworkAccess>(reader.GetString(2), out var network))
        {
            return ConnectorSandboxSettings.Default;
        }

        return new ConnectorSandboxSettings(reader.GetBoolean(0), profile, network).Normalize();
    }

    public async Task SaveAsync(
        ConnectorSandboxSettings settings,
        CancellationToken cancellationToken = default)
    {
        if (settings.NetworkAccess is ConnectorSandboxNetworkAccess.Controlled)
        {
            var readiness = networkGuard is null
                ? new NetworkGuardReadiness(NetworkGuardReadinessState.ServiceUnavailable)
                : await networkGuard.CheckReadinessAsync(cancellationToken).ConfigureAwait(false);
            if (!readiness.IsReady)
            {
                throw new InvalidOperationException(
                    $"Controlled-domain networking is unavailable ({readiness.State}).");
            }
        }

        settings = settings.Normalize();
        await using var connection = await database.OpenConnectionAsync(cancellationToken)
            .ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO connector_sandbox_settings(
                singleton_id, enabled, permission_profile, network_access, updated_at)
            VALUES (1, $enabled, $profile, $network, $updated_at)
            ON CONFLICT(singleton_id) DO UPDATE SET
                enabled = excluded.enabled,
                permission_profile = excluded.permission_profile,
                network_access = excluded.network_access,
                updated_at = excluded.updated_at;
            """;
        command.Parameters.AddWithValue("$enabled", settings.Enabled);
        command.Parameters.AddWithValue("$profile", settings.PermissionProfile.ToString());
        command.Parameters.AddWithValue("$network", settings.NetworkAccess.ToString());
        command.Parameters.AddWithValue("$updated_at", DateTimeOffset.UtcNow.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }
}
