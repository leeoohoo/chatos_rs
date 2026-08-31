using System.Net;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.Security;

namespace ChatOS.Connector.Tests;

public sealed class PluginOAuthBrokerTests : IDisposable
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        $"chatos-plugin-oauth-{Guid.NewGuid():N}");

    [Fact]
    public async Task PkceCallbackStoresTokensOnlyInCredentialVaultAndRefreshesThem()
    {
        var record = CreateInstallation();
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        var secrets = new MemorySecrets();
        var tokenRequests = new List<Dictionary<string, string>>();
        var tokenCall = 0;
        var handler = new DelegateHandler(async request =>
        {
            tokenRequests.Add(await FormAsync(request.Content!));
            tokenCall++;
            return Json(HttpStatusCode.OK, tokenCall == 1
                ? """{"access_token":"access-one","refresh_token":"refresh-one","expires_in":0,"scope":"demo:read","token_type":"Bearer"}"""
                : """{"access_token":"access-two","refresh_token":"refresh-two","expires_in":3600,"scope":"demo:read","token_type":"Bearer"}""");
        });
        var launcher = new CapturingLauncher();
        var vault = new PluginCredentialVault(secrets, new SqlitePluginCredentialMetadataStore(database));
        var broker = new PluginOAuthBroker(
            new InstalledStore(record),
            vault,
            new SqlitePluginOAuthConnectionStore(database),
            new Factory(new HttpClient(handler)),
            launcher);

        var started = await broker.BeginAuthorizationAsync(
            "owner-1", "device-1", "plugin-1", "release-1", "demo-app");

        Assert.True(started.BrowserOpened);
        Assert.Equal(started.AuthorizationUrl, launcher.Uri);
        var query = ParseQuery(started.AuthorizationUrl.Query);
        Assert.Equal("S256", query["code_challenge_method"]);
        Assert.False(string.IsNullOrWhiteSpace(query["code_challenge"]));
        var callback = new UriBuilder(query["redirect_uri"])
        {
            Query = $"state={Uri.EscapeDataString(query["state"])}&code=authorization-code",
        }.Uri;
        using var callbackClient = new HttpClient();
        var callbackHtml = await callbackClient.GetStringAsync(callback);

        Assert.Contains("Authorization completed", callbackHtml, StringComparison.Ordinal);
        var connection = Assert.Single(await broker.ListConnectionsAsync("owner-1", "device-1", "plugin-1"));
        Assert.True(connection.Connected);
        Assert.Equal(["demo:read"], connection.Scopes);
        Assert.Equal("authorization_code", tokenRequests[0]["grant_type"]);
        Assert.Equal(query["redirect_uri"], tokenRequests[0]["redirect_uri"]);
        Assert.False(string.IsNullOrWhiteSpace(tokenRequests[0]["code_verifier"]));

        var binding = await broker.PrepareTokenBindingAsync(
            "owner-1", "device-1", "plugin-1", "release-1", "resource-demo");
        var refreshed = await broker.ResolveAccessTokenAsync(binding);

        Assert.Equal("access-two", refreshed);
        Assert.Equal(connection.Id, binding.ConnectionId);
        Assert.Equal(64, binding.SnapshotSha256.Length);
        Assert.Equal("refresh_token", tokenRequests[1]["grant_type"]);
        Assert.Equal("refresh-one", tokenRequests[1]["refresh_token"]);
        Assert.DoesNotContain("access-one", await ReadAllMetadataAsync(database), StringComparison.Ordinal);
        Assert.DoesNotContain("refresh-one", await ReadAllMetadataAsync(database), StringComparison.Ordinal);
        Assert.Contains(secrets.Values, pair => pair.Value == "access-two");
        Assert.Contains(secrets.Values, pair => pair.Value == "refresh-two");
    }

    [Fact]
    public async Task CallbackStateIsConsumedOnceAndDisconnectPurgesTokens()
    {
        var record = CreateInstallation();
        var database = new LocalStateDatabase(Path.Combine(_directory, "state-second.db"));
        await database.InitializeAsync();
        var secrets = new MemorySecrets();
        var broker = new PluginOAuthBroker(
            new InstalledStore(record),
            new PluginCredentialVault(secrets, new SqlitePluginCredentialMetadataStore(database)),
            new SqlitePluginOAuthConnectionStore(database),
            new Factory(new HttpClient(new DelegateHandler(_ => Task.FromResult(Json(
                HttpStatusCode.OK,
                """{"access_token":"access","refresh_token":"refresh","expires_in":3600,"token_type":"Bearer"}"""))))),
            new CapturingLauncher());
        var started = await broker.BeginAuthorizationAsync(
            "owner-1", "device-1", "plugin-1", "release-1", "demo-app");
        var query = ParseQuery(started.AuthorizationUrl.Query);
        var callback = new UriBuilder(query["redirect_uri"])
        {
            Query = $"state={Uri.EscapeDataString(query["state"])}&code=one-time",
        }.Uri;
        using var client = new HttpClient();
        _ = await client.GetStringAsync(callback);
        var connection = Assert.Single(await broker.ListConnectionsAsync("owner-1", "device-1", "plugin-1"));

        await broker.DisconnectAsync(connection.Id);

        Assert.Empty(await broker.ListConnectionsAsync("owner-1", "device-1", "plugin-1"));
        Assert.Empty(secrets.Values);
    }

    public void Dispose()
    {
        try
        {
            if (Directory.Exists(_directory))
            {
                Directory.Delete(_directory, recursive: true);
            }
        }
        catch (IOException)
        {
        }
    }

    private InstalledPluginRecord CreateInstallation()
    {
        var installation = Path.Combine(_directory, $"installed-{Guid.NewGuid():N}");
        Directory.CreateDirectory(Path.Combine(installation, "oauth"));
        var manifest = """{"schemaVersion":3,"name":"oauth-plugin","version":"1.0.0","apps":[{"component_key":"demo-app","manifest":{"path":"oauth/demo.json"}}],"mcpServers":{},"permissions":[]}""";
        var app = """{"schemaVersion":1,"provider":"demo","clientId":"client-1","authorizationUrl":"https://oauth.example/authorize","tokenUrl":"https://oauth.example/token","resource":"resource-demo","scopes":["demo:read"],"callbackType":"loopback","authorizationParams":{"audience":"resource-demo"}}""";
        File.WriteAllText(Path.Combine(installation, "chatos.plugin.json"), manifest);
        File.WriteAllText(Path.Combine(installation, "oauth", "demo.json"), app);
        return new InstalledPluginRecord(
            "plugin-1",
            "release-1",
            "1.0.0",
            new string('a', 64),
            installation,
            DateTimeOffset.UtcNow,
            ["oauth.scope:demo:demo:read"],
            new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["chatos.plugin.json"] = Sha256(manifest),
                ["oauth/demo.json"] = Sha256(app),
            });
    }

    private static async Task<string> ReadAllMetadataAsync(LocalStateDatabase database)
    {
        await using var connection = await database.OpenConnectionAsync();
        var command = connection.CreateCommand();
        command.CommandText = """
            SELECT group_concat(plugin_id || release_id || component_key || secret_name, '|')
            FROM plugin_credential_metadata;
            """;
        return await command.ExecuteScalarAsync() as string ?? string.Empty;
    }

    private static async Task<Dictionary<string, string>> FormAsync(HttpContent content)
    {
        var value = await content.ReadAsStringAsync();
        return ParseQuery(value);
    }

    private static Dictionary<string, string> ParseQuery(string value) =>
        value.TrimStart('?').Split('&', StringSplitOptions.RemoveEmptyEntries)
            .Select(pair => pair.Split('=', 2))
            .ToDictionary(
                pair => Uri.UnescapeDataString(pair[0].Replace('+', ' ')),
                pair => pair.Length > 1 ? Uri.UnescapeDataString(pair[1].Replace('+', ' ')) : string.Empty,
                StringComparer.Ordinal);

    private static HttpResponseMessage Json(HttpStatusCode status, string body) => new(status)
    {
        Content = new StringContent(body, Encoding.UTF8, "application/json"),
    };

    private static string Sha256(string value) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant();

    private sealed class InstalledStore(InstalledPluginRecord record) : IInstalledPluginStore
    {
        public Task<IReadOnlyList<InstalledPluginRecord>> ListAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<InstalledPluginRecord>>([record]);
        public Task<InstalledPluginRecord?> GetAsync(string pluginId, CancellationToken cancellationToken = default) =>
            Task.FromResult<InstalledPluginRecord?>(pluginId == record.PluginId ? record : null);
        public Task SaveAsync(InstalledPluginRecord value, CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task DeleteAsync(string pluginId, CancellationToken cancellationToken = default) => Task.CompletedTask;
    }

    private sealed class MemorySecrets : IConnectorSecretStore
    {
        public Dictionary<string, string> Values { get; } = new(StringComparer.Ordinal);
        public ValueTask<string?> GetAsync(string key, CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(Values.GetValueOrDefault(key));
        public ValueTask SetAsync(string key, string value, CancellationToken cancellationToken = default)
        {
            Values[key] = value;
            return ValueTask.CompletedTask;
        }
        public ValueTask DeleteAsync(string key, CancellationToken cancellationToken = default)
        {
            Values.Remove(key);
            return ValueTask.CompletedTask;
        }
    }

    private sealed class CapturingLauncher : IExternalUriLauncher
    {
        public Uri? Uri { get; private set; }
        public Task LaunchAsync(Uri uri, CancellationToken cancellationToken = default)
        {
            Uri = uri;
            return Task.CompletedTask;
        }
    }

    private sealed class Factory(HttpClient client) : IHttpClientFactory
    {
        public HttpClient CreateClient(string name) => client;
    }

    private sealed class DelegateHandler(
        Func<HttpRequestMessage, Task<HttpResponseMessage>> handler) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken) => handler(request);
    }
}
