using System.Net;
using System.Security.Cryptography;
using System.Text;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Tests;

public sealed class ApprovalAiReviewerTests
{
    [Fact]
    public async Task RuntimeConfigurationRequiresValidManagedPromptAndCapability()
    {
        var context = await TestContext.CreateAsync();

        var configuration = await context.Configuration.ResolveAsync();

        Assert.Equal("model-1", configuration.ModelConfigId);
        Assert.Equal("gpt", configuration.Provider);
        Assert.Equal("managed approval prompt", configuration.SystemPrompt);
        Assert.Equal(1, configuration.MaximumTransientRetries);
        Assert.Equal("policy-1", configuration.CapabilityPolicyRevision);
    }

    [Theory]
    [InlineData("wrong-owner", true, true, "sha256:bad")]
    [InlineData("owner-1", false, true, "sha256:bad")]
    [InlineData("owner-1", true, false, "sha256:bad")]
    public async Task RuntimeConfigurationRejectsInvalidManagedState(
        string owner,
        bool capabilityEnabled,
        bool modelEnabled,
        string invalidChecksum)
    {
        var context = await TestContext.CreateAsync();
        context.Gateway.Capability = context.Gateway.Capability with
        {
            OwnerUserId = owner,
            AgentEnabled = capabilityEnabled,
        };
        context.Gateway.Model = context.Gateway.Model with { Enabled = modelEnabled };
        if (owner == "owner-1" && capabilityEnabled && modelEnabled)
        {
            var prompt = context.Gateway.Bundle.Prompts[0] with { Checksum = invalidChecksum };
            context.Gateway.Bundle = context.Gateway.Bundle with { Prompts = [prompt] };
        }

        await Assert.ThrowsAsync<InvalidOperationException>(() => context.Configuration.ResolveAsync());
    }

