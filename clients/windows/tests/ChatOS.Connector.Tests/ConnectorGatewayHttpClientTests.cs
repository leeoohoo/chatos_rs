using System.Net;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Gateway;

namespace ChatOS.Connector.Tests;

public sealed class ConnectorGatewayHttpClientTests
{
    [Fact]
    public async Task ExchangesTicketWithWindowsIdentityAndMapsUser()
    {
        HttpRequestMessage? captured = null;
        string? body = null;
        var client = Client(async request =>
        {
            captured = request;
            body = request.Content is null ? null : await request.Content.ReadAsStringAsync();
            return Json(HttpStatusCode.OK, """
                {
                  "token": "connector-token",
                  "user": {
                    "id": "owner-1",
                    "username": "owner",
                    "display_name": "Owner",
                    "role": "user"
                  }
                }
                """);
        });

        var login = await client.ExchangeTicketAsync(
            new Uri("https://gateway.example"),
            "ticket-1",
            "Windows PC");

        Assert.Equal("connector-token", login.Token);
        Assert.Equal("owner-1", login.User.Id);
        Assert.Equal("/api/auth/local-connector-ticket/exchange", captured?.RequestUri?.AbsolutePath);
        Assert.Equal("local-connector-windows", captured?.Headers.GetValues("X-Chatos-Client-Surface").Single());
        using var document = JsonDocument.Parse(body!);
        Assert.Equal("1.0.0-windows", document.RootElement.GetProperty("client_version").GetString());
        Assert.Equal("Windows PC", document.RootElement.GetProperty("device_name").GetString());
    }

    [Fact]
    public async Task RegistersWindowsDeviceAndReadsManagedTrust()
    {
        var requests = new List<(string Path, string? Authorization, string? Body)>();
        var client = Client(async request =>
        {
            requests.Add((
                request.RequestUri!.AbsolutePath,
                request.Headers.Authorization?.ToString(),
                request.Content is null ? null : await request.Content.ReadAsStringAsync()));
            return request.RequestUri.AbsolutePath.EndsWith("/config/runtime", StringComparison.Ordinal)
                ? Json(HttpStatusCode.OK, """
                    {
                      "remote_control_trust": {
                        "require_signed_messages": true,
                        "signature_max_skew_seconds": 120,
                        "trusted_relay_public_keys": { "relay-1": "ed25519:key" }
                      }
                    }
                    """)
                : Json(HttpStatusCode.OK, """
                    {
                      "id": "device-1",
                      "owner_user_id": "owner-1",
                      "display_name": "Windows PC",
                      "public_key": "ed25519:public",
                      "status": "online"
                    }
                    """);
        });

        var device = await client.CreateDeviceAsync(
            new Uri("https://gateway.example"),
            "token-1",
            "Windows PC",
            "ed25519:public");
        var trust = await client.GetRemoteControlTrustAsync(
            new Uri("https://gateway.example"),
            "token-1");

        Assert.Equal("device-1", device.Id);
        Assert.True(trust.RequireSignedMessages);
        Assert.Equal(120, trust.SignatureMaxSkewSeconds);
        Assert.All(requests, request => Assert.Equal("Bearer token-1", request.Authorization));
        using var body = JsonDocument.Parse(requests[0].Body!);
        Assert.Equal("Windows", body.RootElement.GetProperty("os").GetString());
        Assert.Equal("1.0.0-windows", body.RootElement.GetProperty("client_version").GetString());
    }

