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

public sealed class PluginOAuthBroker
{
    internal const string HttpClientName = "ChatOS.PluginOAuth";
    private const int MaximumAppBytes = 256 * 1024;
    private const int MaximumTokenResponseBytes = 1024 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };
    private readonly IInstalledPluginStore _installed;
    private readonly PluginCredentialVault _credentials;
    private readonly IPluginOAuthConnectionStore _connections;
    private readonly IHttpClientFactory _httpClients;
    private readonly IExternalUriLauncher _launcher;
    private readonly ConcurrentDictionary<string, PendingTransaction> _pending = new(StringComparer.Ordinal);
    private readonly ConcurrentDictionary<string, SemaphoreSlim> _refreshLocks = new(StringComparer.Ordinal);

    public PluginOAuthBroker(
        IInstalledPluginStore installed,
        PluginCredentialVault credentials,
        IPluginOAuthConnectionStore connections,
        IHttpClientFactory httpClients,
        IExternalUriLauncher launcher)
    {
        _installed = installed;
        _credentials = credentials;
        _connections = connections;
        _httpClients = httpClients;
        _launcher = launcher;
    }

    public async Task<PluginOAuthAuthorizationStart> BeginAuthorizationAsync(
        string ownerUserId,
        string deviceId,
        string pluginId,
        string releaseId,
        string componentKey,
        CancellationToken cancellationToken = default)
    {
        var record = await RequireInstallationAsync(pluginId, releaseId, cancellationToken).ConfigureAwait(false);
        var app = await LoadAppAsync(record, componentKey, cancellationToken).ConfigureAwait(false);
        var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start(1);
        var port = ((IPEndPoint)listener.LocalEndpoint).Port;
        var redirectUri = new Uri($"http://127.0.0.1:{port}/oauth/callback");
        var state = RandomUrlSafe(32);
        var verifier = RandomUrlSafe(48);
        var challenge = Base64Url(SHA256.HashData(Encoding.ASCII.GetBytes(verifier)));
        var transactionId = Guid.NewGuid().ToString("D");
        var expiresAt = DateTimeOffset.UtcNow.AddMinutes(10);
        var authorizationUrl = BuildAuthorizationUrl(app, redirectUri, state, challenge);
        var pending = new PendingTransaction(
            transactionId,
            ownerUserId,
            deviceId,
            record,
            componentKey,
            redirectUri,
            verifier,
            app,
            expiresAt,
            listener);
        if (!_pending.TryAdd(state, pending))
        {
            listener.Stop();
            throw new PluginRuntimeException("Plugin OAuth state collision.");
        }

        _ = ObserveCallbackAsync(state, pending);
        var browserOpened = true;
        string? browserError = null;
        try
        {
            await _launcher.LaunchAsync(authorizationUrl, cancellationToken).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            browserOpened = false;
            browserError = exception.Message;
        }

        return new PluginOAuthAuthorizationStart(
            transactionId,
            authorizationUrl,
            expiresAt,
            browserOpened,
            browserError);
    }

    public async Task<IReadOnlyList<PluginOAuthConnection>> ListConnectionsAsync(
        string ownerUserId,
        string deviceId,
        string pluginId,
        CancellationToken cancellationToken = default)
    {
        var values = await _connections.ListAsync(
            ownerUserId, deviceId, pluginId, cancellationToken).ConfigureAwait(false);
        foreach (var value in values)
        {
            await VerifySealAsync(value, cancellationToken).ConfigureAwait(false);
        }

        return values;
    }

    public async Task<string> GetAccessTokenAsync(
        string connectionId,
        CancellationToken cancellationToken = default)
    {
        var gate = _refreshLocks.GetOrAdd(connectionId, static _ => new SemaphoreSlim(1, 1));
        await gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var connection = await _connections.GetAsync(connectionId, cancellationToken).ConfigureAwait(false)
                ?? throw new PluginRuntimeException("Plugin OAuth connection was not found.");
            await VerifySealAsync(connection, cancellationToken).ConfigureAwait(false);
            if (!connection.Connected || connection.NeedsAuth)
            {
                throw new PluginRuntimeException("Plugin OAuth connection requires authorization.");
            }

            if (connection.ExpiresAt is null || connection.ExpiresAt > DateTimeOffset.UtcNow.AddMinutes(5))
            {
                return await _credentials.ResolveAsync(TokenScope(connection, "oauth.access_token"), cancellationToken)
                    .ConfigureAwait(false);
            }

            OAuthTokenResponse token;
            try
            {
                var refresh = await _credentials.ResolveAsync(
                    TokenScope(connection, "oauth.refresh_token"),
                    cancellationToken).ConfigureAwait(false);
                var record = await RequireInstallationAsync(
                    connection.PluginId,
                    connection.ReleaseId,
                    cancellationToken).ConfigureAwait(false);
                var app = await LoadAppAsync(record, connection.ComponentKey, cancellationToken).ConfigureAwait(false);
                token = await RequestTokenAsync(app.TokenUrl, new Dictionary<string, string>
                {
                    ["grant_type"] = "refresh_token",
                    ["client_id"] = app.ClientId,
                    ["refresh_token"] = refresh,
                }, cancellationToken).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                throw;
            }
            catch
            {
                await MarkNeedsAuthAsync(connection, cancellationToken).ConfigureAwait(false);
                throw;
            }

            var updated = connection with
            {
                Scopes = NormalizeScopes(token.Scope?.Split(' ', StringSplitOptions.RemoveEmptyEntries) ?? connection.Scopes),
                ExpiresAt = token.ExpiresIn is null ? null : DateTimeOffset.UtcNow.AddSeconds(token.ExpiresIn.Value),
                UpdatedAt = DateTimeOffset.UtcNow,
            };
            await PersistTokensAsync(updated, token, keepExistingRefresh: true, cancellationToken).ConfigureAwait(false);
            return token.AccessToken;
        }
        finally
        {
            gate.Release();
        }
    }

    internal async Task<PluginOAuthTokenBinding> PrepareTokenBindingAsync(
        string ownerUserId,
        string deviceId,
        string pluginId,
        string releaseId,
        string resource,
        CancellationToken cancellationToken = default)
    {
        var connections = await ListConnectionsAsync(
            ownerUserId,
            deviceId,
            pluginId,
            cancellationToken).ConfigureAwait(false);
        var matches = connections.Where(connection =>
                connection.Connected &&
                !connection.NeedsAuth &&
                string.Equals(connection.ReleaseId, releaseId, StringComparison.Ordinal) &&
                string.Equals(connection.Resource, resource, StringComparison.Ordinal))
            .ToArray();
        if (matches.Length != 1)
        {
            throw new PluginRuntimeException(
                "Plugin OAuth resource requires exactly one active local connection.");
        }

        var connection = matches[0];
        return new PluginOAuthTokenBinding(
            connection.Id,
            connection.Provider,
            connection.Resource,
            connection.Scopes,
            ConnectionBindingSnapshot(connection));
    }

    internal async Task<string> ResolveAccessTokenAsync(
        PluginOAuthTokenBinding binding,
        CancellationToken cancellationToken = default)
    {
        var connection = await _connections.GetAsync(binding.ConnectionId, cancellationToken).ConfigureAwait(false)
            ?? throw new PluginRuntimeException("Plugin OAuth connection no longer exists.");
        await VerifySealAsync(connection, cancellationToken).ConfigureAwait(false);
        var current = ConnectionBindingSnapshot(connection);
        if (!connection.Connected || connection.NeedsAuth ||
            !CryptographicOperations.FixedTimeEquals(
                Encoding.ASCII.GetBytes(binding.SnapshotSha256),
                Encoding.ASCII.GetBytes(current)))
        {
            throw new PluginRuntimeException("Plugin OAuth connection changed after prepare.");
        }

        return await GetAccessTokenAsync(binding.ConnectionId, cancellationToken).ConfigureAwait(false);
    }

    public async Task DisconnectAsync(string connectionId, CancellationToken cancellationToken = default)
    {
        var connection = await _connections.GetAsync(connectionId, cancellationToken).ConfigureAwait(false);
        if (connection is null)
        {
            return;
        }

        foreach (var name in new[] { "oauth.access_token", "oauth.refresh_token", "oauth.connection_snapshot" })
        {
            await _credentials.DeleteAsync(TokenScope(connection, name), cancellationToken).ConfigureAwait(false);
        }

        await _connections.DeleteAsync(connectionId, cancellationToken).ConfigureAwait(false);
    }

    public async Task PurgePluginAsync(
        string ownerUserId,
        string deviceId,
        string pluginId,
        CancellationToken cancellationToken = default)
    {
        var connections = await _connections.ListAsync(
            ownerUserId,
            deviceId,
            pluginId,
            cancellationToken).ConfigureAwait(false);
        foreach (var connection in connections)
        {
            await DisconnectAsync(connection.Id, cancellationToken).ConfigureAwait(false);
        }
    }

    private async Task ObserveCallbackAsync(string state, PendingTransaction pending)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromMinutes(10));
        try
        {
            using var client = await pending.Listener.AcceptTcpClientAsync(timeout.Token).ConfigureAwait(false);
            var query = await ReadCallbackAsync(client, timeout.Token).ConfigureAwait(false);
            if (!_pending.TryRemove(state, out var consumed) || !ReferenceEquals(consumed, pending))
            {
                await WriteCallbackAsync(client, false, "OAuth state has already been consumed.", timeout.Token)
                    .ConfigureAwait(false);
                return;
            }

            if (!query.TryGetValue("state", out var returnedState) ||
                !CryptographicOperations.FixedTimeEquals(
                    Encoding.UTF8.GetBytes(state),
                    Encoding.UTF8.GetBytes(returnedState)))
            {
                await WriteCallbackAsync(client, false, "OAuth state is invalid.", timeout.Token).ConfigureAwait(false);
                return;
            }

            if (query.TryGetValue("error", out var error))
            {
                var description = query.GetValueOrDefault("error_description");
                await WriteCallbackAsync(
                    client,
                    false,
                    error == "access_denied" ? "Authorization was denied." : description ?? error,
                    timeout.Token).ConfigureAwait(false);
                return;
            }

            if (!query.TryGetValue("code", out var code) || string.IsNullOrWhiteSpace(code))
            {
                await WriteCallbackAsync(client, false, "OAuth authorization code is missing.", timeout.Token)
                    .ConfigureAwait(false);
                return;
            }

            var token = await RequestTokenAsync(pending.App.TokenUrl, new Dictionary<string, string>
            {
                ["grant_type"] = "authorization_code",
                ["client_id"] = pending.App.ClientId,
                ["code"] = code,
                ["redirect_uri"] = pending.RedirectUri.AbsoluteUri,
                ["code_verifier"] = pending.CodeVerifier,
            }, timeout.Token).ConfigureAwait(false);
            var scopes = NormalizeScopes(token.Scope?.Split(' ', StringSplitOptions.RemoveEmptyEntries)
                ?? pending.App.Scopes);
            var connection = new PluginOAuthConnection(
                ConnectionId(
                    pending.OwnerUserId,
                    pending.DeviceId,
                    pending.Record.PluginId,
                    pending.ComponentKey,
                    pending.App.Provider),
                pending.OwnerUserId,
                pending.DeviceId,
                pending.Record.PluginId,
                pending.Record.ReleaseId,
                pending.ComponentKey,
                pending.App.Provider,
                pending.App.Resource,
                scopes,
                Connected: true,
                NeedsAuth: false,
                token.ExpiresIn is null ? null : DateTimeOffset.UtcNow.AddSeconds(token.ExpiresIn.Value),
                AccountDisplay: null,
                DateTimeOffset.UtcNow);
            await PersistTokensAsync(connection, token, keepExistingRefresh: false, timeout.Token)
                .ConfigureAwait(false);
            await WriteCallbackAsync(client, true, "Authorization completed. You can close this window.", timeout.Token)
                .ConfigureAwait(false);
        }
        catch
        {
            _pending.TryRemove(state, out _);
        }
        finally
        {
            pending.Listener.Stop();
        }
    }

    private async Task PersistTokensAsync(
        PluginOAuthConnection connection,
        OAuthTokenResponse token,
        bool keepExistingRefresh,
        CancellationToken cancellationToken)
    {
        var accessScope = TokenScope(connection, "oauth.access_token");
        var refreshScope = TokenScope(connection, "oauth.refresh_token");
        var snapshotScope = TokenScope(connection, "oauth.connection_snapshot");
        var previousConnection = await _connections.GetAsync(connection.Id, cancellationToken).ConfigureAwait(false);
        var previousAccess = await TryResolveAsync(accessScope, cancellationToken).ConfigureAwait(false);
        var previousRefresh = await TryResolveAsync(refreshScope, cancellationToken).ConfigureAwait(false);
        var previousSnapshot = await TryResolveAsync(snapshotScope, cancellationToken).ConfigureAwait(false);
        try
        {
            await _credentials.UpsertAsync(
                accessScope,
                token.AccessToken,
                cancellationToken).ConfigureAwait(false);
            if (!string.IsNullOrWhiteSpace(token.RefreshToken))
            {
                await _credentials.UpsertAsync(
                    refreshScope,
                    token.RefreshToken,
                    cancellationToken).ConfigureAwait(false);
            }
            else if (!keepExistingRefresh)
            {
                await _credentials.DeleteAsync(refreshScope, cancellationToken).ConfigureAwait(false);
            }

            await _connections.SaveAsync(connection, cancellationToken).ConfigureAwait(false);
            await _credentials.UpsertAsync(
                snapshotScope,
                ConnectionSnapshot(connection),
                cancellationToken).ConfigureAwait(false);
        }
        catch
        {
            await RestoreCredentialAsync(accessScope, previousAccess).ConfigureAwait(false);
            await RestoreCredentialAsync(refreshScope, previousRefresh).ConfigureAwait(false);
            await RestoreCredentialAsync(snapshotScope, previousSnapshot).ConfigureAwait(false);
            if (previousConnection is null)
            {
                await _connections.DeleteAsync(connection.Id, CancellationToken.None).ConfigureAwait(false);
            }
            else
            {
                await _connections.SaveAsync(previousConnection, CancellationToken.None).ConfigureAwait(false);
            }

            throw;
        }
    }

    private async Task<string?> TryResolveAsync(
        PluginCredentialScope scope,
        CancellationToken cancellationToken)
    {
        try
        {
            return await _credentials.ResolveAsync(scope, cancellationToken).ConfigureAwait(false);
        }
        catch (PluginRuntimeException)
        {
            return null;
        }
    }

    private async Task RestoreCredentialAsync(PluginCredentialScope scope, string? value)
    {
        if (value is null)
        {
            await _credentials.DeleteAsync(scope, CancellationToken.None).ConfigureAwait(false);
        }
        else
        {
            await _credentials.UpsertAsync(scope, value, CancellationToken.None).ConfigureAwait(false);
        }
    }

    private async Task MarkNeedsAuthAsync(
        PluginOAuthConnection connection,
        CancellationToken cancellationToken)
    {
        foreach (var name in new[] { "oauth.access_token", "oauth.refresh_token" })
        {
            await _credentials.DeleteAsync(TokenScope(connection, name), cancellationToken).ConfigureAwait(false);
        }

        var updated = connection with
        {
            Connected = false,
            NeedsAuth = true,
            ExpiresAt = null,
            UpdatedAt = DateTimeOffset.UtcNow,
        };
        await _connections.SaveAsync(updated, cancellationToken).ConfigureAwait(false);
        await _credentials.UpsertAsync(
            TokenScope(updated, "oauth.connection_snapshot"),
            ConnectionSnapshot(updated),
            cancellationToken).ConfigureAwait(false);
    }

    private async Task VerifySealAsync(
        PluginOAuthConnection connection,
        CancellationToken cancellationToken)
    {
        var expected = await _credentials.ResolveAsync(
            TokenScope(connection, "oauth.connection_snapshot"),
            cancellationToken).ConfigureAwait(false);
        var actual = ConnectionSnapshot(connection);
        if (!CryptographicOperations.FixedTimeEquals(
                Encoding.UTF8.GetBytes(expected),
                Encoding.UTF8.GetBytes(actual)))
        {
            throw new PluginRuntimeException("Plugin OAuth connection metadata failed integrity validation.");
        }
    }

    private async Task<InstalledPluginRecord> RequireInstallationAsync(
        string pluginId,
        string releaseId,
        CancellationToken cancellationToken)
    {
        var record = await _installed.GetAsync(pluginId, cancellationToken).ConfigureAwait(false);
        return record is not null && record.ReleaseId == releaseId
            ? record
            : throw new PluginRuntimeException("Plugin OAuth request does not match an installed Release.");
    }

    private static async Task<OAuthAppManifest> LoadAppAsync(
        InstalledPluginRecord record,
        string componentKey,
        CancellationToken cancellationToken)
    {
        var manifest = await ReadJsonAsync<PluginManifest>(
            Path.Combine(record.InstallationPath, "chatos.plugin.json"),
            4 * 1024 * 1024,
            cancellationToken).ConfigureAwait(false);
        var appReference = manifest.Apps.FirstOrDefault(app => app.ComponentKey == componentKey)
            ?? throw new PluginRuntimeException("Plugin Connected App was not found.");
        var relative = NormalizeRelativePath(appReference.Manifest.Path);
        VerifyChecksum(record, relative);
        var app = await ReadJsonAsync<OAuthAppManifest>(
            Path.Combine(record.InstallationPath, relative.Replace('/', Path.DirectorySeparatorChar)),
            MaximumAppBytes,
            cancellationToken).ConfigureAwait(false);
        app.Validate();
        return app;
    }

    private async Task<OAuthTokenResponse> RequestTokenAsync(
        Uri tokenUrl,
        IReadOnlyDictionary<string, string> fields,
        CancellationToken cancellationToken)
    {
        using var request = new HttpRequestMessage(HttpMethod.Post, tokenUrl)
        {
            Content = new FormUrlEncodedContent(fields),
        };
        request.Headers.Accept.ParseAdd("application/json");
        using var response = await _httpClients.CreateClient(HttpClientName)
            .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            throw new PluginRuntimeException($"Plugin OAuth token endpoint failed with status {(int)response.StatusCode}.");
        }

        if (response.Content.Headers.ContentLength is > MaximumTokenResponseBytes)
        {
            throw new PluginRuntimeException("Plugin OAuth token response is too large.");
        }

        await using var stream = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        using var buffer = new MemoryStream();
        var bytes = new byte[16 * 1024];
        while (true)
        {
            var read = await stream.ReadAsync(bytes, cancellationToken).ConfigureAwait(false);
            if (read == 0)
            {
                break;
            }

            if (buffer.Length + read > MaximumTokenResponseBytes)
            {
                throw new PluginRuntimeException("Plugin OAuth token response is too large.");
            }

            buffer.Write(bytes, 0, read);
        }

        OAuthTokenResponse token;
        try
        {
            token = JsonSerializer.Deserialize<OAuthTokenResponse>(buffer.ToArray(), JsonOptions)
                ?? throw new JsonException("Token response is empty.");
        }
        catch (JsonException exception)
        {
            throw new PluginRuntimeException("Plugin OAuth token response is invalid.", exception);
        }

        token.Validate();
        return token;
    }

    private static Uri BuildAuthorizationUrl(
        OAuthAppManifest app,
        Uri redirectUri,
        string state,
        string challenge)
    {
        var values = new Dictionary<string, string>(app.AuthorizationParams, StringComparer.Ordinal)
        {
            ["response_type"] = "code",
            ["client_id"] = app.ClientId,
            ["redirect_uri"] = redirectUri.AbsoluteUri,
            ["state"] = state,
            ["code_challenge"] = challenge,
            ["code_challenge_method"] = "S256",
        };
        if (app.Scopes.Count > 0)
        {
            values["scope"] = string.Join(' ', app.Scopes);
        }

        var builder = new UriBuilder(app.AuthorizationUrl)
        {
            Query = string.Join('&', values.Select(pair =>
                $"{Uri.EscapeDataString(pair.Key)}={Uri.EscapeDataString(pair.Value)}")),
        };
        return builder.Uri;
    }

    private static async Task<Dictionary<string, string>> ReadCallbackAsync(
        TcpClient client,
        CancellationToken cancellationToken)
    {
        using var reader = new StreamReader(
            client.GetStream(),
            Encoding.ASCII,
            detectEncodingFromByteOrderMarks: false,
            bufferSize: 4 * 1024,
            leaveOpen: true);
        var requestLine = await reader.ReadLineAsync(cancellationToken).ConfigureAwait(false);
        if (requestLine is null || requestLine.Length > 8 * 1024)
        {
            throw new PluginRuntimeException("Plugin OAuth callback request is invalid.");
        }

        string? line;
        do
        {
            line = await reader.ReadLineAsync(cancellationToken).ConfigureAwait(false);
        } while (!string.IsNullOrEmpty(line));
        var parts = requestLine.Split(' ', 3);
        if (parts.Length != 3 || parts[0] != "GET" ||
            !Uri.TryCreate("http://127.0.0.1" + parts[1], UriKind.Absolute, out var uri))
        {
            throw new PluginRuntimeException("Plugin OAuth callback request is invalid.");
        }

        return uri.Query.TrimStart('?')
            .Split('&', StringSplitOptions.RemoveEmptyEntries)
            .Select(item => item.Split('=', 2))
            .ToDictionary(
                item => Uri.UnescapeDataString(item[0].Replace('+', ' ')),
                item => item.Length > 1 ? Uri.UnescapeDataString(item[1].Replace('+', ' ')) : string.Empty,
                StringComparer.Ordinal);
    }

    private static async Task WriteCallbackAsync(
        TcpClient client,
        bool success,
        string message,
        CancellationToken cancellationToken)
    {
        var safe = WebUtility.HtmlEncode(message);
        var html = $"<!doctype html><meta charset=\"utf-8\"><title>ChatOS OAuth</title><body><h1>{(success ? "Success" : "Authorization failed")}</h1><p>{safe}</p></body>";
        var body = Encoding.UTF8.GetBytes(html);
        var headers = Encoding.ASCII.GetBytes(
            $"HTTP/1.1 {(success ? "200 OK" : "400 Bad Request")}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {body.Length}\r\nConnection: close\r\n\r\n");
        var stream = client.GetStream();
        await stream.WriteAsync(headers, cancellationToken).ConfigureAwait(false);
        await stream.WriteAsync(body, cancellationToken).ConfigureAwait(false);
        await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
    }

    private static async Task<T> ReadJsonAsync<T>(string path, int limit, CancellationToken cancellationToken)
    {
        var info = new FileInfo(path);
        if (!info.Exists || info.Length <= 0 || info.Length > limit ||
            (info.Attributes & FileAttributes.ReparsePoint) != 0)
        {
            throw new PluginRuntimeException("Plugin OAuth manifest is missing or unsafe.");
        }

        await using var stream = File.OpenRead(path);
        return await JsonSerializer.DeserializeAsync<T>(stream, JsonOptions, cancellationToken)
            .ConfigureAwait(false) ?? throw new PluginRuntimeException("Plugin OAuth manifest is empty.");
    }

    private static void VerifyChecksum(InstalledPluginRecord record, string relative)
    {
        if (record.PackageFileSha256 is null || !record.PackageFileSha256.TryGetValue(relative, out var expected))
        {
            throw new PluginRuntimeException("Plugin OAuth manifest is not covered by installation checksums.");
        }

        var path = Path.Combine(record.InstallationPath, relative.Replace('/', Path.DirectorySeparatorChar));
        var actual = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path))).ToLowerInvariant();
        if (!CryptographicOperations.FixedTimeEquals(
                Convert.FromHexString(expected), Convert.FromHexString(actual)))
        {
            throw new PluginRuntimeException("Plugin OAuth manifest checksum changed after installation.");
        }
    }

    private static string NormalizeRelativePath(string? value)
    {
        var path = value?.Replace('\\', '/').Trim() ?? string.Empty;
        while (path.StartsWith("./", StringComparison.Ordinal))
        {
            path = path[2..];
        }

        var parts = path.Split('/', StringSplitOptions.None);
        if (parts.Length == 0 || parts.Any(part => part.Length == 0 || part is "." or ".."))
        {
            throw new PluginRuntimeException("Plugin OAuth manifest path is invalid.");
        }

        return string.Join('/', parts);
    }

    private static PluginCredentialScope TokenScope(PluginOAuthConnection connection, string name) => new(
        connection.OwnerUserId,
        connection.DeviceId,
        connection.PluginId,
        connection.ReleaseId,
        connection.ComponentKey,
        name);

    private static string ConnectionId(
        string ownerUserId,
        string deviceId,
        string pluginId,
        string componentKey,
        string provider) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(
            $"chatos.plugin.oauth.storage.v1\n{ownerUserId}\n{deviceId}\n{pluginId}\n{componentKey}\n{provider}")))
            .ToLowerInvariant();

    private static string ConnectionSnapshot(PluginOAuthConnection value) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(string.Join('\n', new[]
        {
            "chatos.plugin.oauth.connection.v1",
            value.Id,
            value.OwnerUserId,
            value.DeviceId,
            value.PluginId,
            value.ReleaseId,
            value.ComponentKey,
            value.Provider,
            value.Resource,
            string.Join(' ', value.Scopes.Order()),
            value.Connected.ToString(),
            value.NeedsAuth.ToString(),
            value.ExpiresAt?.ToString("O") ?? string.Empty,
            value.UpdatedAt.ToString("O"),
        })))).ToLowerInvariant();

    private static string ConnectionBindingSnapshot(PluginOAuthConnection value) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(string.Join('\n', new[]
        {
            "chatos.plugin.oauth.binding.v1",
            value.Id,
            value.OwnerUserId,
            value.DeviceId,
            value.PluginId,
            value.ReleaseId,
            value.ComponentKey,
            value.Provider,
            value.Resource,
            string.Join('\n', value.Scopes.Order()),
        })))).ToLowerInvariant();

    private static string RandomUrlSafe(int bytes)
    {
        var value = RandomNumberGenerator.GetBytes(bytes);
        return Base64Url(value);
    }

    private static string Base64Url(byte[] value) =>
        Convert.ToBase64String(value).TrimEnd('=').Replace('+', '-').Replace('/', '_');

    private static IReadOnlyList<string> NormalizeScopes(IEnumerable<string> values)
    {
        var scopes = values.Select(value => value.Trim()).Where(value => value.Length > 0).Distinct().Order().ToArray();
        if (scopes.Length > 64 || scopes.Any(scope =>
                scope.Length > 256 || scope.Any(character =>
                    !(char.IsAsciiLetterOrDigit(character) || character is '-' or '_' or '.' or ':' or '/' or '+' or '='))))
        {
            throw new PluginRuntimeException("Plugin OAuth scopes are invalid.");
        }

        return scopes;
    }

    private sealed record PendingTransaction(
        string TransactionId,
        string OwnerUserId,
        string DeviceId,
        InstalledPluginRecord Record,
        string ComponentKey,
        Uri RedirectUri,
        string CodeVerifier,
        OAuthAppManifest App,
        DateTimeOffset ExpiresAt,
        TcpListener Listener);

    private sealed record OAuthAppManifest
    {
        [System.Text.Json.Serialization.JsonPropertyName("schemaVersion")]
        public required int SchemaVersion { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("provider")]
        public required string Provider { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("clientId")]
        public required string ClientId { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("authorizationUrl")]
        public required Uri AuthorizationUrl { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("tokenUrl")]
        public required Uri TokenUrl { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("resource")]
        public required string Resource { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("scopes")]
        public IReadOnlyList<string> Scopes { get; init; } = [];
        [System.Text.Json.Serialization.JsonPropertyName("callbackType")]
        public required string CallbackType { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("authorizationParams")]
        public IReadOnlyDictionary<string, string> AuthorizationParams { get; init; } =
            new Dictionary<string, string>();

        public void Validate()
        {
            if (SchemaVersion != 1 || CallbackType != "loopback" ||
                string.IsNullOrWhiteSpace(Provider) || Provider.Length > 96 ||
                string.IsNullOrWhiteSpace(ClientId) || ClientId.Length > 512 ||
                string.IsNullOrWhiteSpace(Resource) || Resource.Length > 2048)
            {
                throw new PluginRuntimeException("Plugin OAuth app manifest is invalid.");
            }

            ValidateEndpoint(AuthorizationUrl);
            ValidateEndpoint(TokenUrl);
            _ = NormalizeScopes(Scopes);
            var reserved = new HashSet<string>(StringComparer.Ordinal)
            {
                "response_type", "client_id", "redirect_uri", "state", "scope",
                "code_challenge", "code_challenge_method",
            };
            if (AuthorizationParams.Count > 32 || AuthorizationParams.Keys.Any(reserved.Contains) ||
                AuthorizationParams.Any(pair =>
                    string.IsNullOrWhiteSpace(pair.Key) || pair.Key.Length > 96 ||
                    string.IsNullOrWhiteSpace(pair.Value) || pair.Value.Length > 2048))
            {
                throw new PluginRuntimeException("Plugin OAuth authorization parameters are invalid.");
            }
        }

        private static void ValidateEndpoint(Uri uri)
        {
            var loopback = IPAddress.TryParse(uri.Host, out var address) && IPAddress.IsLoopback(address) ||
                uri.Host.Equals("localhost", StringComparison.OrdinalIgnoreCase);
            if ((uri.Scheme != Uri.UriSchemeHttps && !(uri.Scheme == Uri.UriSchemeHttp && loopback)) ||
                !string.IsNullOrEmpty(uri.UserInfo) || !string.IsNullOrEmpty(uri.Fragment))
            {
                throw new PluginRuntimeException("Plugin OAuth endpoints require HTTPS except for loopback development.");
            }
        }
    }

    private sealed record OAuthTokenResponse
    {
        [System.Text.Json.Serialization.JsonPropertyName("access_token")]
        public required string AccessToken { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("refresh_token")]
        public string? RefreshToken { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("expires_in")]
        public long? ExpiresIn { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("scope")]
        public string? Scope { get; init; }
        [System.Text.Json.Serialization.JsonPropertyName("token_type")]
        public string? TokenType { get; init; }

        public void Validate()
        {
            if (string.IsNullOrWhiteSpace(AccessToken) || AccessToken.Length > 64 * 1024 ||
                RefreshToken?.Length > 64 * 1024 || ExpiresIn is < 0 or > 315_360_000 ||
                (TokenType is not null && !TokenType.Equals("Bearer", StringComparison.OrdinalIgnoreCase)))
            {
                throw new PluginRuntimeException("Plugin OAuth token response is invalid.");
            }
        }
    }
}
