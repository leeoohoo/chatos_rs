using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.AgentTeams;

internal sealed record AgentToolDefinition(
    string Name,
    string Description,
    object Parameters);

internal sealed record AgentToolCall(string Id, string Name, string Arguments);

internal sealed record AgentModelTurn(
    string Content,
    IReadOnlyList<AgentToolCall> ToolCalls,
    IReadOnlyList<JsonElement> OutputItems);

internal sealed class AgentTeamModelGateway(
    ChatOSApiClient apiClient,
    IHttpClientFactory httpClientFactory)
{
    public const string HttpClientName = "AgentTeamModel";
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public async Task<AgentModelTurn> CompleteAsync(
        AgentProfile profile,
        IReadOnlyList<object> input,
        IReadOnlyList<AgentToolDefinition> tools,
        CancellationToken cancellationToken)
    {
        var config = await apiClient.GetAsync<AgentModelConfigurationDto>(
            $"ai-model-configs/{Uri.EscapeDataString(profile.Draft.ModelConfigId)}?include_secret=true",
            cancellationToken).ConfigureAwait(false);
        var endpoint = ResponsesEndpoint(config);
        if (config.Enabled == false || string.IsNullOrWhiteSpace(config.ApiKey) ||
            string.IsNullOrWhiteSpace(config.Model))
        {
            throw new AgentTeamException(AgentTeamError.ModelUnavailable,
                "The Agent model configuration is disabled or has no usable credential.");
        }

        var body = new Dictionary<string, object?>
        {
            ["model"] = config.Model,
            ["input"] = input,
            ["tools"] = tools.Select(static tool => new Dictionary<string, object>
            {
                ["type"] = "function",
                ["name"] = tool.Name,
                ["description"] = tool.Description,
                ["parameters"] = tool.Parameters,
            }).ToArray(),
            ["tool_choice"] = "auto",
            ["store"] = false,
            ["max_output_tokens"] = 16_384,
            ["prompt_cache_key"] = $"windows-agent-team:{profile.Id}:{profile.Draft.ModelConfigId}",
        };
        var thinking = profile.Draft.ThinkingLevel?.Trim().ToLowerInvariant();
        if (!string.IsNullOrWhiteSpace(thinking) && thinking != "auto" && thinking != "none")
        {
            body["reasoning"] = new Dictionary<string, object> { ["effort"] = thinking };
            body["include"] = new[] { "reasoning.encrypted_content" };
        }

        using var request = new HttpRequestMessage(HttpMethod.Post, endpoint)
        {
            Content = JsonContent.Create(body, options: JsonOptions),
        };
        request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", config.ApiKey);
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromMinutes(3));
        using var response = await httpClientFactory.CreateClient(HttpClientName)
            .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, timeout.Token)
            .ConfigureAwait(false);
        var payload = await ReadBoundedAsync(response.Content, timeout.Token).ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            throw ProviderFailure(response.StatusCode, payload);
        }

        return Decode(payload);
    }

    private static Uri ResponsesEndpoint(AgentModelConfigurationDto config)
    {
        if (!Uri.TryCreate(config.BaseUrl?.Trim(), UriKind.Absolute, out var baseUri) ||
            baseUri.Scheme is not ("http" or "https") || string.IsNullOrWhiteSpace(baseUri.Host) ||
            !string.IsNullOrEmpty(baseUri.UserInfo))
        {
            throw new AgentTeamException(AgentTeamError.ModelUnavailable,
                "The Agent model endpoint is invalid.");
        }

        var path = baseUri.AbsolutePath.TrimEnd('/');
        if (path.EndsWith("/chat/completions", StringComparison.OrdinalIgnoreCase))
        {
            path = path[..^"/chat/completions".Length];
        }
        else if (path.EndsWith("/responses", StringComparison.OrdinalIgnoreCase))
        {
            path = path[..^"/responses".Length];
        }

        var builder = new UriBuilder(baseUri)
        {
            Path = $"{path}/responses",
            Query = string.Empty,
            Fragment = string.Empty,
        };
        return builder.Uri;
    }

    private static async Task<byte[]> ReadBoundedAsync(
        HttpContent content,
        CancellationToken cancellationToken)
    {
        await using var stream = await content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        using var output = new MemoryStream();
        var buffer = new byte[16 * 1024];
        while (true)
        {
            var read = await stream.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (read == 0)
            {
                break;
            }

            if (output.Length + read > 4L * 1024 * 1024)
            {
                throw new AgentTeamException(AgentTeamError.ModelUnavailable,
                    "The Agent model returned an oversized response.");
            }

            output.Write(buffer, 0, read);
        }

        return output.ToArray();
    }

    private static AgentModelTurn Decode(byte[] payload)
    {
        using var document = JsonDocument.Parse(payload);
        var root = document.RootElement;
        var status = root.TryGetProperty("status", out var statusValue)
            ? statusValue.GetString()
            : "completed";
        if (!string.Equals(status, "completed", StringComparison.OrdinalIgnoreCase))
        {
            throw new AgentTeamException(AgentTeamError.ModelUnavailable,
                $"The Agent model ended with status '{SafeToken(status)}'.");
        }

        if (!root.TryGetProperty("output", out var output) || output.ValueKind != JsonValueKind.Array)
        {
            throw new AgentTeamException(AgentTeamError.ModelUnavailable,
                "The Agent model returned an invalid Responses envelope.");
        }

        var content = new List<string>();
        var calls = new List<AgentToolCall>();
        var items = new List<JsonElement>();
        foreach (var item in output.EnumerateArray())
        {
            items.Add(item.Clone());
            var type = item.TryGetProperty("type", out var typeValue)
                ? typeValue.GetString()
                : null;
            if (type == "message" && item.TryGetProperty("content", out var parts))
            {
                foreach (var part in parts.EnumerateArray())
                {
                    var partType = part.TryGetProperty("type", out var partTypeValue)
                        ? partTypeValue.GetString()
                        : null;
                    if (partType is "output_text" or "text" &&
                        part.TryGetProperty("text", out var textValue) &&
                        textValue.GetString() is { Length: > 0 } text)
                    {
                        content.Add(text);
                    }
                }
            }
            else if (type == "function_call")
            {
                var id = item.TryGetProperty("call_id", out var callId)
                    ? callId.GetString()
                    : item.TryGetProperty("id", out var itemId) ? itemId.GetString() : null;
                var name = item.TryGetProperty("name", out var nameValue) ? nameValue.GetString() : null;
                var arguments = item.TryGetProperty("arguments", out var argumentsValue)
                    ? argumentsValue.GetString()
                    : null;
                if (string.IsNullOrWhiteSpace(id) || string.IsNullOrWhiteSpace(name) || arguments is null)
                {
                    throw new AgentTeamException(AgentTeamError.ModelUnavailable,
                        "The Agent model returned an invalid function call.");
                }

                calls.Add(new AgentToolCall(id, name, arguments));
            }
        }

        return new AgentModelTurn(string.Join("", content), calls, items);
    }

    private static AgentTeamException ProviderFailure(
        System.Net.HttpStatusCode status,
        byte[] payload)
    {
        var category = status switch
        {
            System.Net.HttpStatusCode.Unauthorized => "rejected the configured credential",
            System.Net.HttpStatusCode.TooManyRequests => "is currently rate limited",
            System.Net.HttpStatusCode.BadGateway or
            System.Net.HttpStatusCode.ServiceUnavailable or
            System.Net.HttpStatusCode.GatewayTimeout => "is temporarily unavailable",
            _ => $"failed with HTTP {(int)status}",
        };
        if (payload.Length > 0)
        {
            try
            {
                using var document = JsonDocument.Parse(payload);
                if (document.RootElement.TryGetProperty("error", out var error) &&
                    error.TryGetProperty("code", out var code))
                {
                    category += $" ({SafeToken(code.GetString())})";
                }
            }
            catch (JsonException)
            {
            }
        }

        return new AgentTeamException(AgentTeamError.ModelUnavailable,
            $"The Agent model provider {category}.");
    }

    private static string SafeToken(string? value)
    {
        if (string.IsNullOrWhiteSpace(value) || value.Length > 80 ||
            value.Any(character => !char.IsLetterOrDigit(character) && character is not ('.' or '_' or '-')))
        {
            return "unknown";
        }

        return value;
    }

    private sealed record AgentModelConfigurationDto(
        [property: JsonPropertyName("enabled")] bool Enabled,
        [property: JsonPropertyName("model")] string Model,
        [property: JsonPropertyName("provider")] string? Provider,
        [property: JsonPropertyName("api_key")] string? ApiKey,
        [property: JsonPropertyName("base_url")] string? BaseUrl);
}
