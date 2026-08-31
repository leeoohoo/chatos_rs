using System.Net.Http.Headers;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Plugins;

internal sealed class PluginHttpMcpClient(
    PreparedPluginLaunch launch,
    IHttpClientFactory httpClients,
    PluginOAuthBroker oauth) : IPluginMcpClient
{
    private const int MaximumRequestBytes = 8 * 1024 * 1024;
    private const int MaximumResponseBytes = 16 * 1024 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);
    private readonly CancellationTokenSource _lifetime = new();
    private long _nextRequestId;
    private int _started;
    private int _stopped;

    public Task StartAsync(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (Volatile.Read(ref _stopped) != 0)
        {
            throw new PluginRuntimeException("Plugin HTTP MCP session has ended.");
        }

        Interlocked.Exchange(ref _started, 1);
        return Task.CompletedTask;
    }

    public async Task<PluginMcpInitialization> InitializeAsync(
        CancellationToken cancellationToken = default)
    {
        EnsureStarted();
        var timeout = ConnectTimeout();
        var initialized = await RequestAsync(
            "initialize",
            JsonSerializer.SerializeToElement(new
            {
                protocolVersion = "2025-06-18",
                capabilities = new { },
                clientInfo = new { name = "ChatOS Windows", version = "1" },
            }, JsonOptions),
            timeout,
            cancellationToken).ConfigureAwait(false);
        var instructions = initialized.ValueKind == JsonValueKind.Object &&
            initialized.TryGetProperty("instructions", out var instructionValue) &&
            instructionValue.ValueKind == JsonValueKind.String
                ? instructionValue.GetString()?.Trim()
                : null;

        await NotifyAsync(
            "notifications/initialized",
            JsonSerializer.SerializeToElement(new { }, JsonOptions),
            timeout,
            cancellationToken).ConfigureAwait(false);
        var listed = await RequestAsync(
            "tools/list",
            JsonSerializer.SerializeToElement(new { }, JsonOptions),
            timeout,
            cancellationToken).ConfigureAwait(false);
        if (listed.ValueKind != JsonValueKind.Object ||
            !listed.TryGetProperty("tools", out var toolsValue) ||
            toolsValue.ValueKind != JsonValueKind.Array)
        {
            throw new PluginRuntimeException("Plugin HTTP MCP did not publish a valid tool list.");
        }

        var tools = toolsValue.EnumerateArray().Select(value => value.Clone()).ToArray();
        if (tools.Length == 0)
        {
            throw new PluginRuntimeException("Plugin HTTP MCP did not publish any tools.");
        }

        return new PluginMcpInitialization(
            string.IsNullOrWhiteSpace(instructions) ? null : instructions,
            tools);
    }

    public Task<JsonElement> CallToolAsync(
        string name,
        JsonElement arguments,
        TimeSpan timeout,
        CancellationToken cancellationToken = default) =>
        RequestAsync(
            "tools/call",
            JsonSerializer.SerializeToElement(new { name, arguments }, JsonOptions),
            timeout,
            cancellationToken);

    public Task TerminateAsync()
    {
        if (Interlocked.Exchange(ref _stopped, 1) == 0)
        {
            _lifetime.Cancel();
        }

        return Task.CompletedTask;
    }

    public async ValueTask DisposeAsync()
    {
        await TerminateAsync().ConfigureAwait(false);
        _lifetime.Dispose();
    }

    private async Task<JsonElement> RequestAsync(
        string method,
        JsonElement parameters,
        TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        var id = Interlocked.Increment(ref _nextRequestId);
        var envelope = JsonSerializer.SerializeToUtf8Bytes(new
        {
            jsonrpc = "2.0",
            id,
            method,
            @params = parameters,
        }, JsonOptions);
        var response = await SendAsync(envelope, timeout, allowEmpty: false, cancellationToken)
            .ConfigureAwait(false);
        if (response.ValueKind != JsonValueKind.Object)
        {
            throw new PluginRuntimeException("Plugin HTTP MCP returned an invalid JSON-RPC response.");
        }

        if (response.TryGetProperty("id", out var responseId) &&
            responseId.ValueKind == JsonValueKind.Number &&
            responseId.TryGetInt64(out var actualId) &&
            actualId != id)
        {
            throw new PluginRuntimeException("Plugin HTTP MCP returned a mismatched request id.");
        }

        if (response.TryGetProperty("error", out var error) && error.ValueKind == JsonValueKind.Object)
        {
            var message = error.TryGetProperty("message", out var value) &&
                value.ValueKind == JsonValueKind.String
                    ? value.GetString()
                    : null;
            throw new PluginRuntimeException(message ?? "Plugin HTTP MCP call failed.");
        }

        return response.TryGetProperty("result", out var result)
            ? result.Clone()
            : throw new PluginRuntimeException("Plugin HTTP MCP response is missing result.");
    }

    private async Task NotifyAsync(
        string method,
        JsonElement parameters,
        TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        var envelope = JsonSerializer.SerializeToUtf8Bytes(new
        {
            jsonrpc = "2.0",
            method,
            @params = parameters,
        }, JsonOptions);
        _ = await SendAsync(envelope, timeout, allowEmpty: true, cancellationToken).ConfigureAwait(false);
    }

    private async Task<JsonElement> SendAsync(
        byte[] body,
        TimeSpan timeout,
        bool allowEmpty,
        CancellationToken cancellationToken)
    {
        EnsureStarted();
        if (body.Length > MaximumRequestBytes)
        {
            throw new PluginRuntimeException("Plugin HTTP MCP request exceeds the 8 MB limit.");
        }

        using var linked = CancellationTokenSource.CreateLinkedTokenSource(
            cancellationToken,
            _lifetime.Token);
        linked.CancelAfter(TimeSpan.FromMilliseconds(Math.Clamp(
            timeout.TotalMilliseconds,
            300,
            7_200_000)));
        using var request = new HttpRequestMessage(
            HttpMethod.Post,
            launch.HttpEndpoint ?? throw new PluginRuntimeException("Plugin HTTP MCP endpoint is unavailable."))
        {
            Content = new ByteArrayContent(body),
        };
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        request.Content.Headers.ContentType = new MediaTypeHeaderValue("application/json");
        await ApplyHeadersAsync(request, linked.Token).ConfigureAwait(false);

        HttpResponseMessage response;
        try
        {
            response = await httpClients.CreateClient(PluginHttpMcpProxy.HttpClientName)
                .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, linked.Token)
                .ConfigureAwait(false);
        }
        catch (OperationCanceledException exception) when (!cancellationToken.IsCancellationRequested &&
                                                            !_lifetime.IsCancellationRequested)
        {
            throw new PluginRuntimeException("Plugin HTTP MCP call timed out.", exception);
        }

        using (response)
        {
            if (!response.IsSuccessStatusCode)
            {
                throw new PluginRuntimeException(
                    $"Plugin HTTP MCP request failed with status {(int)response.StatusCode}.");
            }

            if (response.Content.Headers.ContentLength is > MaximumResponseBytes)
            {
                throw new PluginRuntimeException("Plugin HTTP MCP response exceeds the 16 MB limit.");
            }

            await using var stream = await response.Content.ReadAsStreamAsync(linked.Token).ConfigureAwait(false);
            using var buffer = new MemoryStream();
            var chunk = new byte[64 * 1024];
            while (true)
            {
                var read = await stream.ReadAsync(chunk, linked.Token).ConfigureAwait(false);
                if (read == 0)
                {
                    break;
                }

                if (buffer.Length + read > MaximumResponseBytes)
                {
                    throw new PluginRuntimeException("Plugin HTTP MCP response exceeds the 16 MB limit.");
                }

                buffer.Write(chunk, 0, read);
            }

            if (buffer.Length == 0 && allowEmpty)
            {
                return JsonSerializer.SerializeToElement<object?>(null, JsonOptions);
            }

            try
            {
                using var document = JsonDocument.Parse(buffer.ToArray());
                return document.RootElement.Clone();
            }
            catch (JsonException exception)
            {
                throw new PluginRuntimeException("Plugin HTTP MCP returned invalid JSON.", exception);
            }
        }
    }

    private async Task ApplyHeadersAsync(HttpRequestMessage request, CancellationToken cancellationToken)
    {
        foreach (var pair in launch.HttpHeaderTemplates)
        {
            var template = pair.Value;
            var value = template.Prefix;
            if (template.SecretName is not null)
            {
                var binding = launch.CredentialBinding
                    ?? throw new PluginRuntimeException("Plugin HTTP MCP credential binding is unavailable.");
                var secret = await binding.ResolveAsync(template.SecretName, cancellationToken).ConfigureAwait(false);
                value = template.Prefix + secret + template.Suffix;
            }

            if (value.Any(char.IsControl))
            {
                throw new PluginRuntimeException("Resolved Plugin HTTP MCP header contains control characters.");
            }

            if (pair.Key.Equals("content-type", StringComparison.OrdinalIgnoreCase))
            {
                request.Content!.Headers.ContentType = MediaTypeHeaderValue.Parse(value);
            }
            else if (pair.Key.Equals("accept", StringComparison.OrdinalIgnoreCase))
            {
                request.Headers.Accept.Clear();
                request.Headers.Accept.ParseAdd(value);
            }
            else if (!request.Headers.TryAddWithoutValidation(pair.Key, value))
            {
                request.Content!.Headers.TryAddWithoutValidation(pair.Key, value);
            }
        }

        if (launch.OAuthBinding is not null)
        {
            var accessToken = await oauth.ResolveAccessTokenAsync(launch.OAuthBinding, cancellationToken)
                .ConfigureAwait(false);
            request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", accessToken);
        }
    }

    private TimeSpan ConnectTimeout() => TimeSpan.FromMilliseconds(Math.Clamp(
        launch.Server.ConnectTimeoutMilliseconds ?? 30_000,
        300,
        120_000));

    private void EnsureStarted()
    {
        if (Volatile.Read(ref _started) == 0 || Volatile.Read(ref _stopped) != 0)
        {
            throw new PluginRuntimeException("Plugin HTTP MCP session is unavailable.");
        }
    }
}