    [Fact]
    public async Task ReadsAuthoritativeControlledNetworkReadinessForPairedDevice()
    {
        HttpRequestMessage? captured = null;
        var client = Client(request =>
        {
            captured = request;
            return Task.FromResult(Json(HttpStatusCode.OK, """
                {
                  "available": true,
                  "state": "ready",
                  "permission_profile": "windows-controlled",
                  "allowed_host_count": 3
                }
                """));
        });

        var readiness = await client.GetControlledNetworkReadinessAsync(
            new Uri("https://gateway.example"),
            "token-1",
            "device/1");

        Assert.True(readiness.Available);
        Assert.Equal("windows-controlled", readiness.PermissionProfile);
        Assert.Equal(3, readiness.AllowedHostCount);
        Assert.Equal(
            "/api/local-connectors/devices/device%2F1/controlled-network/readiness",
            captured?.RequestUri?.AbsolutePath);
        Assert.Equal("Bearer token-1", captured?.Headers.Authorization?.ToString());
    }

    [Fact]
    public async Task NotFoundDeviceMapsToNullButOtherErrorsRemainVisible()
    {
        var client = Client(request => Task.FromResult(
            request.RequestUri!.Query == "?forbidden=true"
                ? Json(HttpStatusCode.Forbidden, "{\"error\":\"denied\"}")
                : Json(HttpStatusCode.NotFound, "{\"error\":\"missing\"}")));

        Assert.Null(await client.GetDeviceAsync(
            new Uri("https://gateway.example"),
            "token",
            "missing"));
    }

    [Fact]
    public async Task MapsPluginInstallSourcesAndUpdatesPreference()
    {
        var requests = new List<(string Path, string? Body)>();
        var client = Client(async request =>
        {
            requests.Add((
                request.RequestUri!.AbsolutePath,
                request.Content is null ? null : await request.Content.ReadAsStringAsync()));
            return request.Method == HttpMethod.Get
                ? Json(HttpStatusCode.OK, """
                    {
                      "items": [{
                        "catalog": {
                          "id": "plugin-1",
                          "display_name": "Browser",
                          "name": "browser-plugin",
                          "description": "Browser tools",
                          "publisher": { "name": "ChatOS" },
                          "interface": { "category": "Browser", "developer_name": "ChatOS" }
                        },
                        "release": {
                          "id": "release-1",
                          "version": "1.2.3",
                          "artifact_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                          "supported_platforms": ["windows", "macos"],
                          "npm_package": {
                            "name": "browser-plugin",
                            "version": "1.2.3",
                            "integrity": "sha512-YQ=="
                          }
                        },
                        "preference": { "enabled": false }
                      }]
                    }
                    """)
                : Json(HttpStatusCode.OK, "{}");
        });

        var sources = await client.ListPluginSourcesAsync(new Uri("https://gateway.example"), "token");
        await client.UpdatePluginPreferenceAsync(
            new Uri("https://gateway.example"),
            "token",
            "plugin-1",
            "device-1",
            true);

        var source = Assert.Single(sources);
        Assert.Equal("Browser", source.Catalog.DisplayName);
        Assert.Equal(["windows", "macos"], source.Release.SupportedPlatforms);
        Assert.False(source.Preference?.Enabled);
        Assert.Equal("/api/plugin-management/plugins/install-sources", requests[0].Path);
        using var body = JsonDocument.Parse(requests[1].Body!);
        Assert.Equal("device-1", body.RootElement.GetProperty("device_id").GetString());
        Assert.True(body.RootElement.GetProperty("enabled").GetBoolean());
    }

    [Fact]
    public async Task StreamsPluginArtifactWithoutBufferingWholeResponse()
    {
        var payload = Encoding.UTF8.GetBytes("plugin archive");
        var client = Client(_ => Task.FromResult(new HttpResponseMessage(HttpStatusCode.OK)
        {
            Content = new ByteArrayContent(payload),
        }));
        await using var destination = new MemoryStream();

        await client.DownloadPluginArtifactAsync(
            new Uri("https://gateway.example"),
            "token",
            "plugin-1",
            "release-1",
            destination);

        Assert.Equal(payload, destination.ToArray());
    }

