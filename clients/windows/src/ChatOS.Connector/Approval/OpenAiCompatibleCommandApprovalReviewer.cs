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
    IHttpClientFactory httpClientFactory,
    IApprovalMemoryEngineRecorder memoryRecorder) : ICommandApprovalAiReviewer
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
        var memoryRun = await memoryRecorder.BeginAsync(
            runtime,
            request,
            runtime.SystemPrompt,
            userPrompt,
            cancellationToken).ConfigureAwait(false);
        Exception? lastError = null;
        for (var attempt = 0; attempt <= runtime.MaximumTransientRetries; attempt++)
        {
            try
            {
                return await SendAsync(runtime, request, userPrompt, memoryRun, cancellationToken)
                    .ConfigureAwait(false);
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
        CommandApprovalRequest approvalRequest,
        string userPrompt,
        ApprovalMemoryRun memoryRun,
        CancellationToken cancellationToken)
    {
        var endpoint = new Uri(runtime.BaseUri, "responses");
        using var request = new HttpRequestMessage(HttpMethod.Post, endpoint);
        request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", runtime.ApiKey);
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        var input = new List<object>
        {
            new { role = "system", content = runtime.SystemPrompt },
        };
        input.AddRange(memoryRun.ContextBlocks.Select(block =>
            (object)new { role = "system", content = block }));
        input.Add(new { role = "user", content = userPrompt });
        var requestPayload = new Dictionary<string, object?>
        {
            ["model"] = runtime.Model,
            ["temperature"] = runtime.Temperature,
            ["max_output_tokens"] = runtime.MaxOutputTokens,
            ["input"] = input,
            ["tools"] = new[]
            {
                new
                {
                    type = "function",
                    name = "approval_decision",
                    description = "Return the authoritative local command approval decision.",
                    parameters = ApprovalDecisionParameters(),
                },
            },
            ["tool_choice"] = new { type = "function", name = "approval_decision" },
            ["store"] = false,
            ["prompt_cache_key"] = $"approval-agent:{runtime.ModelConfigId}",
            ["context_management"] = new[]
            {
                new { type = "compaction", compact_threshold = 200_000 },
            },
        };
        var thinking = runtime.ThinkingLevel?.Trim().ToLowerInvariant();
        if (!string.IsNullOrEmpty(thinking) && thinking is not ("none" or "auto"))
        {
            requestPayload["reasoning"] = new { effort = thinking };
            requestPayload["include"] = new[] { "reasoning.encrypted_content" };
        }
        request.Content = JsonContent.Create(requestPayload, options: JsonOptions);

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
        var decision = ParseDecision(payload);
        await memoryRecorder.CompleteAsync(
            memoryRun,
            approvalRequest,
            payload,
            decision,
            cancellationToken).ConfigureAwait(false);
        return decision;
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

    private static object ApprovalDecisionParameters() => new
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
    };

    private static CommandApprovalAiReview ParseDecision(string payload)
    {
        try
        {
            using var document = JsonDocument.Parse(payload);
            if (!document.RootElement.TryGetProperty("status", out var status) ||
                !string.Equals(status.GetString(), "completed", StringComparison.Ordinal))
            {
                throw new JsonException("Approval response did not complete.");
            }
            var toolCalls = document.RootElement.GetProperty("output");
            foreach (var toolCall in toolCalls.EnumerateArray())
            {
                var function = toolCall;
                if (!toolCall.TryGetProperty("type", out var type) ||
                    !string.Equals(type.GetString(), "function_call", StringComparison.Ordinal))
                {
                    continue;
                }
                if (!string.Equals(function.GetProperty("name").GetString(),
                    "approval_decision", StringComparison.Ordinal))
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
