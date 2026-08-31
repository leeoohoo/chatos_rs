using System.Net;
using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Text;
using System.Text.Json;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Approval;

internal sealed class OpenAiCompatibleCommandApprovalReviewer(
    ApprovalModelRuntimeConfigurationService configuration,
    IConnectorWorkspaceCatalog workspaces,
    IHttpClientFactory httpClientFactory) : ICommandApprovalAiReviewer
{
    internal const string HttpClientName = "ChatOS.WindowsApprovalReviewer";
    private const int MaximumResponseBytes = 256 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public async Task<CommandApprovalAiReview> ReviewAsync(
        CommandApprovalRequest request,
        ConnectorApprovalRisk risk,
        CancellationToken cancellationToken = default)
    {
        var runtime = await configuration.ResolveAsync(cancellationToken).ConfigureAwait(false);
        var userPrompt = BuildUserPrompt(request, risk);
        Exception? lastError = null;
        for (var attempt = 0; attempt <= runtime.MaximumTransientRetries; attempt++)
        {
            try
            {
                return await SendAsync(runtime, userPrompt, cancellationToken).ConfigureAwait(false);
            }
            catch (ApprovalReviewerTransientException exception) when (attempt < runtime.MaximumTransientRetries)
            {
                lastError = exception;
            }
        }

        throw lastError ?? new InvalidOperationException("The approval model did not return a decision.");
    }

    private async Task<CommandApprovalAiReview> SendAsync(
        ApprovalModelRuntimeConfiguration runtime,
        string userPrompt,
        CancellationToken cancellationToken)
    {
        var endpoint = new Uri(runtime.BaseUri, "chat/completions");
        using var request = new HttpRequestMessage(HttpMethod.Post, endpoint);
        request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", runtime.ApiKey);
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        request.Content = JsonContent.Create(new
        {
            model = runtime.Model,
            temperature = runtime.Temperature,
            max_tokens = runtime.MaxOutputTokens,
            messages = new object[]
            {
                new { role = "system", content = runtime.SystemPrompt },
                new { role = "user", content = userPrompt },
            },
            tools = new[]
            {
                new
                {
                    type = "function",
                    function = new
                    {
                        name = "approval_decision",
                        description = "Return the authoritative local command approval decision.",
                        parameters = new
                        {
                            type = "object",
                            additionalProperties = false,
                            required = new[] { "decision", "reason" },
                            properties = new
                            {
                                decision = new { type = "string", @enum = new[] { "approve", "deny", "ask_user" } },
                                reason = new { type = "string", minLength = 1, maxLength = 2000 },
                                remember_allow = new { type = "boolean" },
                            },
                        },
                    },
                },
            },
            tool_choice = new
            {
                type = "function",
                function = new { name = "approval_decision" },
            },
        }, options: JsonOptions);

        using var response = await httpClientFactory.CreateClient(HttpClientName)
            .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            _ = await ReadBoundedTextAsync(response, cancellationToken).ConfigureAwait(false);
            if (response.StatusCode is HttpStatusCode.RequestTimeout or HttpStatusCode.TooManyRequests ||
                (int)response.StatusCode >= 500)
            {
                throw new ApprovalReviewerTransientException(
                    $"Approval model returned HTTP {(int)response.StatusCode}.");
            }

            throw new InvalidOperationException(
                $"Approval model returned HTTP {(int)response.StatusCode}.");
        }

        var payload = await ReadBoundedTextAsync(response, cancellationToken).ConfigureAwait(false);
        return ParseDecision(payload);
    }

    private string BuildUserPrompt(CommandApprovalRequest request, ConnectorApprovalRisk risk)
    {
        var cwd = request.WorkingDirectory;
        var workspace = workspaces.Find(request.WorkspaceId);
        if (workspace is not null && Path.IsPathFullyQualified(cwd))
        {
            var relative = Path.GetRelativePath(workspace.AbsoluteRoot, cwd);
            cwd = relative == "." ? "." : relative.StartsWith("..", StringComparison.Ordinal)
                ? "<outside-workspace>"
                : relative;
        }

        return $"""
            Review this local command and call approval_decision exactly once.

            source: {SanitizeLine(request.Source)}
            cwd: {SanitizeLine(cwd)}
            command: {SanitizeLine(request.DisplayCommand)}
            static_risk_level: {risk.Level.ToString().ToLowerInvariant()}
            static_risk_reason: {SanitizeLine(risk.Reason ?? string.Empty)}
            """;
    }

    private static CommandApprovalAiReview ParseDecision(string payload)
    {
        try
        {
            using var document = JsonDocument.Parse(payload);
            var toolCalls = document.RootElement.GetProperty("choices")[0]
                .GetProperty("message").GetProperty("tool_calls");
            foreach (var toolCall in toolCalls.EnumerateArray())
            {
                var function = toolCall.GetProperty("function");
                if (!string.Equals(function.GetProperty("name").GetString(), "approval_decision", StringComparison.Ordinal))
                {
                    continue;
                }

                var argumentsValue = function.GetProperty("arguments");
                using var arguments = argumentsValue.ValueKind == JsonValueKind.String
                    ? JsonDocument.Parse(argumentsValue.GetString() ?? "{}")
                    : JsonDocument.Parse(argumentsValue.GetRawText());
                var root = arguments.RootElement;
                var decision = root.GetProperty("decision").GetString();
                var reason = root.GetProperty("reason").GetString()?.Trim();
                if (string.IsNullOrWhiteSpace(reason) || reason.Length > 2_000 || reason.Any(char.IsControl))
                {
                    throw new JsonException("Approval reason is invalid.");
                }

                var remember = root.TryGetProperty("remember_allow", out var rememberValue) &&
                    rememberValue.ValueKind is JsonValueKind.True;
                return decision switch
                {
                    "approve" => new CommandApprovalAiReview(CommandApprovalAiDecisionKind.Approve, reason, remember),
                    "deny" => new CommandApprovalAiReview(CommandApprovalAiDecisionKind.Deny, reason),
                    "ask_user" => new CommandApprovalAiReview(CommandApprovalAiDecisionKind.AskUser, reason),
                    _ => throw new JsonException("Approval decision is unsupported."),
                };
            }
        }
        catch (Exception exception) when (exception is
            JsonException or KeyNotFoundException or InvalidOperationException or IndexOutOfRangeException)
        {
            throw new InvalidOperationException("The approval model returned an invalid structured decision.", exception);
        }

        throw new InvalidOperationException("The approval model did not call approval_decision.");
    }

    private static async Task<string> ReadBoundedTextAsync(
        HttpResponseMessage response,
        CancellationToken cancellationToken)
    {
        if (response.Content.Headers.ContentLength is > MaximumResponseBytes)
        {
            throw new InvalidOperationException("Approval model response exceeded 256 KB.");
        }

        await using var stream = await response.Content.ReadAsStreamAsync(cancellationToken).ConfigureAwait(false);
        using var buffer = new MemoryStream();
        var bytes = new byte[16 * 1024];
        while (true)
        {
            var count = await stream.ReadAsync(bytes, cancellationToken).ConfigureAwait(false);
            if (count == 0) break;
            if (buffer.Length + count > MaximumResponseBytes)
            {
                throw new InvalidOperationException("Approval model response exceeded 256 KB.");
            }
            buffer.Write(bytes, 0, count);
        }
        return Encoding.UTF8.GetString(buffer.ToArray());
    }

    private static string SanitizeLine(string value) =>
        new(value.Where(character => !char.IsControl(character) || character == '\t').Take(8_000).ToArray());

    private sealed class ApprovalReviewerTransientException(string message) : Exception(message);
}