    [Fact]
    public async Task RuntimeConfigurationRejectsNonLoopbackHttpProvider()
    {
        var context = await TestContext.CreateAsync();
        context.Gateway.Model = context.Gateway.Model with { BaseUrl = "http://provider.example/v1" };

        var error = await Assert.ThrowsAsync<InvalidOperationException>(
            () => context.Configuration.ResolveAsync());

        Assert.Contains("unsafe", error.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task ReviewerParsesForcedApprovalToolAndDoesNotSendAbsoluteWorkspacePath()
    {
        var context = await TestContext.CreateAsync();
        string? body = null;
        string? authorization = null;
        var reviewer = context.Reviewer(async request =>
        {
            authorization = request.Headers.Authorization?.ToString();
            body = await request.Content!.ReadAsStringAsync();
            return Json(HttpStatusCode.OK, ToolDecision("approve", "Read-only command.", true));
        });

        var result = await reviewer.ReviewAsync(context.Request(), context.Risk);

        Assert.Equal(CommandApprovalAiDecisionKind.Approve, result.Decision);
        Assert.True(result.RememberForSession);
        Assert.Equal("Bearer secret-key", authorization);
        Assert.Contains("managed approval prompt", body, StringComparison.Ordinal);
        Assert.Contains("cwd: nested", body, StringComparison.Ordinal);
        Assert.DoesNotContain(context.WorkspaceRoot, body, StringComparison.Ordinal);
    }

    [Fact]
    public async Task ReviewerRetriesTransientFailureOnlyOnce()
    {
        var context = await TestContext.CreateAsync();
        var calls = 0;
        var reviewer = context.Reviewer(_ =>
        {
            calls++;
            return Task.FromResult(calls == 1
                ? Json(HttpStatusCode.TooManyRequests, "{\"error\":{\"message\":\"busy\"}}")
                : Json(HttpStatusCode.OK, ToolDecision("deny", "Unsafe.")));
        });

        var result = await reviewer.ReviewAsync(context.Request(), context.Risk);

        Assert.Equal(CommandApprovalAiDecisionKind.Deny, result.Decision);
        Assert.Equal(2, calls);
    }

    [Fact]
    public async Task ReviewerRejectsMalformedDecisionWithoutExposingProviderErrorBody()
    {
        var context = await TestContext.CreateAsync();
        var malformed = context.Reviewer(_ => Task.FromResult(Json(HttpStatusCode.OK, "{\"choices\":[]}")));
        await Assert.ThrowsAsync<InvalidOperationException>(
            () => malformed.ReviewAsync(context.Request(), context.Risk));

        var providerError = context.Reviewer(_ => Task.FromResult(Json(
            HttpStatusCode.BadRequest,
            "{\"error\":{\"message\":\"invalid secret-key credential\"}}")));
        var error = await Assert.ThrowsAsync<InvalidOperationException>(
            () => providerError.ReviewAsync(context.Request(), context.Risk));

        Assert.DoesNotContain("secret-key", error.ToString(), StringComparison.Ordinal);
        Assert.Contains("HTTP 400", error.Message, StringComparison.Ordinal);
        Assert.DoesNotContain("credential", error.Message, StringComparison.OrdinalIgnoreCase);
    }

    private static string ToolDecision(string decision, string reason, bool remember = false) => $$"""
        {
          "choices": [{
            "message": {
              "tool_calls": [{
                "function": {
                  "name": "approval_decision",
                  "arguments": "{\"decision\":\"{{decision}}\",\"reason\":\"{{reason}}\",\"remember_allow\":{{remember.ToString().ToLowerInvariant()}}}"
                }
              }]
            }
          }]
        }
        """;

    private static HttpResponseMessage Json(HttpStatusCode status, string body) => new(status)
    {
        Content = new StringContent(body, Encoding.UTF8, "application/json"),
    };

    private sealed class TestContext
    {
        private TestContext(
            string workspaceRoot,
            ConnectorRuntimeContext runtime,
            FakeGateway gateway,
            ApprovalModelRuntimeConfigurationService configuration)
        {
            WorkspaceRoot = workspaceRoot;
            Runtime = runtime;
            Gateway = gateway;
            Configuration = configuration;
        }

        public string WorkspaceRoot { get; }
        public ConnectorRuntimeContext Runtime { get; }
        public FakeGateway Gateway { get; }
        public ApprovalModelRuntimeConfigurationService Configuration { get; }
        public ConnectorApprovalRisk Risk { get; } =
            new(ConnectorApprovalRiskLevel.Low, "Static read-only classification.");

        public static async Task<TestContext> CreateAsync()
        {
            var workspaceRoot = Path.Combine(Path.GetTempPath(), "chatos-reviewer-workspace");
            var runtime = new ConnectorRuntimeContext(
                new MemoryPersistentStateStore(),
                new MemoryAccessTokenStore("connector-token"));
            await runtime.ReplaceAsync(new ConnectorPersistentState(
                new Uri("https://gateway.example"),
                new ConnectorUser("owner-1", "owner", "Owner", "user"),
                "device-1",
                "Windows PC",
                [new ConnectorWorkspace("workspace-1", "Workspace", workspaceRoot, "fingerprint")],
                new RemoteControlTrust(false, 120, new Dictionary<string, string>())));

            const string prompt = "managed approval prompt";
            var gateway = new FakeGateway
            {
                Model = new ConnectorGatewayModelConfig(
                    "model-1", "Approval GPT", "openai", "gpt", "gpt-5-mini",
                    "https://provider.example/v1", "secret-key", true, false, null, 0, 900),
                Bundle = new ConnectorAgentPromptBundle(
                    2,
                    DateTimeOffset.UtcNow,
                    [new ConnectorAgentPrompt(
                        ApprovalModelRuntimeConfigurationService.AgentKey,
                        "gpt",
                        prompt,
                        3,
                        Checksum(prompt),
                        DateTimeOffset.UtcNow)]),
                Capability = new ConnectorAgentCapability(
                    ApprovalModelRuntimeConfigurationService.AgentKey,
                    "owner-1",
                    "policy-1",
                    true),
            };
            var configuration = new ApprovalModelRuntimeConfigurationService(
                new MemoryModelSettingsStore(new ConnectorModelSettings(5, "model-1")),
                runtime,
                gateway);
            return new TestContext(workspaceRoot, runtime, gateway, configuration);
        }

        public CommandApprovalRequest Request() => new(
            "request-1",
            "owner-1",
            "device-1",
            "workspace-1",
            "git",
            ["status"],
            Path.Combine(WorkspaceRoot, "nested"),
            "terminal",
            "scope-1");

        public OpenAiCompatibleCommandApprovalReviewer Reviewer(
            Func<HttpRequestMessage, Task<HttpResponseMessage>> response) =>
            new(Configuration, Runtime, new FakeHttpClientFactory(
                new HttpClient(new DelegateHandler(response))));

        private static string Checksum(string content) =>
            "sha256:" + Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(content))).ToLowerInvariant();
    }

