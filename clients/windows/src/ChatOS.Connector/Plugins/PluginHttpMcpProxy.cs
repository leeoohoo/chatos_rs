using System.Net;
using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Relay;

namespace ChatOS.Connector.Plugins;

internal sealed class PluginHttpMcpProxy(IHttpClientFactory httpClientFactory) : IRelayRequestHandler
{
    internal const string HttpClientName = "ChatOS.PluginHttpMcp";
    internal const int MaximumResponseBytes = 16 * 1024 * 1024;
    private const string RuntimeHeader = "x-local-connector-inline-mcp-runtime";
    private const string ResourceHeader = "x-plugin-management-resource-id";
    private static readonly HashSet<string> ForbiddenHeaders = new(StringComparer.OrdinalIgnoreCase)
    {
        "accept", "connection", "content-length", "content-type", "host",
        "proxy-authenticate", "proxy-authorization", "te", "trailer",
        "transfer-encoding", "upgrade", RuntimeHeader, ResourceHeader,
    };
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    public bool CanHandle(string requestType) => requestType == "mcp";

    public string ResponseType(string requestType) => "mcp";

    public async Task<RelayHandlerResult> HandleAsync(
        RelayRequest request,
        CancellationToken cancellationToken)
    {
        if (request.Header(ResourceHeader) is null || request.Header(RuntimeHeader) is not { } encodedRuntime)
        {
            throw new RelayRequestException(400, "Local HTTP MCP runtime configuration is missing.");
        }

        HttpMcpRuntime runtime;
        try
        {
            var decoded = Uri.UnescapeDataString(encodedRuntime);
            runtime = JsonSerializer.Deserialize<HttpMcpRuntime>(decoded, JsonOptions)
                ?? throw new JsonException("Runtime is empty.");
        }
        catch (Exception exception) when (exception is JsonException or UriFormatException)
        {
            throw new RelayRequestException(400, "Local HTTP MCP runtime configuration is invalid.");
        }

        var endpoint = ValidateEndpoint(runtime.Url);
        if (request.Body.ValueKind != JsonValueKind.Object ||
            !request.Body.TryGetProperty("method", out var methodValue) ||
            methodValue.ValueKind != JsonValueKind.String ||
            methodValue.GetString() is not ("tools/list" or "tools/call"))
        {
            throw new RelayRequestException(400, "HTTP MCP only supports tools/list and tools/call.");
        }

        ValidateHeaders(runtime.Headers);
        using var outbound = new HttpRequestMessage(HttpMethod.Post, endpoint)
        {
            Content = new ByteArrayContent(JsonSerializer.SerializeToUtf8Bytes(request.Body, JsonOptions)),
        };
        outbound.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        outbound.Content.Headers.ContentType = new MediaTypeHeaderValue("application/json");
        foreach (var pair in runtime.Headers)
        {
            if (!outbound.Headers.TryAddWithoutValidation(pair.Key, pair.Value))
            {
                outbound.Content.Headers.TryAddWithoutValidation(pair.Key, pair.Value);
            }
        }

        var timeout = TimeSpan.FromMilliseconds(Math.Clamp(runtime.TimeoutMilliseconds, 300, 120_000));
        using var timeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeoutSource.CancelAfter(timeout);
        HttpResponseMessage response;
        try
        {
            response = await httpClientFactory.CreateClient(HttpClientName)
                .SendAsync(outbound, HttpCompletionOption.ResponseHeadersRead, timeoutSource.Token)
                .ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new RelayRequestException(408, "HTTP MCP request timed out.");
        }

        using (response)
        {
            if (!response.IsSuccessStatusCode)
            {
                throw new RelayRequestException(
                    502,
                    $"HTTP MCP request failed with status {(int)response.StatusCode}.");
            }

            if (response.Content.Headers.ContentLength is > MaximumResponseBytes)
            {
                throw new RelayRequestException(502, "HTTP MCP response exceeds the 16 MB limit.");
            }

            await using var stream = await response.Content.ReadAsStreamAsync(timeoutSource.Token)
                .ConfigureAwait(false);
            using var buffer = new MemoryStream();
            var chunk = new byte[64 * 1024];
            while (true)
            {
                var read = await stream.ReadAsync(chunk, timeoutSource.Token).ConfigureAwait(false);
                if (read == 0)
                {
                    break;
                }

                if (buffer.Length + read > MaximumResponseBytes)
                {
                    throw new RelayRequestException(502, "HTTP MCP response exceeds the 16 MB limit.");
                }

                buffer.Write(chunk, 0, read);
            }

            try
            {
                using var document = JsonDocument.Parse(buffer.ToArray());
                return RelayHandlerResult.Ok(document.RootElement.Clone());
            }
            catch (JsonException exception)
            {
                throw new RelayRequestException(502, $"HTTP MCP returned invalid JSON: {exception.Message}");
            }
        }
    }

    private static Uri ValidateEndpoint(string value)
    {
        if (!Uri.TryCreate(value.Trim(), UriKind.Absolute, out var uri) ||
            !string.IsNullOrEmpty(uri.UserInfo) ||
            !string.IsNullOrEmpty(uri.Fragment) ||
            string.IsNullOrWhiteSpace(uri.Host))
        {
            throw new RelayRequestException(400, "HTTP MCP endpoint is invalid.");
        }

        if (uri.Scheme.Equals(Uri.UriSchemeHttps, StringComparison.OrdinalIgnoreCase))
        {
            return uri;
        }

        if (uri.Scheme.Equals(Uri.UriSchemeHttp, StringComparison.OrdinalIgnoreCase) && IsLoopback(uri.Host))
        {
            return uri;
        }

        throw new RelayRequestException(400, "HTTP MCP must use HTTPS or a loopback HTTP endpoint.");
    }

    private static bool IsLoopback(string host)
    {
        if (host.Equals("localhost", StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }

        return IPAddress.TryParse(host.Trim('[', ']'), out var address) && IPAddress.IsLoopback(address);
    }

    private static void ValidateHeaders(IReadOnlyDictionary<string, string> headers)
    {
        if (headers.Count > 64 || headers.Sum(pair =>
                Encoding.UTF8.GetByteCount(pair.Key) + Encoding.UTF8.GetByteCount(pair.Value)) > 32 * 1024)
        {
            throw new RelayRequestException(400, "HTTP MCP headers exceed the configured limit.");
        }

        foreach (var pair in headers)
        {
            if (string.IsNullOrWhiteSpace(pair.Key) ||
                pair.Key.Any(character => !(char.IsAsciiLetterOrDigit(character) || character is '-' or '_')) ||
                pair.Value.Contains('\r') ||
                pair.Value.Contains('\n') ||
                ForbiddenHeaders.Contains(pair.Key.Trim()))
            {
                throw new RelayRequestException(400, "HTTP MCP contains an unsafe header.");
            }
        }
    }

    private sealed record HttpMcpRuntime
    {
        [System.Text.Json.Serialization.JsonPropertyName("url")]
        public required string Url { get; init; }

        [System.Text.Json.Serialization.JsonPropertyName("headers")]
        public IReadOnlyDictionary<string, string> Headers { get; init; } =
            new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);

        [System.Text.Json.Serialization.JsonPropertyName("timeout_ms")]
        public int TimeoutMilliseconds { get; init; } = 30_000;
    }
}
