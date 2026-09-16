using System.Net.Http.Headers;
using System.Net.Http.Json;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Approval;

internal sealed record ApprovalMemoryRun(
    string TenantId,
    string ThreadId,
    string SubjectId,
    Uri GatewayBaseUri,
    string AccessToken,
    DateTimeOffset StartedAt,
    IReadOnlyList<string> ContextBlocks);

internal interface IApprovalMemoryEngineRecorder
{
    Task<ApprovalMemoryRun> BeginAsync(
        ApprovalModelRuntimeConfiguration runtime,
        CommandApprovalRequest request,
        string systemPrompt,
        string userPrompt,
        CancellationToken cancellationToken);

    Task CompleteAsync(
        ApprovalMemoryRun run,
        CommandApprovalRequest request,
        string responsePayload,
        CommandApprovalAiReview decision,
        CancellationToken cancellationToken);
}

internal sealed class ApprovalMemoryEngineRecorder(IHttpClientFactory httpClientFactory)
    : IApprovalMemoryEngineRecorder
{
    internal const string HttpClientName = "ChatOS.WindowsApprovalMemoryEngine";
    private const int MaximumResponseBytes = 8 * 1024 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web);

    public async Task<ApprovalMemoryRun> BeginAsync(
        ApprovalModelRuntimeConfiguration runtime,
        CommandApprovalRequest request,
        string systemPrompt,
        string userPrompt,
        CancellationToken cancellationToken)
    {
        if (!string.Equals(runtime.OwnerUserId, request.OwnerUserId, StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Approval Memory Engine owner mismatch.");
        }

        var runKey = Guid.NewGuid().ToString("N");
        var workspaceKey = Digest($"{request.OwnerUserId}\0{request.WorkspaceId}");
        var threadId = $"client-agent:approval:{workspaceKey}:{runKey}";
        var subjectId = $"client-agent:approval-run:{runKey}";
        var run = new ApprovalMemoryRun(
            request.OwnerUserId,
            threadId,
            subjectId,
            runtime.GatewayBaseUri,
            runtime.ConnectorAccessToken,
            DateTimeOffset.UtcNow,
            []);

        var createdThread = await SendAsync(
            run,
            HttpMethod.Put,
            $"/api/memory/threads/{Uri.EscapeDataString(threadId)}",
            new
            {
                tenant_id = run.TenantId,
                source_id = "chatos",
                subject_id = run.SubjectId,
                thread_type = "client_agent",
                external_thread_id = request.RequestId,
                labels = new[] { "client_agent", "approval", "memory_mapping:client_agent.v1" },
            },
            cancellationToken).ConfigureAwait(false);
        if (!Matches(createdThread, "id", run.ThreadId) ||
            !Matches(createdThread, "tenant_id", run.TenantId) ||
            !Matches(createdThread, "source_id", "chatos") ||
            !Matches(createdThread, "subject_id", run.SubjectId))
        {
            throw new InvalidOperationException("Approval Memory Engine thread response is invalid.");
        }

        await SyncAsync(
            run,
            [
                MessageRecord(run, 0, "system", systemPrompt),
                MessageRecord(run, 1, "user", userPrompt),
            ],
            cancellationToken).ConfigureAwait(false);
        var contextBlocks = await ComposeAsync(run, cancellationToken).ConfigureAwait(false);
        return run with { ContextBlocks = contextBlocks };
    }

    public async Task CompleteAsync(
        ApprovalMemoryRun run,
        CommandApprovalRequest request,
        string responsePayload,
        CommandApprovalAiReview decision,
        CancellationToken cancellationToken)
    {
        using var document = JsonDocument.Parse(responsePayload);
        var root = document.RootElement;
        var call = root.GetProperty("output")
            .EnumerateArray()
            .First(item => item.TryGetProperty("type", out var type) &&
                string.Equals(type.GetString(), "function_call", StringComparison.Ordinal) &&
                item.TryGetProperty("name", out var name) &&
                string.Equals(name.GetString(), "approval_decision", StringComparison.Ordinal));
        var callId = call.TryGetProperty("call_id", out var callIdValue)
            ? callIdValue.GetString()
            : call.GetProperty("id").GetString();
        if (string.IsNullOrWhiteSpace(callId))
        {
            throw new InvalidOperationException("Approval response has no tool call id for Memory Engine.");
        }
        var arguments = call.GetProperty("arguments");
        using var argumentDocument = arguments.ValueKind == JsonValueKind.String
            ? JsonDocument.Parse(arguments.GetString() ?? "{}")
            : JsonDocument.Parse(arguments.GetRawText());
        var argumentObject = argumentDocument.RootElement.Clone();
        var argumentText = arguments.ValueKind == JsonValueKind.String
            ? arguments.GetString() ?? "{}"
            : arguments.GetRawText();
        var responsesOutput = root.GetProperty("output").Clone();
        var usage = root.TryGetProperty("usage", out var usageValue)
            ? usageValue.Clone()
            : default(JsonElement?);
        var responseId = root.TryGetProperty("id", out var responseIdValue)
            ? responseIdValue.GetString()
            : null;

        var assistant = new
        {
            id = RecordId(run, 2),
            role = "assistant",
            record_type = "message",
            content = "",
            structured_payload = new
            {
                tool_calls = new[]
                {
                    new
                    {
                        id = callId,
                        type = "function",
                        function = new
                        {
                            name = "approval_decision",
                            arguments = argumentText,
                        },
                    },
                },
            },
            metadata = new
            {
                response_id = responseId,
                response_status = "tool_calls",
                responses_output = responsesOutput,
                provider_usage = usage,
                message_mode = "approval_agent",
                message_source = "windows_connector",
            },
            created_at = run.StartedAt.AddMilliseconds(2).ToString("O"),
        };
        var tool = new
        {
            id = RecordId(run, 3),
            role = "tool",
            record_type = "message",
            content = argumentObject.GetRawText(),
            structured_payload = new
            {
                tool_call_id = callId,
                result = new
                {
                    approved = decision.Decision == CommandApprovalAiDecisionKind.Approve,
                    decision = DecisionName(decision.Decision),
                    decision.Reason,
                    decision.RememberForSession,
                },
            },
            metadata = new
            {
                tool_call_id = callId,
                toolName = "approval_decision",
                success = true,
                isError = false,
                isStream = false,
                message_mode = "approval_agent",
                message_source = "windows_connector",
                request_id = request.RequestId,
            },
            created_at = run.StartedAt.AddMilliseconds(3).ToString("O"),
        };
        await SyncAsync(run, [assistant, tool], cancellationToken).ConfigureAwait(false);
    }

    private async Task<IReadOnlyList<string>> ComposeAsync(
        ApprovalMemoryRun run,
        CancellationToken cancellationToken)
    {
        var endpoint = Endpoint(run.GatewayBaseUri, "/api/memory/context/compose");
        using var request = new HttpRequestMessage(HttpMethod.Post, endpoint);
        request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", run.AccessToken);
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        request.Content = JsonContent.Create(new
        {
            tenant_id = run.TenantId,
            source_id = "chatos",
            thread_id = run.ThreadId,
            subject_id = run.SubjectId,
            policy = new
            {
                include_thread_summary = true,
                include_recent_records = true,
                include_subject_memory = false,
                summary_limit = 2,
            },
        }, options: JsonOptions);
        using var response = await httpClientFactory.CreateClient(HttpClientName)
            .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            throw new InvalidOperationException(
                $"Approval Memory Engine compose returned HTTP {(int)response.StatusCode}.");
        }
        var root = await ReadJsonAsync(response, cancellationToken).ConfigureAwait(false);
        if (!root.TryGetProperty("thread_id", out var threadId) ||
            !string.Equals(threadId.GetString(), run.ThreadId, StringComparison.Ordinal) ||
            !root.TryGetProperty("blocks", out var blocks) ||
            blocks.ValueKind != JsonValueKind.Array)
        {
            throw new InvalidOperationException("Approval Memory Engine compose response is invalid.");
        }
        return blocks.EnumerateArray()
            .Select(block => block.TryGetProperty("text", out var text) ? text.GetString() : null)
            .Where(text => !string.IsNullOrWhiteSpace(text))
            .Select(text => text!)
            .ToArray();
    }

    private async Task SyncAsync(
        ApprovalMemoryRun run,
        IReadOnlyList<object> records,
        CancellationToken cancellationToken)
    {
        var result = await SendAsync(
            run,
            HttpMethod.Put,
            $"/api/memory/threads/{Uri.EscapeDataString(run.ThreadId)}/records/batch-sync",
            new
            {
                tenant_id = run.TenantId,
                source_id = "chatos",
                records,
            },
            cancellationToken).ConfigureAwait(false);
        if (!Matches(result, "thread_id", run.ThreadId) ||
            !Matches(result, "received_count", records.Count) ||
            !Matches(result, "upserted_count", records.Count))
        {
            throw new InvalidOperationException("Approval Memory Engine sync response is invalid.");
        }
    }

    private static object MessageRecord(ApprovalMemoryRun run, int index, string role, string content) => new
    {
        id = RecordId(run, index),
        role,
        record_type = "message",
        content,
        structured_payload = (object?)null,
        metadata = new
        {
            message_mode = "approval_agent",
            message_source = "windows_connector",
            client_agent_message_index = index,
        },
        created_at = run.StartedAt.AddMilliseconds(index).ToString("O"),
    };

    private async Task<JsonElement> SendAsync(
        ApprovalMemoryRun run,
        HttpMethod method,
        string path,
        object body,
        CancellationToken cancellationToken)
    {
        var endpoint = Endpoint(run.GatewayBaseUri, path);
        using var request = new HttpRequestMessage(method, endpoint);
        request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", run.AccessToken);
        request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));
        request.Content = JsonContent.Create(body, options: JsonOptions);
        using var response = await httpClientFactory.CreateClient(HttpClientName)
            .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
            .ConfigureAwait(false);
        if (!response.IsSuccessStatusCode)
        {
            throw new InvalidOperationException(
                $"Approval Memory Engine returned HTTP {(int)response.StatusCode}.");
        }
        return await ReadJsonAsync(response, cancellationToken).ConfigureAwait(false);
    }

    private static async Task<JsonElement> ReadJsonAsync(
        HttpResponseMessage response,
        CancellationToken cancellationToken)
    {
        if (response.Content.Headers.ContentLength is > MaximumResponseBytes)
        {
            throw new InvalidOperationException("Approval Memory Engine response exceeded 8 MB.");
        }

        await using var stream = await response.Content.ReadAsStreamAsync(cancellationToken)
            .ConfigureAwait(false);
        using var buffer = new MemoryStream();
        var bytes = new byte[16 * 1024];
        while (true)
        {
            var count = await stream.ReadAsync(bytes, cancellationToken).ConfigureAwait(false);
            if (count == 0)
            {
                break;
            }
            if (buffer.Length + count > MaximumResponseBytes)
            {
                throw new InvalidOperationException("Approval Memory Engine response exceeded 8 MB.");
            }
            buffer.Write(bytes, 0, count);
        }
        using var document = JsonDocument.Parse(buffer.ToArray());
        return document.RootElement.Clone();
    }

    private static string RecordId(ApprovalMemoryRun run, int index) =>
        $"{run.ThreadId}:message:{index}";

    private static Uri Endpoint(Uri baseUri, string path)
    {
        if (!baseUri.IsAbsoluteUri || baseUri.Scheme is not ("http" or "https") ||
            !path.StartsWith("/", StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Approval Memory Engine endpoint is invalid.");
        }
        return new Uri(baseUri.AbsoluteUri.TrimEnd('/') + path, UriKind.Absolute);
    }

    private static bool Matches(JsonElement root, string property, string expected) =>
        root.TryGetProperty(property, out var value) &&
        string.Equals(value.GetString(), expected, StringComparison.Ordinal);

    private static bool Matches(JsonElement root, string property, int expected) =>
        root.TryGetProperty(property, out var value) &&
        value.TryGetInt32(out var actual) && actual == expected;

    private static string DecisionName(CommandApprovalAiDecisionKind decision) => decision switch
    {
        CommandApprovalAiDecisionKind.Approve => "approve",
        CommandApprovalAiDecisionKind.Deny => "deny",
        _ => "ask_user",
    };

    private static string Digest(string value) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value)))
            .ToLowerInvariant()[..24];
}
