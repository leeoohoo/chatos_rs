using System.Net;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentExternalMcpSkillGateTests
{
    [Fact]
    public void BundleUsesCanonicalSkillResourcesAndFreezesTheActualCatalog()
    {
        var first = Tool(Element("""{"type":"object","properties":{"query":{"type":"string"}}}"""));
        var bundle = Assert.IsType<WindowsLocalAgentExternalMcpSkillBundleSnapshot>(
            WindowsLocalAgentExternalMcpSkillBundle.Build([first]));
        using var document = JsonDocument.Parse(bundle.Json);
        var root = document.RootElement;

        Assert.Equal(2, root.GetProperty("schema_version").GetInt32());
        Assert.Equal(bundle.Revision, root.GetProperty("bundle_revision").GetString());
        var skill = Assert.Single(root.GetProperty("documents").EnumerateArray());
        Assert.Equal(WindowsLocalAgentExternalMcpSkillBundle.SkillRef,
            skill.GetProperty("skill_ref").GetString());
        Assert.Contains("External MCP in a local Task", skill.GetProperty("instructions").GetString());
        Assert.Equal(2, skill.GetProperty("resources").GetArrayLength());
        var binding = Assert.Single(root.GetProperty("tool_skill_bindings").EnumerateArray());
        Assert.Equal("chatos.external-mcp:mcp-1", binding.GetProperty("provider_id").GetString());
        Assert.Equal("on_demand", binding.GetProperty("activation_policy").GetString());

        var changedSchema = first with
        {
            InputSchema = Element("""{"type":"object","required":["query"]}"""),
        };
        Assert.NotEqual(
            WindowsLocalAgentExternalMcpSkillBundle.BindingId([first]),
            WindowsLocalAgentExternalMcpSkillBundle.BindingId([changedSchema]));
        var rotatedCredential = first with
        {
            Headers = new Dictionary<string, string> { ["Authorization"] = "Bearer rotated" },
        };
        Assert.Equal(
            WindowsLocalAgentExternalMcpSkillBundle.BindingId([first]),
            WindowsLocalAgentExternalMcpSkillBundle.BindingId([rotatedCredential]));
    }

    [Fact]
    public async Task MissingActivationReturnsExactSkillRefBeforeHttp()
    {
        var factory = new ResponseHttpClientFactory("""{"jsonrpc":"2.0","id":"call-1","result":{"ok":true}}""");
        var host = new SkillHost();
        var executor = new WindowsLocalAgentExternalMcpExecutor(host, factory);
        var tool = Assert.Single(executor.Configure([Resolved()], Selected()));
        host.Bundle = ParseBundle(WindowsLocalAgentExternalMcpSkillBundle.Build([tool])!.Json);

        var result = await executor.ExecuteAsync("user-1", Invocation(tool), CancellationToken.None);

        Assert.True(result.GetProperty("is_error").GetBoolean());
        Assert.Equal("skill_activation_required",
            result.GetProperty("content").GetProperty("error").GetString());
        Assert.Equal(new[] { WindowsLocalAgentExternalMcpSkillBundle.SkillRef },
            result.GetProperty("content").GetProperty("required_skill_refs")
                .EnumerateArray().Select(value => value.GetString()!).ToArray());
        Assert.Equal(0, factory.RequestCount);
    }

    [Fact]
    public async Task FrozenBindingDriftFailsClosedBeforeHttp()
    {
        var factory = new ResponseHttpClientFactory("""{"result":{"ok":true}}""");
        var host = new SkillHost
        {
            Activations = [WindowsLocalAgentExternalMcpSkillBundle.SkillRef],
        };
        var executor = new WindowsLocalAgentExternalMcpExecutor(host, factory);
        var tool = Assert.Single(executor.Configure([Resolved()], Selected()));
        host.Bundle = ParseBundle(WindowsLocalAgentExternalMcpSkillBundle.Build([
            tool with { Description = "changed after the Run was frozen" },
        ])!.Json);

        var result = await executor.ExecuteAsync("user-1", Invocation(tool), CancellationToken.None);

        Assert.Equal("external_mcp_snapshot_changed",
            result.GetProperty("content").GetProperty("error").GetString());
        Assert.Equal(0, factory.RequestCount);
    }

    [Fact]
    public async Task RemoteBusinessSkillGateIsExplicitlyUnsupportedWithoutTrustedDocuments()
    {
        var factory = new ResponseHttpClientFactory("""{"result":{"ok":true}}""");
        var executor = new WindowsLocalAgentExternalMcpExecutor(new SkillHost(), factory);
        var tool = Assert.Single(executor.Configure([Resolved(remoteGate: true)], Selected()));

        var result = await executor.ExecuteAsync("user-1", Invocation(tool), CancellationToken.None);

        Assert.Equal("external_mcp_skill_instructions_unavailable",
            result.GetProperty("content").GetProperty("error").GetString());
        Assert.Equal(0, factory.RequestCount);
    }

    [Fact]
    public async Task ActivatedFrozenToolCallsHttpAndPreservesJsonRpcDiagnosis()
    {
        var factory = new ResponseHttpClientFactory(
            """{"jsonrpc":"2.0","id":"call-1","error":{"code":-32602,"message":"query is required"}}""");
        var host = new SkillHost
        {
            Activations = [WindowsLocalAgentExternalMcpSkillBundle.SkillRef],
        };
        var executor = new WindowsLocalAgentExternalMcpExecutor(host, factory);
        var tool = Assert.Single(executor.Configure([Resolved()], Selected()));
        host.Bundle = ParseBundle(WindowsLocalAgentExternalMcpSkillBundle.Build([tool])!.Json);

        var result = await executor.ExecuteAsync("user-1", Invocation(tool), CancellationToken.None);

        Assert.True(result.GetProperty("is_error").GetBoolean());
        Assert.Equal("external_mcp_rpc_error",
            result.GetProperty("content").GetProperty("error").GetString());
        Assert.Equal(-32602,
            result.GetProperty("content").GetProperty("detail").GetProperty("code").GetInt32());
        Assert.Equal(1, factory.RequestCount);
    }

    private static ConnectorResolvedMcp Resolved(bool remoteGate = false) => new(
        new ConnectorMcpResource(
            "mcp-1", "MCP", "MCP", null, true,
            new ConnectorMcpRuntime(
                "http", null, null, "server", "https://example.test/mcp",
                new Dictionary<string, string>())),
        new ConnectorCapabilityBinding(true, true),
        true,
        "ready",
        [Element(remoteGate
            ? """{"name":"search","description":"Search","inputSchema":{"type":"object"},"_meta":{"chatos/skillGate":{"allOf":["server-skill:search"]}}}"""
            : """{"name":"search","description":"Search","inputSchema":{"type":"object"}}""")]);

    private static HashSet<string> Selected() => new(StringComparer.Ordinal) { "mcp-1" };

    private static WindowsLocalAgentExternalMcpTool Tool(JsonElement schema) => new(
        "mcp-1", "server", "external_mcp__server__search", "search", "Search", schema,
        null, new Uri("https://example.test/mcp"), new Dictionary<string, string>());

    private static WindowsLocalToolInvocation Invocation(WindowsLocalAgentExternalMcpTool tool) => new(
        "invocation-1", "run-1", "batch-1", "call-1", tool.PublicName,
        Element("{}"), true, false, "not_required", null, null, null,
        "claimed", null, null, 1, "claim-1", null, 1, 1);

    private static JsonElement Element(string json)
    {
        using var document = JsonDocument.Parse(json);
        return document.RootElement.Clone();
    }

    private static JsonElement ParseBundle(string json) => Element(json);

    private sealed class SkillHost : ILocalAgentHostClient
    {
        private static readonly JsonSerializerOptions Options = new()
        {
            PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        };

        public JsonElement? Bundle { get; set; }
        public IReadOnlyList<string> Activations { get; set; } = [];
        public string? ActiveOwnerUserId => "user-1";

        public Task StartForOwnerAsync(string ownerUserId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default) where TCommand : notnull
        {
            var type = command.GetType().GetProperty("Type")?.GetValue(command) as string;
            if (type == "get_run")
            {
                object response = new GetLocalRunResult("run", new WindowsLocalAgentRun(
                    "run-1", "user-1", "task", "task-1", "task_execution",
                    JsonSerializer.SerializeToElement(new
                    {
                        tool_options = new { external_mcp_config_ids = new[] { "mcp-1" } },
                    }),
                    "running", 1, null, 1, 1));
                return Task.FromResult((TResponse)response);
            }
            var payload = type switch
            {
                "get_run_skill_bundle" => JsonSerializer.Serialize(new
                {
                    type = "run_skill_bundle",
                    run_id = "run-1",
                    bundle = Bundle,
                }),
                "get_run_skill_activations" => JsonSerializer.Serialize(new
                {
                    type = "run_skill_activations",
                    run_id = "run-1",
                    skill_refs = Activations,
                }),
                _ => throw new InvalidOperationException("Unexpected Host command: " + type),
            };
            return Task.FromResult(JsonSerializer.Deserialize<TResponse>(payload, Options)!);
        }
    }

    private sealed class ResponseHttpClientFactory(string json) : IHttpClientFactory
    {
        public int RequestCount { get; private set; }

        public HttpClient CreateClient(string name) => new(new Handler(this, json));

        private sealed class Handler(ResponseHttpClientFactory owner, string json) : HttpMessageHandler
        {
            protected override Task<HttpResponseMessage> SendAsync(
                HttpRequestMessage request,
                CancellationToken cancellationToken)
            {
                owner.RequestCount++;
                return Task.FromResult(new HttpResponseMessage(HttpStatusCode.OK)
                {
                    Content = new StringContent(json, Encoding.UTF8, "application/json"),
                });
            }
        }
    }
}
