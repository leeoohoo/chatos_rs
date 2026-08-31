using System.Net;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.Security;

namespace ChatOS.Connector.Tests;

public sealed class PluginHttpMcpClientRuntimeTests : IDisposable
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        $"chatos-http-mcp-client-{Guid.NewGuid():N}");

    [Fact]
    public async Task InitializesCallsAndRejectsCredentialChangesAfterPrepare()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        var secrets = new MemorySecrets();
        var vault = new PluginCredentialVault(secrets, new SqlitePluginCredentialMetadataStore(database));
        var launch = await CreateLaunchAsync(vault);
        var authorizations = new List<string>();
        var client = new PluginHttpMcpClient(
            launch,
            new Factory(new Handler(async (request, _) =>
            {
                authorizations.Add(request.Headers.Authorization?.ToString() ?? string.Empty);
                var body = await request.Content!.ReadAsStringAsync();
                using var document = JsonDocument.Parse(body);
                var root = document.RootElement;
                var method = root.GetProperty("method").GetString();
                if (method == "notifications/initialized")
                {
                    return new HttpResponseMessage(HttpStatusCode.Accepted)
                    {
                        Content = new ByteArrayContent([]),
                    };
                }

                var result = method switch
                {
                    "initialize" => new
                    {
                        protocolVersion = "2025-06-18",
                        instructions = "Use safely",
                    },
                    "tools/list" => (object)new
                    {
                        tools = new[] { new { name = "echo", inputSchema = new { type = "object" } } },
                    },
                    _ => new { content = new { value = "ok" } },
                };
                return Json(new
                {
                    jsonrpc = "2.0",
                    id = root.GetProperty("id").GetInt64(),
                    result,
                });
            })),
            null!);

        await client.StartAsync();
        var initialized = await client.InitializeAsync();
        var result = await client.CallToolAsync(
            "echo",
            JsonSerializer.SerializeToElement(new { value = "hello" }),
            TimeSpan.FromSeconds(5));

        Assert.Equal("Use safely", initialized.Instructions);
        Assert.Equal("echo", Assert.Single(initialized.Tools).GetProperty("name").GetString());
        Assert.Equal("ok", result.GetProperty("content").GetProperty("value").GetString());
        Assert.All(authorizations, value => Assert.Equal("Bearer top-secret-token", value));

        await vault.UpsertAsync(new PluginCredentialScope(
            "owner-1", "device-1", "plugin-1", "release-1", "remote", "api.token"),
            "rotated-token");
        var error = await Assert.ThrowsAsync<PluginRuntimeException>(() => client.CallToolAsync(
            "echo",
            JsonSerializer.SerializeToElement(new { }),
            TimeSpan.FromSeconds(5)));
        Assert.Contains("snapshot changed", error.Message, StringComparison.OrdinalIgnoreCase);
        await client.DisposeAsync();
    }

    [Fact]
    public async Task CancelsInflightHttpToolCall()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "cancel-state.db"));
        await database.InitializeAsync();
        var vault = new PluginCredentialVault(
            new MemorySecrets(),
            new SqlitePluginCredentialMetadataStore(database));
        var launch = await CreateLaunchAsync(vault);
        var started = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var client = new PluginHttpMcpClient(
            launch,
            new Factory(new Handler(async (request, cancellationToken) =>
            {
                var body = await request.Content!.ReadAsStringAsync(cancellationToken);
                using var document = JsonDocument.Parse(body);
                if (document.RootElement.GetProperty("method").GetString() == "tools/call")
                {
                    started.TrySetResult();
                    await Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
                }

                return Json(new
                {
                    jsonrpc = "2.0",
                    id = document.RootElement.TryGetProperty("id", out var id) ? id.GetInt64() : 0,
                    result = new
                    {
                        tools = new[] { new { name = "echo", inputSchema = new { type = "object" } } },
                    },
                });
            })),
            null!);
        await client.StartAsync();
        using var cancellation = new CancellationTokenSource();
        var call = client.CallToolAsync(
            "echo",
            JsonSerializer.SerializeToElement(new { }),
            TimeSpan.FromMinutes(1),
            cancellation.Token);
        await started.Task.WaitAsync(TimeSpan.FromSeconds(2));

        cancellation.Cancel();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => call);
        await client.DisposeAsync();
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

    private async Task<PreparedPluginLaunch> CreateLaunchAsync(PluginCredentialVault vault)
    {
        var installation = Path.Combine(_directory, "installed");
        Directory.CreateDirectory(installation);
        await File.WriteAllTextAsync(
            Path.Combine(installation, "chatos.plugin.json"),
            """{"schemaVersion":3,"name":"http-plugin","version":"1.0.0","mcpServers":{"remote":{"type":"http","url":"http://127.0.0.1:43111/mcp","headers":{"Authorization":"Bearer ${credential:api.token}"}}},"permissions":[{"permission":"network.domain:127.0.0.1","required":true,"components":["remote"]},{"permission":"credential.use:demo","required":true,"components":["remote"]}]}""");
        var scope = new PluginCredentialScope(
            "owner-1", "device-1", "plugin-1", "release-1", "remote", "api.token");
        await vault.UpsertAsync(scope, "top-secret-token");
        var loader = new PluginManifestLoader(Path.Combine(_directory, "runtime"), vault);
        return await loader.PrepareAsync(
            new InstalledPluginRecord(
                "plugin-1", "release-1", "1.0.0", new string('a', 64), installation,
                DateTimeOffset.UtcNow,
                ["network.domain:127.0.0.1", "credential.use:demo"]),
            "remote",
            null,
            Guid.NewGuid().ToString("D"),
            null,
            new HashSet<string> { "network.domain:127.0.0.1", "credential.use:demo" },
            "owner-1",
            "device-1");
    }

    private static HttpResponseMessage Json(object value) => new(HttpStatusCode.OK)
    {
        Content = new StringContent(JsonSerializer.Serialize(value), Encoding.UTF8, "application/json"),
    };

    private sealed class Factory(HttpMessageHandler handler) : IHttpClientFactory
    {
        private readonly HttpClient _client = new(handler);
        public HttpClient CreateClient(string name) => _client;
    }

    private sealed class Handler(
        Func<HttpRequestMessage, CancellationToken, Task<HttpResponseMessage>> send) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken) => send(request, cancellationToken);
    }

    private sealed class MemorySecrets : IConnectorSecretStore
    {
        private readonly Dictionary<string, string> _values = new(StringComparer.Ordinal);
        public ValueTask<string?> GetAsync(string key, CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(_values.GetValueOrDefault(key));
        public ValueTask SetAsync(string key, string value, CancellationToken cancellationToken = default)
        {
            _values[key] = value;
            return ValueTask.CompletedTask;
        }
        public ValueTask DeleteAsync(string key, CancellationToken cancellationToken = default)
        {
            _values.Remove(key);
            return ValueTask.CompletedTask;
        }
    }
}
