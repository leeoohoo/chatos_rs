using System.Net;
using System.Security.Cryptography;
using System.Text;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Security;

namespace ChatOS.Connector.Tests;

public sealed class PluginConfigurationServiceTests : IDisposable
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        $"chatos-plugin-config-{Guid.NewGuid():N}");

    [Fact]
    public async Task ListsDeclaredSettingsAndConfiguresCredentialWithoutReturningItsValue()
    {
        var record = CreateInstallation();
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        var secrets = new MemorySecrets();
        var vault = new PluginCredentialVault(secrets, new SqlitePluginCredentialMetadataStore(database));
        var oauth = new PluginOAuthBroker(
            new InstalledStore(record),
            vault,
            new SqlitePluginOAuthConnectionStore(database),
            new Factory(),
            new Launcher());
        var runtime = new ConnectorRuntimeContext(
            new RuntimeStore(new ConnectorPersistentState(
                new Uri("https://gateway.example"),
                new ConnectorUser("owner-1", "owner", "Owner", "user"),
                "device-1",
                "Windows PC",
                [],
                new RemoteControlTrust(false, 120, new Dictionary<string, string>()))),
            new TokenStore());
        await runtime.InitializeAsync();
        var service = new PluginConfigurationService(
            new InstalledStore(record),
            vault,
            oauth,
            runtime);

        var initial = await service.GetAsync("plugin-1");

        var component = Assert.Single(initial.Components);
        var credential = Assert.Single(component.Credentials);
        Assert.Equal("api.token", credential.SecretName);
        Assert.False(credential.Configured);
        Assert.Equal("http", component.Transport);
        var app = Assert.Single(initial.OAuthApps);
        Assert.Equal("demo", app.Provider);
        Assert.Null(app.Connection);

        await service.SetCredentialAsync("plugin-1", "remote", "api.token", "top-secret-token");
        var configured = await service.GetAsync("plugin-1");
        Assert.True(Assert.Single(Assert.Single(configured.Components).Credentials).Configured);
        Assert.DoesNotContain("top-secret-token", System.Text.Json.JsonSerializer.Serialize(configured));
        Assert.Contains(secrets.Values.Values, value => value == "top-secret-token");

        await service.DeleteCredentialAsync("plugin-1", "remote", "api.token");
        Assert.False(Assert.Single(Assert.Single((await service.GetAsync("plugin-1")).Components).Credentials)
            .Configured);
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
        var installation = Path.Combine(_directory, "installed");
        Directory.CreateDirectory(Path.Combine(installation, "oauth"));
        var manifest = """{"schemaVersion":3,"name":"config-plugin","version":"1.0.0","apps":[{"component_key":"demo-app","manifest":{"path":"oauth/demo.json"}}],"mcpServers":{"remote":{"type":"http","url":"https://mcp.example/rpc","headers":{"Authorization":"Bearer ${credential:api.token}"},"oauthResource":"resource-demo"}},"permissions":[{"permission":"network.domain:mcp.example","required":true,"components":["remote"]},{"permission":"credential.use:demo","required":true,"components":["remote"]},{"permission":"oauth.scope:demo:read","required":true,"components":["remote"]}]}""";
        var app = """{"schemaVersion":1,"provider":"demo","clientId":"client-1","authorizationUrl":"https://oauth.example/authorize","tokenUrl":"https://oauth.example/token","resource":"resource-demo","scopes":["read"],"callbackType":"loopback"}""";
        File.WriteAllText(Path.Combine(installation, "chatos.plugin.json"), manifest);
        File.WriteAllText(Path.Combine(installation, "oauth", "demo.json"), app);
        return new InstalledPluginRecord(
            "plugin-1",
            "release-1",
            "1.0.0",
            new string('a', 64),
            installation,
            DateTimeOffset.UtcNow,
            ["network.domain:mcp.example", "credential.use:demo", "oauth.scope:demo:read"],
            new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
            {
                ["chatos.plugin.json"] = Sha256(manifest),
                ["oauth/demo.json"] = Sha256(app),
            });
    }

    private static string Sha256(string value) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant();

    private sealed class InstalledStore(InstalledPluginRecord record) : IInstalledPluginStore
    {
        public Task<IReadOnlyList<InstalledPluginRecord>> ListAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<InstalledPluginRecord>>([record]);
        public Task<InstalledPluginRecord?> GetAsync(string pluginId, CancellationToken cancellationToken = default) =>
            Task.FromResult<InstalledPluginRecord?>(pluginId == record.PluginId ? record : null);
        public Task SaveAsync(InstalledPluginRecord value, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
        public Task DeleteAsync(string pluginId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
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

    private sealed class RuntimeStore(ConnectorPersistentState state) : IConnectorPersistentStateStore
    {
        public Task<ConnectorPersistentState?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<ConnectorPersistentState?>(state);
        public Task SaveAsync(ConnectorPersistentState? value, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
    }

    private sealed class TokenStore : IConnectorAccessTokenStore
    {
        public ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default) =>
            ValueTask.FromResult<string?>("token");
        public ValueTask SetAccessTokenAsync(string token, CancellationToken cancellationToken = default) =>
            ValueTask.CompletedTask;
        public ValueTask ClearAsync(CancellationToken cancellationToken = default) => ValueTask.CompletedTask;
    }

    private sealed class Factory : IHttpClientFactory
    {
        public HttpClient CreateClient(string name) => new(new Handler());
    }

    private sealed class Handler : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken) => Task.FromResult(new HttpResponseMessage(HttpStatusCode.OK));
    }

    private sealed class Launcher : IExternalUriLauncher
    {
        public Task LaunchAsync(Uri uri, CancellationToken cancellationToken = default) => Task.CompletedTask;
    }
}
