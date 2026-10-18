using System.Text.Json;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.Tests;

public sealed class WindowsExternalToolSchemaValidatorTests
{
    [Theory]
    [InlineData("{}", "$.url")]
    [InlineData("{\"url\":12}", "$.url")]
    [InlineData("{\"url\":\"\"}", "$.url")]
    [InlineData("{\"url\":\"ok\",\"options\":{\"timeout\":true}}", "$.options.timeout")]
    [InlineData("{\"url\":\"ok\",\"unknown\":1}", "$.unknown")]
    public void RequiredNestedAndAdditionalPropertyFailuresExposeCorrectablePaths(
        string arguments,
        string expectedPath)
    {
        var schema = Element("""
            {
              "type": "object",
              "properties": {
                "url": { "type": "string", "minLength": 1 },
                "options": {
                  "type": "object",
                  "properties": { "timeout": { "type": "integer", "minimum": 1 } }
                }
              },
              "required": ["url"],
              "additionalProperties": false
            }
            """);

        var exception = Assert.Throws<WindowsExternalToolSchemaException>(() =>
            WindowsExternalToolSchemaValidator.Validate(Element(arguments), schema));

        Assert.False(exception.IsUnsupported);
        Assert.Contains(expectedPath, exception.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void LocalReferencesUnionsAndCombinatorsMatchMacContract()
    {
        var schema = Element("""
            {
              "$schema": "https://json-schema.org/draft/2020-12/schema",
              "$defs": { "query": { "type": ["string", "null"] } },
              "type": "object",
              "properties": {
                "query": { "$ref": "#/$defs/query" },
                "action": {
                  "oneOf": [
                    { "type": "object", "properties": { "kind": { "const": "read" } }, "required": ["kind"] },
                    { "type": "object", "properties": { "kind": { "const": "write" } }, "required": ["kind"] }
                  ]
                }
              },
              "required": ["query", "action"]
            }
            """);

        WindowsExternalToolSchemaValidator.Validate(
            Element("{\"query\":null,\"action\":{\"kind\":\"read\"}}"), schema);
        WindowsExternalToolSchemaValidator.Validate(
            Element("{\"query\":\"find\",\"action\":{\"kind\":\"write\"}}"), schema);
        Assert.Throws<WindowsExternalToolSchemaException>(() =>
            WindowsExternalToolSchemaValidator.Validate(
                Element("{\"query\":false,\"action\":{\"kind\":\"read\"}}"), schema));
        Assert.Throws<WindowsExternalToolSchemaException>(() =>
            WindowsExternalToolSchemaValidator.Validate(
                Element("{\"query\":\"find\",\"action\":{\"kind\":\"remove\"}}"), schema));

        var combinators = Element("""
            { "allOf": [{ "type": "integer" }, { "minimum": 1 }],
              "anyOf": [{ "maximum": 3 }, { "const": 4 }] }
            """);
        WindowsExternalToolSchemaValidator.Validate(Element("4"), combinators);
        Assert.Throws<WindowsExternalToolSchemaException>(() =>
            WindowsExternalToolSchemaValidator.Validate(Element("5"), combinators));
    }

    [Fact]
    public void ArraysUnicodePatternsNumbersAndBooleanSchemasAreEnforced()
    {
        WindowsExternalToolSchemaValidator.Validate(
            Element("[\"a\",1]"),
            Element("{\"type\":\"array\",\"prefixItems\":[{\"type\":\"string\"}],\"items\":{\"type\":\"integer\"},\"minItems\":2}"));
        WindowsExternalToolSchemaValidator.Validate(
            Element("\"😀\""), Element("{\"type\":\"string\",\"minLength\":1,\"maxLength\":1,\"pattern\":\"^😀$\"}"));
        WindowsExternalToolSchemaValidator.Validate(Element("null"), Element("{\"type\":[\"string\",\"null\"]}"));
        WindowsExternalToolSchemaValidator.Validate(Element("{}"), Element("true"));

        Assert.Throws<WindowsExternalToolSchemaException>(() =>
            WindowsExternalToolSchemaValidator.Validate(
                Element("[\"a\",\"b\"]"),
                Element("{\"type\":\"array\",\"prefixItems\":[{\"type\":\"string\"}],\"items\":{\"type\":\"integer\"}}")));
        Assert.Throws<WindowsExternalToolSchemaException>(() =>
            WindowsExternalToolSchemaValidator.Validate(Element("[1,1.0]"), Element("{\"type\":\"array\",\"uniqueItems\":true}")));
        Assert.Throws<WindowsExternalToolSchemaException>(() =>
            WindowsExternalToolSchemaValidator.Validate(Element("{}"), Element("false")));
    }

    [Theory]
    [InlineData("{\"$ref\":\"https://example.test/schema\"}")]
    [InlineData("{\"unknownAssertion\":true}")]
    [InlineData("{\"$ref\":\"#\"}")]
    [InlineData("{\"$ref\":\"#/$defs/missing\"}")]
    [InlineData("{\"type\":\"string\",\"pattern\":\"[\"}")]
    public void UnsupportedSchemasFailClosed(string schema)
    {
        var exception = Assert.Throws<WindowsExternalToolSchemaException>(() =>
            WindowsExternalToolSchemaValidator.Validate(Element("\"value\""), Element(schema)));

        Assert.True(exception.IsUnsupported);
        Assert.Contains("call was not executed", exception.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void SchemaFailuresAreSafeAndDoNotRequireUnknownSideEffectReview()
    {
        var exception = WindowsExternalToolSchemaException.Mismatch(
            "$.query", "required field is missing");

        Assert.Equal(exception.Message,
            WindowsLocalAgentPlatformToolWorker.SafeErrorSummary(exception));
        Assert.DoesNotContain("password", exception.Message, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void CapabilityDisclosureFailuresExposeCorrectableButNonSensitiveGuidance()
    {
        var exception = new WindowsAgentCapabilityDisclosureException();

        var summary = WindowsLocalAgentPlatformToolWorker.SafeErrorSummary(exception);

        Assert.Equal(exception.Message, summary);
        Assert.Contains("tool_option", summary, StringComparison.Ordinal);
        Assert.Contains("schema_offset", summary, StringComparison.Ordinal);
        Assert.DoesNotContain("password", summary, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public async Task ExternalMcpInvalidArgumentsAreRejectedBeforeHttpRequest()
    {
        var factory = new CountingHttpClientFactory();
        var executor = new WindowsLocalAgentExternalMcpExecutor(
            new SelectedExternalMcpHost(), factory);
        var configured = executor.Configure(
            [new ConnectorResolvedMcp(
                new ConnectorMcpResource(
                    "mcp-1",
                    "MCP",
                    "MCP",
                    null,
                    true,
                    new ConnectorMcpRuntime(
                        "http", null, null, "server", "https://example.test/mcp",
                        new Dictionary<string, string>())),
                new ConnectorCapabilityBinding(true, true),
                true,
                "ready",
                [Element("""
                    {
                      "name": "search",
                      "inputSchema": {
                        "type": "object",
                        "properties": { "query": { "type": "string" } },
                        "required": ["query"],
                        "additionalProperties": false
                      }
                    }
                    """)])],
            new HashSet<string>(StringComparer.Ordinal) { "mcp-1" });
        var tool = Assert.Single(configured);
        var invocation = new WindowsLocalToolInvocation(
            "invocation-1", "run-1", "batch-1", "call-1", tool.PublicName,
            Element("{}"), true, false, "not_required", null, null, null,
            "claimed", null, null, 1, "claim-1", null, 1, 1);

        var exception = await Assert.ThrowsAsync<WindowsExternalToolSchemaException>(() =>
            executor.ExecuteAsync("user-1", invocation, CancellationToken.None));

        Assert.Contains("$.query", exception.Message, StringComparison.Ordinal);
        Assert.Equal(0, factory.RequestCount);
    }

    private static JsonElement Element(string json)
    {
        using var document = JsonDocument.Parse(json);
        return document.RootElement.Clone();
    }

    private sealed class SelectedExternalMcpHost : ILocalAgentHostClient
    {
        public string? ActiveOwnerUserId => "user-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

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
                    "run-1",
                    "user-1",
                    "conversation_turn",
                    "turn-1",
                    "main_chat",
                    JsonSerializer.SerializeToElement(new
                    {
                        tool_options = new { external_mcp_config_ids = new[] { "mcp-1" } },
                    }),
                    "running",
                    1,
                    null,
                    1,
                    1));
                return Task.FromResult((TResponse)response);
            }
            Assert.Equal("get_run_skill_bundle", type);
            var json = """{"type":"run_skill_bundle","run_id":"run-1","bundle":null}""";
            return Task.FromResult(JsonSerializer.Deserialize<TResponse>(json,
                new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower })!);
        }
    }

    private sealed class CountingHttpClientFactory : IHttpClientFactory
    {
        public int RequestCount { get; private set; }

        public HttpClient CreateClient(string name) => new(new CountingHandler(this));

        private sealed class CountingHandler(CountingHttpClientFactory owner) : HttpMessageHandler
        {
            protected override Task<HttpResponseMessage> SendAsync(
                HttpRequestMessage request,
                CancellationToken cancellationToken)
            {
                owner.RequestCount++;
                return Task.FromResult(new HttpResponseMessage(System.Net.HttpStatusCode.OK));
            }
        }
    }
}
