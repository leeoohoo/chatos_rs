using System.Net;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
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
            return Json(HttpStatusCode.OK, ResponsesToolDecision("approve", "Read-only command.", true));
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
    public async Task ReviewerSynchronizesCompleteMemoryTranscriptAndUsesComposedContext()
    {
        var context = await TestContext.CreateAsync();
        var requests = new List<(HttpMethod Method, Uri Uri, string Body)>();
        string? memoryThreadId = null;
        var handler = new DelegateHandler(async request =>
        {
            var body = request.Content is null ? string.Empty : await request.Content.ReadAsStringAsync();
            requests.Add((request.Method, request.RequestUri!, body));
            if (request.RequestUri!.AbsolutePath == "/api/memory/context/compose")
            {
                using var composeRequest = JsonDocument.Parse(body);
                var threadId = composeRequest.RootElement.GetProperty("thread_id").GetString();
                return Json(HttpStatusCode.OK, $$"""
                    {
                      "thread_id": "{{threadId}}",
                      "blocks": [{"block_type":"thread_summary","text":"durable approval context"}],
                      "recent_records": [],
                      "meta": {"recent_record_count": 0}
                    }
                    """);
            }
            if (request.RequestUri.AbsolutePath.EndsWith("/records/batch-sync", StringComparison.Ordinal))
            {
                using var syncRequest = JsonDocument.Parse(body);
                var count = syncRequest.RootElement.GetProperty("records").GetArrayLength();
                return Json(HttpStatusCode.OK, $$"""
                    {"thread_id":"{{memoryThreadId}}","received_count":{{count}},"upserted_count":{{count}}}
                    """);
            }
            if (request.Method == HttpMethod.Put &&
                request.RequestUri.AbsolutePath.StartsWith("/api/memory/threads/", StringComparison.Ordinal))
            {
                memoryThreadId = Uri.UnescapeDataString(request.RequestUri.AbsolutePath.Split('/')[^1]);
                using var threadRequest = JsonDocument.Parse(body);
                var root = threadRequest.RootElement;
                return Json(HttpStatusCode.OK, $$"""
                    {
                      "id":"{{memoryThreadId}}",
                      "tenant_id":"{{root.GetProperty("tenant_id").GetString()}}",
                      "source_id":"{{root.GetProperty("source_id").GetString()}}",
                      "subject_id":"{{root.GetProperty("subject_id").GetString()}}"
                    }
                    """);
            }
            if (request.RequestUri.Host == "provider.example")
            {
                return Json(HttpStatusCode.OK, ResponsesToolDecision("approve", "Safe.", true, includeUsage: true));
            }
            return Json(HttpStatusCode.NotFound, "{}");
        });
        var factory = new FakeHttpClientFactory(new HttpClient(handler));
        var reviewer = new OpenAiCompatibleCommandApprovalReviewer(
            context.Configuration,
            context.Runtime,
            factory,
            new ApprovalMemoryEngineRecorder(factory));

        var result = await reviewer.ReviewAsync(context.Request(), context.Risk);

        Assert.Equal(CommandApprovalAiDecisionKind.Approve, result.Decision);
        Assert.Equal(5, requests.Count);
        Assert.Equal(HttpMethod.Put, requests[0].Method);
        Assert.StartsWith("/api/memory/threads/", requests[0].Uri.AbsolutePath, StringComparison.Ordinal);
        Assert.EndsWith("/records/batch-sync", requests[1].Uri.AbsolutePath, StringComparison.Ordinal);
        Assert.Equal("/api/memory/context/compose", requests[2].Uri.AbsolutePath);
        Assert.Equal("/v1/responses", requests[3].Uri.AbsolutePath);
        Assert.EndsWith("/records/batch-sync", requests[4].Uri.AbsolutePath, StringComparison.Ordinal);

        using var initialSync = JsonDocument.Parse(requests[1].Body);
        var initialRecords = initialSync.RootElement.GetProperty("records");
        Assert.Equal(new[] { "system", "user" }, initialRecords.EnumerateArray()
            .Select(record => record.GetProperty("role").GetString()).ToArray());

        using var modelRequest = JsonDocument.Parse(requests[3].Body);
        var input = modelRequest.RootElement.GetProperty("input");
        Assert.Equal(new[] { "system", "system", "user" }, input.EnumerateArray()
            .Select(message => message.GetProperty("role").GetString()).ToArray());
        Assert.Equal("durable approval context", input[1].GetProperty("content").GetString());

        using var completionSync = JsonDocument.Parse(requests[4].Body);
        var completionRecords = completionSync.RootElement.GetProperty("records");
        Assert.Equal(new[] { "assistant", "tool" }, completionRecords.EnumerateArray()
            .Select(record => record.GetProperty("role").GetString()).ToArray());
        var assistant = completionRecords[0];
        var tool = completionRecords[1];
        Assert.Equal("call_1", assistant.GetProperty("structured_payload")
            .GetProperty("tool_calls")[0].GetProperty("id").GetString());
        Assert.Equal("call_1", tool.GetProperty("structured_payload")
            .GetProperty("tool_call_id").GetString());
        Assert.Equal("resp_1", assistant.GetProperty("metadata")
            .GetProperty("response_id").GetString());
        Assert.Equal(9, assistant.GetProperty("metadata").GetProperty("provider_usage")
            .GetProperty("input_tokens").GetInt32());
    }

    [Fact]
    public async Task MemoryFailureStopsAutomaticApprovalBeforeCallingTheModel()
    {
        var context = await TestContext.CreateAsync();
        var providerCalls = 0;
        var memory = new FakeApprovalMemoryEngineRecorder
        {
            BeginError = new InvalidOperationException("memory unavailable"),
        };
        var reviewer = context.Reviewer(request =>
        {
            providerCalls++;
            return Task.FromResult(Json(HttpStatusCode.OK, ResponsesToolDecision("approve", "Safe.")));
        }, memory);

        await Assert.ThrowsAsync<InvalidOperationException>(
            () => reviewer.ReviewAsync(context.Request(), context.Risk));

        Assert.Equal(0, providerCalls);
        Assert.Equal(1, memory.BeginCalls);
        Assert.Equal(0, memory.CompleteCalls);
    }

    [Fact]
    public async Task MemoryCompletionFailureSuppressesAutomaticApprovalResult()
    {
        var context = await TestContext.CreateAsync();
        var memory = new FakeApprovalMemoryEngineRecorder
        {
            CompleteError = new InvalidOperationException("memory unavailable"),
        };
        var reviewer = context.Reviewer(
            _ => Task.FromResult(Json(HttpStatusCode.OK, ResponsesToolDecision("approve", "Safe."))),
            memory);

        await Assert.ThrowsAsync<InvalidOperationException>(
            () => reviewer.ReviewAsync(context.Request(), context.Risk));

        Assert.Equal(1, memory.BeginCalls);
        Assert.Equal(1, memory.CompleteCalls);
    }

    [Fact]
    public async Task ConfiguredGatewayUsesResponsesServerCompaction()
    {
        var context = await TestContext.CreateAsync();
        context.Gateway.Model = context.Gateway.Model with
        {
            BaseUrl = "https://new-api.example/v1",
        };
        Uri? endpoint = null;
        string? body = null;
        var reviewer = context.Reviewer(async request =>
        {
            endpoint = request.RequestUri;
            body = await request.Content!.ReadAsStringAsync();
            return Json(HttpStatusCode.OK, ResponsesToolDecision("approve", "Safe."));
        });

        var result = await reviewer.ReviewAsync(context.Request(), context.Risk);

        Assert.Equal(CommandApprovalAiDecisionKind.Approve, result.Decision);
        Assert.Equal("https://new-api.example/v1/responses", endpoint?.AbsoluteUri);
        using var payload = JsonDocument.Parse(body!);
        var root = payload.RootElement;
        Assert.Equal(200_000, root.GetProperty("context_management")[0]
            .GetProperty("compact_threshold").GetInt32());
        Assert.Equal("compaction", root.GetProperty("context_management")[0]
            .GetProperty("type").GetString());
        Assert.Equal("approval_decision", root.GetProperty("tools")[0]
            .GetProperty("name").GetString());
        Assert.False(root.GetProperty("store").GetBoolean());
        Assert.Equal("approval-agent:model-1", root.GetProperty("prompt_cache_key").GetString());
        Assert.False(root.TryGetProperty("messages", out _));
        Assert.False(root.TryGetProperty("previous_response_id", out _));
    }

    [Fact]
    public async Task OfficialOpenAiReviewerRejectsIncompleteResponsesResult()
    {
        var context = await TestContext.CreateAsync();
        context.Gateway.Model = context.Gateway.Model with
        {
            BaseUrl = "https://api.openai.com/v1",
        };
        var payload = ResponsesToolDecision("approve", "Safe.")
            .Replace("\"status\": \"completed\"", "\"status\": \"incomplete\"", StringComparison.Ordinal);
        var reviewer = context.Reviewer(_ => Task.FromResult(Json(HttpStatusCode.OK, payload)));

        await Assert.ThrowsAsync<InvalidOperationException>(
            () => reviewer.ReviewAsync(context.Request(), context.Risk));
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
                : Json(HttpStatusCode.OK, ResponsesToolDecision("deny", "Unsafe.")));
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

    private static string ResponsesToolDecision(
        string decision,
        string reason,
        bool remember = false,
        bool includeUsage = false) => $$"""
        {
          "id": "resp_1",
          "status": "completed",
          "output": [{
            "type": "function_call",
            "call_id": "call_1",
            "name": "approval_decision",
            "arguments": "{\"decision\":\"{{decision}}\",\"reason\":\"{{reason}}\",\"remember_allow\":{{remember.ToString().ToLowerInvariant()}}}"
          }]{{(includeUsage ? ",\n  \"usage\": {\"input_tokens\":9,\"output_tokens\":4}" : string.Empty)}}
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
                    "https://provider.example/v1", "secret-key", true, true, false, null, 0, 900),
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
            Func<HttpRequestMessage, Task<HttpResponseMessage>> response,
            IApprovalMemoryEngineRecorder? memoryRecorder = null) =>
            new(Configuration, Runtime, new FakeHttpClientFactory(
                new HttpClient(new DelegateHandler(response))),
                memoryRecorder ?? new FakeApprovalMemoryEngineRecorder());

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

    private sealed class FakeApprovalMemoryEngineRecorder : IApprovalMemoryEngineRecorder
    {
        public Exception? BeginError { get; init; }
        public Exception? CompleteError { get; init; }
        public int BeginCalls { get; private set; }
        public int CompleteCalls { get; private set; }

        public Task<ApprovalMemoryRun> BeginAsync(
            ApprovalModelRuntimeConfiguration runtime,
            CommandApprovalRequest request,
            string systemPrompt,
            string userPrompt,
            CancellationToken cancellationToken)
        {
            BeginCalls++;
            if (BeginError is not null)
            {
                return Task.FromException<ApprovalMemoryRun>(BeginError);
            }
            return Task.FromResult(new ApprovalMemoryRun(
                request.OwnerUserId,
                "client-agent:approval:test",
                "client-agent:approval-run:test",
                runtime.GatewayBaseUri,
                runtime.ConnectorAccessToken,
                DateTimeOffset.UtcNow,
                []));
        }

        public Task CompleteAsync(
            ApprovalMemoryRun run,
            CommandApprovalRequest request,
            string responsePayload,
            CommandApprovalAiReview decision,
            CancellationToken cancellationToken)
        {
            CompleteCalls++;
            return CompleteError is null
                ? Task.CompletedTask
                : Task.FromException(CompleteError);
        }
    }

    private sealed class DelegateHandler(
        Func<HttpRequestMessage, Task<HttpResponseMessage>> response) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken) => response(request);
    }
}
