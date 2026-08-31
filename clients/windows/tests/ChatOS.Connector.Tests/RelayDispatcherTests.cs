using System.Text.Json;
using ChatOS.Connector.Relay;

namespace ChatOS.Connector.Tests;

public sealed class RelayDispatcherTests
{
    [Fact]
    public async Task AllowsDeviceScopedPluginRequestWithoutWorkspaceId()
    {
        var handler = new CapturingHandler("plugin_prepare_request", "plugin_prepare_response");
        var dispatcher = new RelayDispatcher([handler], new StubVerifier([]));

        var response = await dispatcher.DispatchAsync("""
            {"type":"plugin_prepare_request","request_id":"request-1","workspace_id":"","body":{}}
            """);

        Assert.Equal(200, response.Status);
        Assert.Equal("plugin_prepare_response", response.Type);
    }
    [Fact]
    public async Task VerifiesBeforeDispatchingAndReturnsHandlerResponseType()
    {
        var sequence = new List<string>();
        var dispatcher = new RelayDispatcher(
            [new StubHandler(sequence)],
            new StubVerifier(sequence));

        var response = await dispatcher.DispatchAsync(RequestPayload("workspace_filesystem_request"));

        Assert.Equal(200, response.Status);
        Assert.Equal("workspace_filesystem_response", response.Type);
        Assert.Equal("request-1", response.RequestId);
        Assert.Equal(["verify", "handle"], sequence);
    }

    [Fact]
    public async Task UnsupportedRequestPreservesCorrelationIdentity()
    {
        var dispatcher = new RelayDispatcher([], new StubVerifier([]));

        var response = await dispatcher.DispatchAsync(RequestPayload("unknown_request"));

        Assert.Equal(400, response.Status);
        Assert.Equal("request-1", response.RequestId);
        Assert.Contains("Unsupported", response.Body.GetProperty("error").GetString());
    }

    [Fact]
    public async Task VerificationFailureNeverInvokesHandler()
    {
        var sequence = new List<string>();
        var dispatcher = new RelayDispatcher(
            [new StubHandler(sequence)],
            new RejectingVerifier());

        var response = await dispatcher.DispatchAsync(RequestPayload("workspace_filesystem_request"));

        Assert.Equal(403, response.Status);
        Assert.Empty(sequence);
    }

    [Fact]
    public async Task MalformedPayloadReturnsStructuredResponse()
    {
        var dispatcher = new RelayDispatcher([], new StubVerifier([]));

        var response = await dispatcher.DispatchAsync("{not-json");

        Assert.Equal(400, response.Status);
        Assert.Equal(string.Empty, response.RequestId);
        Assert.False(string.IsNullOrWhiteSpace(response.Body.GetProperty("error").GetString()));
    }

    private static string RequestPayload(string type) => $$"""
        {
          "type": "{{type}}",
          "request_id": "request-1",
          "workspace_id": "workspace-1",
          "headers": {},
          "body": { "operation": "list" }
        }
        """;

    private sealed class StubVerifier(List<string> sequence) : IRelayRequestVerifier
    {
        public Task VerifyAsync(RelayRequest request, CancellationToken cancellationToken)
        {
            sequence.Add("verify");
            return Task.CompletedTask;
        }
    }

    private sealed class RejectingVerifier : IRelayRequestVerifier
    {
        public Task VerifyAsync(RelayRequest request, CancellationToken cancellationToken) =>
            throw new RelayRequestException(403, "signature rejected");
    }

    private sealed class StubHandler(List<string> sequence) : IRelayRequestHandler
    {
        public bool CanHandle(string requestType) => requestType == "workspace_filesystem_request";

        public string ResponseType(string requestType) => "workspace_filesystem_response";

        public Task<RelayHandlerResult> HandleAsync(
            RelayRequest request,
            CancellationToken cancellationToken)
        {
            sequence.Add("handle");
            return Task.FromResult(RelayHandlerResult.Ok(
                JsonSerializer.SerializeToElement(new { ok = true })));
        }
    }

    private sealed class CapturingHandler(string requestType, string responseType) : IRelayRequestHandler
    {
        public bool CanHandle(string value) => value == requestType;

        public string ResponseType(string value) => responseType;

        public Task<RelayHandlerResult> HandleAsync(
            RelayRequest request,
            CancellationToken cancellationToken) =>
            Task.FromResult(RelayHandlerResult.Ok(
                JsonSerializer.SerializeToElement(new { ok = true })));
    }
}