    [Fact]
    public async Task ReadsModelConfigurationIncludingSecretWithBearerAuthentication()
    {
        var requests = new List<(string PathAndQuery, string? Authorization)>();
        var client = Client(request =>
        {
            requests.Add((request.RequestUri!.PathAndQuery, request.Headers.Authorization?.ToString()));
            var payload = request.RequestUri.AbsolutePath == "/api/model-configs"
                ? """
                  [{
                    "id":"model-1","name":"Approval GPT","provider":"openai",
                    "prompt_vendor":"gpt","model":"gpt-5-mini","enabled":true
                  }]
                  """
                : """
                  {
                    "id":"model-1","name":"Approval GPT","provider":"openai",
                    "prompt_vendor":"gpt","model":"gpt-5-mini","api_key":"secret-key",
                    "base_url":"https://api.openai.com/v1","enabled":true,
                    "temperature":0.1,"max_output_tokens":900
                  }
                  """;
            return Task.FromResult(Json(HttpStatusCode.OK, payload));
        });

        var models = await client.ListModelConfigsAsync(new Uri("https://gateway.example"), "token-1");
        var detail = await client.GetModelConfigAsync(
            new Uri("https://gateway.example"),
            "token-1",
            "model-1",
            includeSecret: true);

        Assert.Equal("Approval GPT", Assert.Single(models).Name);
        Assert.Equal("secret-key", detail.ApiKey);
        Assert.Equal("/api/model-configs/model-1?include_secret=true", requests[1].PathAndQuery);
        Assert.All(requests, request => Assert.Equal("Bearer token-1", request.Authorization));
    }

    [Fact]
    public async Task ReadsAgentPromptBundleAndCapabilityPolicy()
    {
        var paths = new List<string>();
        var client = Client(request =>
        {
            paths.Add(request.RequestUri!.AbsolutePath);
            var payload = request.RequestUri.AbsolutePath.EndsWith("/bundle", StringComparison.Ordinal)
                ? """
                  {
                    "bundle_version":12,
                    "updated_at":"2026-08-30T08:00:00Z",
                    "prompts":[{
                      "agent_key":"local_connector_command_approval_agent",
                      "vendor":"gpt","content":"system prompt","revision":4,
                      "checksum":"sha256:abc","published_at":"2026-08-30T07:00:00Z"
                    }]
                  }
                  """
                : """
                  {
                    "agent_key":"local_connector_command_approval_agent",
                    "owner_user_id":"owner-1","policy_revision":"policy-9",
                    "agent_enabled":true
                  }
                  """;
            return Task.FromResult(Json(HttpStatusCode.OK, payload));
        });

        var bundle = await client.GetAgentPromptBundleAsync(new Uri("https://gateway.example"), "token");
        var capability = await client.GetAgentCapabilityAsync(
            new Uri("https://gateway.example"),
            "token",
            "local_connector_command_approval_agent");

        Assert.Equal(12, bundle.BundleVersion);
        Assert.Equal("system prompt", Assert.Single(bundle.Prompts).Content);
        Assert.True(capability.AgentEnabled);
        Assert.Equal("owner-1", capability.OwnerUserId);
        Assert.Equal([
            "/api/plugin-management/agent-prompts/bundle",
            "/api/plugin-management/agent-capabilities/local_connector_command_approval_agent",
        ], paths);
    }

    private static ConnectorGatewayHttpClient Client(
        Func<HttpRequestMessage, Task<HttpResponseMessage>> response) =>
        new(new FakeHttpClientFactory(new HttpClient(new DelegateHandler(response))));

    private static HttpResponseMessage Json(HttpStatusCode status, string body) => new(status)
    {
        Content = new StringContent(body, Encoding.UTF8, "application/json"),
    };

    private sealed class FakeHttpClientFactory(HttpClient client) : IHttpClientFactory
    {
        public HttpClient CreateClient(string name) => client;
    }

    private sealed class DelegateHandler(
        Func<HttpRequestMessage, Task<HttpResponseMessage>> response) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken) =>
            response(request);
    }
}
