using System.Net;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Plugins;
using ChatOS.Connector.Relay;

namespace ChatOS.Connector.Tests;

public sealed class PluginHttpMcpProxyTests
{
    [Fact]
    public async Task ForwardsAllowedJsonRpcCallWithValidatedRuntimeHeaders()
    {
        HttpRequestMessage? captured = null;
        var proxy = new PluginHttpMcpProxy(new Factory(new ClientHandler(async request =>
        {
            captured = request;
            var body = await request.Content!.ReadAsStringAsync();
            using var input = JsonDocument.Parse(body);
            return new HttpResponseMessage(HttpStatusCode.OK)
            {
                Content = new StringContent(JsonSerializer.Serialize(new
                {
                    jsonrpc = "2.0",
                    id = input.RootElement.GetProperty("id").GetInt32(),
                    result = new { tools = Array.Empty<object>() },
                }), Encoding.UTF8, "application/json"),
            };
        })));

        var result = await proxy.HandleAsync(Request(
            new
            {
                url = "https://mcp.example/rpc",
                headers = new Dictionary<string, string> { ["Authorization"] = "Bearer secret" },
                timeout_ms = 5000,
            },
            new { jsonrpc = "2.0", id = 7, method = "tools/list", @params = new { } }),
            CancellationToken.None);

        Assert.Equal(200, result.Status);
        Assert.Equal(7, result.Body.GetProperty("id").GetInt32());
        Assert.Equal("Bearer secret", captured?.Headers.Authorization?.ToString());
        Assert.Equal("/rpc", captured?.RequestUri?.AbsolutePath);
    }

    [Theory]
    [InlineData("http://example.com/rpc")]
    [InlineData("ftp://localhost/rpc")]
    [InlineData("https://user:password@example.com/rpc")]
    public async Task RejectsUnsafeEndpoint(string url)
    {
        var proxy = new PluginHttpMcpProxy(new Factory(new ClientHandler(_ =>
            throw new InvalidOperationException("request should not be sent"))));

        var error = await Assert.ThrowsAsync<RelayRequestException>(() => proxy.HandleAsync(Request(
            new { url, headers = new { }, timeout_ms = 1000 },
            new { jsonrpc = "2.0", id = 1, method = "tools/list", @params = new { } }),
            CancellationToken.None));

        Assert.Equal(400, error.StatusCode);
    }

    [Fact]
    public async Task AllowsLoopbackHttpButRejectsHopByHopHeaderAndUnsupportedMethod()
    {
        var proxy = new PluginHttpMcpProxy(new Factory(new ClientHandler(_ =>
            Task.FromResult(new HttpResponseMessage(HttpStatusCode.OK)
            {
                Content = new StringContent("{}", Encoding.UTF8, "application/json"),
            }))));
        var unsafeHeader = await Assert.ThrowsAsync<RelayRequestException>(() => proxy.HandleAsync(Request(
            new
            {
                url = "http://127.0.0.1:3000/rpc",
                headers = new Dictionary<string, string> { ["Host"] = "evil.example" },
                timeout_ms = 1000,
            },
            new { jsonrpc = "2.0", id = 1, method = "tools/list", @params = new { } }),
            CancellationToken.None));
        Assert.Equal(400, unsafeHeader.StatusCode);

        var unsupported = await Assert.ThrowsAsync<RelayRequestException>(() => proxy.HandleAsync(Request(
            new { url = "http://localhost:3000/rpc", headers = new { }, timeout_ms = 1000 },
            new { jsonrpc = "2.0", id = 1, method = "resources/list", @params = new { } }),
            CancellationToken.None));
        Assert.Equal(400, unsupported.StatusCode);
    }

    [Fact]
    public async Task RejectsOversizedResponseFromStream()
    {
        var payload = new byte[PluginHttpMcpProxy.MaximumResponseBytes + 1];
        var proxy = new PluginHttpMcpProxy(new Factory(new ClientHandler(_ =>
            Task.FromResult(new HttpResponseMessage(HttpStatusCode.OK)
            {
                Content = new ByteArrayContent(payload),
            }))));

        var error = await Assert.ThrowsAsync<RelayRequestException>(() => proxy.HandleAsync(Request(
            new { url = "https://mcp.example/rpc", headers = new { }, timeout_ms = 1000 },
            new { jsonrpc = "2.0", id = 1, method = "tools/call", @params = new { name = "echo" } }),
            CancellationToken.None));

        Assert.Equal(502, error.StatusCode);
    }

    private static RelayRequest Request(object runtime, object body) => new()
    {
        Type = "mcp",
        RequestId = "request-1",
        OwnerUserId = "owner-1",
        DeviceId = "device-1",
        WorkspaceId = "workspace-1",
        Headers = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["x-plugin-management-resource-id"] = "resource-1",
            ["x-local-connector-inline-mcp-runtime"] = Uri.EscapeDataString(JsonSerializer.Serialize(runtime)),
        },
        Body = JsonSerializer.SerializeToElement(body),
    };

    private sealed class Factory(HttpClient client) : IHttpClientFactory
    {
        public Factory(HttpMessageHandler handler)
            : this(new HttpClient(handler))
        {
        }

        public HttpClient CreateClient(string name) => client;
    }

    private sealed class ClientHandler(
        Func<HttpRequestMessage, Task<HttpResponseMessage>> response) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken) => response(request);
    }
}
