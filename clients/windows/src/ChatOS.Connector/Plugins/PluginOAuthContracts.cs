using System.Collections.Concurrent;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Persistence;

namespace ChatOS.Connector.Plugins;

public sealed record PluginOAuthAuthorizationStart(
    string TransactionId,
    Uri AuthorizationUrl,
    DateTimeOffset ExpiresAt,
    bool BrowserOpened,
    string? BrowserError);

public sealed record PluginOAuthConnection(
    string Id,
    string OwnerUserId,
    string DeviceId,
    string PluginId,
    string ReleaseId,
    string ComponentKey,
    string Provider,
    string Resource,
    IReadOnlyList<string> Scopes,
    bool Connected,
    bool NeedsAuth,
    DateTimeOffset? ExpiresAt,
    string? AccountDisplay,
    DateTimeOffset UpdatedAt);

public interface IExternalUriLauncher
{
    Task LaunchAsync(Uri uri, CancellationToken cancellationToken = default);
}

internal sealed class WindowsExternalUriLauncher : IExternalUriLauncher
{
    public Task LaunchAsync(Uri uri, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        System.Diagnostics.Process.Start(new System.Diagnostics.ProcessStartInfo(uri.AbsoluteUri)
        {
            UseShellExecute = true,
        });
        return Task.CompletedTask;
    }
}

public interface IPluginOAuthConnectionStore
{
    Task<IReadOnlyList<PluginOAuthConnection>> ListAsync(
        string ownerUserId,
        string deviceId,
        string pluginId,
        CancellationToken cancellationToken);
    Task<PluginOAuthConnection?> GetAsync(string id, CancellationToken cancellationToken);
    Task SaveAsync(PluginOAuthConnection connection, CancellationToken cancellationToken);
    Task DeleteAsync(string id, CancellationToken cancellationToken);
}

internal sealed class SqlitePluginOAuthConnectionStore(LocalStateDatabase database) :
    IPluginOAuthConnectionStore
{
    public async Task<IReadOnlyList<PluginOAuthConnection>> ListAsync(
        string ownerUserId,
        string deviceId,
        string pluginId,
        CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT id, release_id, component_key, provider, resource, scopes_json,
                   connected, needs_auth, expires_at, account_display, updated_at
            FROM plugin_oauth_connection
            WHERE owner_user_id = $owner_user_id
              AND device_id = $device_id
              AND plugin_id = $plugin_id
            ORDER BY component_key, provider;
            """;
        command.Parameters.AddWithValue("$owner_user_id", ownerUserId);
        command.Parameters.AddWithValue("$device_id", deviceId);
        command.Parameters.AddWithValue("$plugin_id", pluginId);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        var result = new List<PluginOAuthConnection>();
        while (await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            result.Add(Read(reader, ownerUserId, deviceId, pluginId));
        }

        return result;
    }

    public async Task<PluginOAuthConnection?> GetAsync(string id, CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT owner_user_id, device_id, plugin_id, release_id, component_key,
                   provider, resource, scopes_json, connected, needs_auth,
                   expires_at, account_display, updated_at
            FROM plugin_oauth_connection
            WHERE id = $id
            LIMIT 1;
            """;
        command.Parameters.AddWithValue("$id", id);
        await using var reader = await command.ExecuteReaderAsync(cancellationToken).ConfigureAwait(false);
        if (!await reader.ReadAsync(cancellationToken).ConfigureAwait(false))
        {
            return null;
        }

        return new PluginOAuthConnection(
            id,
            reader.GetString(0),
            reader.GetString(1),
            reader.GetString(2),
            reader.GetString(3),
            reader.GetString(4),
            reader.GetString(5),
            reader.GetString(6),
            JsonSerializer.Deserialize<string[]>(reader.GetString(7)) ?? [],
            reader.GetBoolean(8),
            reader.GetBoolean(9),
            reader.IsDBNull(10) ? null : DateTimeOffset.Parse(reader.GetString(10)),
            reader.IsDBNull(11) ? null : reader.GetString(11),
            DateTimeOffset.Parse(reader.GetString(12)));
    }

    public async Task SaveAsync(PluginOAuthConnection value, CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = """
            INSERT INTO plugin_oauth_connection(
                id, owner_user_id, device_id, plugin_id, release_id, component_key,
                provider, resource, scopes_json, connected, needs_auth, expires_at,
                account_display, updated_at)
            VALUES (
                $id, $owner_user_id, $device_id, $plugin_id, $release_id, $component_key,
                $provider, $resource, $scopes_json, $connected, $needs_auth, $expires_at,
                $account_display, $updated_at)
            ON CONFLICT(id) DO UPDATE SET
                release_id = excluded.release_id,
                resource = excluded.resource,
                scopes_json = excluded.scopes_json,
                connected = excluded.connected,
                needs_auth = excluded.needs_auth,
                expires_at = excluded.expires_at,
                account_display = excluded.account_display,
                updated_at = excluded.updated_at;
            """;
        command.Parameters.AddWithValue("$id", value.Id);
        command.Parameters.AddWithValue("$owner_user_id", value.OwnerUserId);
        command.Parameters.AddWithValue("$device_id", value.DeviceId);
        command.Parameters.AddWithValue("$plugin_id", value.PluginId);
        command.Parameters.AddWithValue("$release_id", value.ReleaseId);
        command.Parameters.AddWithValue("$component_key", value.ComponentKey);
        command.Parameters.AddWithValue("$provider", value.Provider);
        command.Parameters.AddWithValue("$resource", value.Resource);
        command.Parameters.AddWithValue("$scopes_json", JsonSerializer.Serialize(value.Scopes));
        command.Parameters.AddWithValue("$connected", value.Connected);
        command.Parameters.AddWithValue("$needs_auth", value.NeedsAuth);
        command.Parameters.AddWithValue("$expires_at", (object?)value.ExpiresAt?.ToString("O") ?? DBNull.Value);
        command.Parameters.AddWithValue("$account_display", (object?)value.AccountDisplay ?? DBNull.Value);
        command.Parameters.AddWithValue("$updated_at", value.UpdatedAt.ToString("O"));
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    public async Task DeleteAsync(string id, CancellationToken cancellationToken)
    {
        await using var connection = await database.OpenConnectionAsync(cancellationToken).ConfigureAwait(false);
        var command = connection.CreateCommand();
        command.CommandText = "DELETE FROM plugin_oauth_connection WHERE id = $id;";
        command.Parameters.AddWithValue("$id", id);
        await command.ExecuteNonQueryAsync(cancellationToken).ConfigureAwait(false);
    }

    private static PluginOAuthConnection Read(
        Microsoft.Data.Sqlite.SqliteDataReader reader,
        string ownerUserId,
        string deviceId,
        string pluginId) => new(
        reader.GetString(0),
        ownerUserId,
        deviceId,
        pluginId,
        reader.GetString(1),
        reader.GetString(2),
        reader.GetString(3),
        reader.GetString(4),
        JsonSerializer.Deserialize<string[]>(reader.GetString(5)) ?? [],
        reader.GetBoolean(6),
        reader.GetBoolean(7),
        reader.IsDBNull(8) ? null : DateTimeOffset.Parse(reader.GetString(8)),
        reader.IsDBNull(9) ? null : reader.GetString(9),
        DateTimeOffset.Parse(reader.GetString(10)));
}