    private sealed class MemoryModelSettingsStore(ConnectorModelSettings settings)
        : IConnectorModelSettingsStore
    {
        public Task<ConnectorModelSettings> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(settings);

        public Task SaveAsync(ConnectorModelSettings value, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
    }

    private sealed class MemoryPersistentStateStore : IConnectorPersistentStateStore
    {
        public Task<ConnectorPersistentState?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<ConnectorPersistentState?>(null);

        public Task SaveAsync(ConnectorPersistentState? state, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
    }

    private sealed class MemoryAccessTokenStore(string token) : IConnectorAccessTokenStore
    {
        public ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default) =>
            ValueTask.FromResult<string?>(token);

        public ValueTask SetAccessTokenAsync(string value, CancellationToken cancellationToken = default) =>
            ValueTask.CompletedTask;

        public ValueTask ClearAsync(CancellationToken cancellationToken = default) => ValueTask.CompletedTask;
    }

    private sealed class FakeGateway : IConnectorGatewayClient
    {
        public required ConnectorGatewayModelConfig Model { get; set; }
        public required ConnectorAgentPromptBundle Bundle { get; set; }
        public required ConnectorAgentCapability Capability { get; set; }

        public Task<ConnectorGatewayModelConfig> GetModelConfigAsync(
            Uri gatewayBaseUri, string token, string modelConfigId, bool includeSecret,
            CancellationToken cancellationToken = default) => Task.FromResult(Model);

        public Task<ConnectorAgentPromptBundle> GetAgentPromptBundleAsync(
            Uri gatewayBaseUri, string token, CancellationToken cancellationToken = default) =>
            Task.FromResult(Bundle);

        public Task<ConnectorAgentCapability> GetAgentCapabilityAsync(
            Uri gatewayBaseUri, string token, string agentKey,
            CancellationToken cancellationToken = default) => Task.FromResult(Capability);

        public Task<ConnectorGatewayLogin> ExchangeTicketAsync(Uri gatewayBaseUri, string ticket, string deviceName, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<ConnectorGatewayDevice> CreateDeviceAsync(Uri gatewayBaseUri, string token, string displayName, string publicKey, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<ConnectorGatewayDevice?> GetDeviceAsync(Uri gatewayBaseUri, string token, string deviceId, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task DisconnectDeviceAsync(Uri gatewayBaseUri, string token, string deviceId, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<IReadOnlyList<ConnectorGatewayWorkspace>> ListWorkspacesAsync(Uri gatewayBaseUri, string token, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<ConnectorGatewayWorkspace> CreateWorkspaceAsync(Uri gatewayBaseUri, string token, string deviceId, string alias, string fingerprint, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<ConnectorGatewayWorkspace> MoveWorkspaceAsync(Uri gatewayBaseUri, string token, string workspaceId, string deviceId, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<RemoteControlTrust> GetRemoteControlTrustAsync(Uri gatewayBaseUri, string token, CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }

    private sealed class FakeHttpClientFactory(HttpClient client) : IHttpClientFactory
    {
        public HttpClient CreateClient(string name) => client;
    }

    private sealed class DelegateHandler(
        Func<HttpRequestMessage, Task<HttpResponseMessage>> response) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken) => response(request);
    }
}
