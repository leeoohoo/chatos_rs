using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Conversation;

public sealed class ConversationHistoryService : IConversationHistoryService
{
    private readonly ChatOSApiClient _client;

    public ConversationHistoryService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<HistoryPage> FetchHistoryAsync(
        ConversationHistoryQuery query,
        CancellationToken cancellationToken = default)
    {
        var limit = Math.Max(1, query.Limit);
        var path = $"conversations/{Uri.EscapeDataString(query.ConversationId)}/compact-history?limit={limit}";
        if (!string.IsNullOrWhiteSpace(query.Before))
        {
            path += $"&before={Uri.EscapeDataString(query.Before)}";
        }

        var response = await _client.GetAsync<CompactHistoryResponseDto>(path, cancellationToken)
            .ConfigureAwait(false);
        return ConversationHistoryMapper.Map(response, query.ConversationId, query.RequestGeneration);
    }
}

internal sealed record CompactHistoryResponseDto
{
    [JsonPropertyName("items")]
    public IReadOnlyList<SessionMessageDto>? Items { get; init; }

    [JsonPropertyName("has_more")]
    public bool HasMore { get; init; }

    [JsonPropertyName("next_before")]
    public string? NextBefore { get; init; }

    [JsonPropertyName("snapshot_revision")]
    public long? SnapshotRevision { get; init; }
}

internal sealed record SessionMessageDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("conversation_id")]
    public string? ConversationId { get; init; }

    [JsonPropertyName("turn_id")]
    public string? TurnId { get; init; }

    [JsonPropertyName("sequence_no")]
    public long? SequenceNumber { get; init; }

    [JsonPropertyName("revision")]
    public long? ProtocolRevision { get; init; }

    [JsonPropertyName("role")]
    public required string Role { get; init; }

    [JsonPropertyName("content")]
    public string? Content { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }

    [JsonPropertyName("metadata")]
    public JsonElement Metadata { get; init; }

    [JsonPropertyName("message_mode")]
    public string? MessageMode { get; init; }

    [JsonPropertyName("message_source")]
    public string? MessageSource { get; init; }

    [JsonPropertyName("created_at")]
    public string? CreatedAt { get; init; }

    [JsonPropertyName("updated_at")]
    public string? UpdatedAt { get; init; }
}

internal static class ConversationHistoryMapper
{
    public static HistoryPage Map(
        CompactHistoryResponseDto response,
        string conversationId,
        long requestGeneration)
    {
        var messages = response.Items ?? Array.Empty<SessionMessageDto>();
        var lookup = new AssistantLookup(messages);
        var turns = messages
            .Where(static message => string.Equals(message.Role, "user", StringComparison.Ordinal))
            .Select((message, index) => MapTurn(
                message,
                index + 1,
                conversationId,
                lookup))
            .ToArray();
        return new HistoryPage(
            turns,
            response.HasMore ? response.NextBefore : null,
            response.HasMore,
            response.SnapshotRevision ?? turns.Select(static turn => turn.Revision).DefaultIfEmpty().Max(),
            requestGeneration);
    }

    private static ConversationTurn MapTurn(
        SessionMessageDto user,
        long fallbackSequence,
        string conversationId,
        AssistantLookup lookup)
    {
        var turnId = user.ResolvedTurnId();
        var assistant = lookup.FinalAssistant(user, turnId);
        var replies = lookup.Replies(user, turnId);
        var startedAt = ParseDate(user.CreatedAt) ?? DateTimeOffset.MinValue;
        var completedAt = replies.Count == 0
            ? null
            : ParseDate(replies[^1].UpdatedAt ?? replies[^1].CreatedAt);
        var revision = replies
            .Select(static reply => reply.ResolvedRevision())
            .Append(user.ResolvedRevision())
            .Max();
        var processCount = user.Metadata.Int32("historyProcess", "processMessageCount") ?? 0;
        var status = ResolveTurnStatus(user, assistant);
        var taskLookup = MergeTaskLookup(
            user.MessageTaskLookup(conversationId),
            assistant?.MessageTaskLookup(conversationId),
            conversationId);
        var projectExecutionContext = user.ProjectExecutionContext()
            ?? assistant?.ProjectExecutionContext();
        return new ConversationTurn(
            turnId,
            conversationId,
            user.SequenceNumber ?? fallbackSequence,
            revision,
            user.ToDomainMessage(ChatMessageRole.User, startedAt),
            processCount <= 0
                ? Array.Empty<TurnProcessEvent>()
                : new[]
                {
                    new TurnProcessEvent(
                        $"history-process-{turnId}",
                        $"包含 {processCount} 条过程记录",
                        "查看这一轮对话的推理、工具调用和中间结果。",
                        status),
                },
            assistant?.ToDomainMessage(ChatMessageRole.Assistant, completedAt ?? startedAt),
            replies.Select(reply => new ConversationAssistantReply(
                reply.ToDomainMessage(ChatMessageRole.Assistant, completedAt ?? startedAt),
                reply.TaskCallback())).ToArray(),
            taskLookup,
            true,
            status,
            startedAt,
            completedAt,
            projectExecutionContext);
    }

    private static MessageTaskLookup? MergeTaskLookup(
        MessageTaskLookup? primary,
        MessageTaskLookup? secondary,
        string conversationId)
    {
        if (primary is null && secondary is null)
        {
            return null;
        }

        return new MessageTaskLookup(
            conversationId,
            primary?.TurnId ?? secondary?.TurnId,
            primary?.SourceUserMessageId ?? secondary?.SourceUserMessageId);
    }

    private static TurnStatus ResolveTurnStatus(
        SessionMessageDto user,
        SessionMessageDto? assistant)
    {
        var taskStatus = user.Metadata.String("task_runner_async", "overall_status")
            ?? user.Metadata.String("task_runner_async", "confirmation_status");
        var status = (assistant?.Status ?? user.Status ?? taskStatus ?? string.Empty).ToLowerInvariant();
        return status switch
        {
            "failed" or "error" => TurnStatus.Failed,
            "cancelled" or "canceled" => TurnStatus.Cancelled,
            "completed" or "succeeded" or "success" => TurnStatus.Completed,
            _ => assistant is null ? TurnStatus.Streaming : TurnStatus.Completed,
        };
    }

    private static DateTimeOffset? ParseDate(string? value) =>
        DateTimeOffset.TryParse(value, out var date) ? date : null;

    private sealed class AssistantLookup
    {
        private readonly Dictionary<string, IndexedMessage> _byId = new(StringComparer.Ordinal);
        private readonly Dictionary<string, IndexedMessage> _finalsByUserMessageId = new(StringComparer.Ordinal);
        private readonly Dictionary<string, IndexedMessage> _finalsByTurnId = new(StringComparer.Ordinal);
        private readonly Dictionary<string, List<IndexedMessage>> _callbacksByUserMessageId = new(StringComparer.Ordinal);
        private readonly Dictionary<string, List<IndexedMessage>> _callbacksByTurnId = new(StringComparer.Ordinal);

        public AssistantLookup(IReadOnlyList<SessionMessageDto> messages)
        {
            for (var index = 0; index < messages.Count; index++)
            {
                var message = messages[index];
                if (!string.Equals(message.Role, "assistant", StringComparison.Ordinal) ||
                    message.IsCancelledTaskCallback())
                {
                    continue;
                }

                var indexed = new IndexedMessage(index, message);
                _byId[message.Id] = indexed;
                if (message.IsTaskCallback())
                {
                    var callback = message.TaskCallback();
                    Add(_callbacksByUserMessageId, callback?.SourceUserMessageId, indexed);
                    Add(_callbacksByTurnId, callback?.SourceTurnId, indexed);
                    continue;
                }

                AddOne(_finalsByUserMessageId, message.Metadata.String("historyFinalForUserMessageId"), indexed);
                AddOne(_finalsByTurnId, message.FinalTurnId(), indexed);
            }
        }

        public SessionMessageDto? FinalAssistant(SessionMessageDto user, string turnId)
        {
            var explicitId = user.Metadata.String("historyProcess", "finalAssistantMessageId");
            if (explicitId is not null &&
                _byId.TryGetValue(explicitId, out var explicitMessage) &&
                !explicitMessage.Message.IsTaskCallback())
            {
                return explicitMessage.Message;
            }

            if (_finalsByUserMessageId.TryGetValue(user.Id, out var byUserMessage))
            {
                return byUserMessage.Message;
            }

            return _finalsByTurnId.TryGetValue(turnId, out var byTurn)
                ? byTurn.Message
                : null;
        }

        public IReadOnlyList<SessionMessageDto> Replies(SessionMessageDto user, string turnId)
        {
            var values = new List<IndexedMessage>();
            var final = FinalAssistant(user, turnId);
            if (final is not null && _byId.TryGetValue(final.Id, out var indexedFinal))
            {
                values.Add(indexedFinal);
            }

            values.AddRange(_callbacksByUserMessageId.GetValueOrDefault(user.Id) ?? []);
            values.AddRange(_callbacksByTurnId.GetValueOrDefault(turnId) ?? []);
            var seen = new HashSet<string>(StringComparer.Ordinal);
            return values
                .OrderBy(static value => value.Index)
                .Where(value => seen.Add(value.Message.Id))
                .Select(static value => value.Message)
                .ToArray();
        }

        private static void Add(
            IDictionary<string, List<IndexedMessage>> dictionary,
            string? key,
            IndexedMessage value)
        {
            if (string.IsNullOrWhiteSpace(key))
            {
                return;
            }

            if (!dictionary.TryGetValue(key, out var list))
            {
                list = [];
                dictionary[key] = list;
            }

            list.Add(value);
        }

        private static void AddOne(
            IDictionary<string, IndexedMessage> dictionary,
            string? key,
            IndexedMessage value)
        {
            if (!string.IsNullOrWhiteSpace(key))
            {
                dictionary[key] = value;
            }
        }

        private readonly record struct IndexedMessage(int Index, SessionMessageDto Message);
    }
}

internal static class SessionMessageMappingExtensions
{
    public static string ResolvedTurnId(this SessionMessageDto message) =>
        message.TurnId.TrimmedOrNull()
        ?? message.Metadata.String("historyProcess", "turnId")
        ?? message.Metadata.String("conversation_turn_id")
        ?? message.Metadata.String("task_runner_async", "source_turn_id")
        ?? message.Id;

    public static string? FinalTurnId(this SessionMessageDto message) =>
        message.Metadata.String("historyFinalForTurnId")
        ?? message.Metadata.String("conversation_turn_id")
        ?? message.Metadata.String("task_runner_async", "source_turn_id")
        ?? message.TurnId.TrimmedOrNull();

    public static long ResolvedRevision(this SessionMessageDto message)
    {
        if (message.ProtocolRevision is > 0)
        {
            return message.ProtocolRevision.Value;
        }

        var date = DateTimeOffset.TryParse(message.UpdatedAt ?? message.CreatedAt, out var parsed)
            ? parsed
            : DateTimeOffset.MinValue;
        return Math.Max(1, date.ToUnixTimeMilliseconds());
    }

    public static ChatMessage ToDomainMessage(
        this SessionMessageDto message,
        ChatMessageRole role,
        DateTimeOffset fallbackDate) => new(
            message.Id,
            role,
            message.Content ?? string.Empty,
            DateTimeOffset.TryParse(message.CreatedAt, out var createdAt) ? createdAt : fallbackDate,
            message.Attachments());

    public static IReadOnlyList<ConversationAttachmentReference> Attachments(
        this SessionMessageDto message)
    {
        var values = message.Metadata.Array("attachments");
        return values.Select((value, index) => new ConversationAttachmentReference(
                value.String("id") ?? $"{message.Id}-attachment-{index}",
                value.String("name") ?? $"附件 {index + 1}",
                value.String("mimeType") ?? value.String("mime") ?? "application/octet-stream",
                value.Int32("size") ?? 0,
                (value.String("type") ?? "file").ToAttachmentKind(),
                value.String("storageProvider") ?? value.String("storage_provider"),
                value.String("bucket"),
                value.String("objectKey") ?? value.String("object_key"),
                value.String("url"),
                value.String("viewUrl") ?? value.String("view_url")))
            .ToArray();
    }

    public static bool IsTaskCallback(this SessionMessageDto message)
    {
        var kind = message.Metadata.String("task_runner_async", "message_kind")?.ToLowerInvariant();
        return string.Equals(message.MessageMode?.Trim(), "task_runner_callback", StringComparison.Ordinal) ||
               kind is "task_terminal_update" or "task_lifecycle_update";
    }

    public static bool IsCancelledTaskCallback(this SessionMessageDto message)
    {
        if (!message.IsTaskCallback())
        {
            return false;
        }

        var eventName = message.Metadata.String("task_runner_async", "event")?.ToLowerInvariant();
        var status = message.Metadata.String("task_runner_async", "status")?.ToLowerInvariant();
        return eventName is "task.cancelled" or "task.canceled" ||
               status is "cancelled" or "canceled";
    }

    public static TaskRunnerCallbackReference? TaskCallback(this SessionMessageDto message)
    {
        if (!message.IsTaskCallback() ||
            message.Metadata.String("task_runner_async", "task_id") is not { } taskId)
        {
            return null;
        }

        var eventName = message.Metadata.String("task_runner_async", "event");
        var status = message.Metadata.String("task_runner_async", "status");
        return new TaskRunnerCallbackReference(
            taskId,
            message.Metadata.String("task_runner_async", "run_id"),
            eventName,
            NormalizeCallbackStatus(eventName, status),
            message.Metadata.String("task_runner_async", "source_session_id"),
            message.Metadata.String("task_runner_async", "source_turn_id"),
            message.Metadata.String("task_runner_async", "source_user_message_id"));
    }

    public static MessageTaskLookup? MessageTaskLookup(
        this SessionMessageDto message,
        string conversationId)
    {
        var turnId = message.Metadata.String("conversation_turn_id")
            ?? message.Metadata.String("task_runner_async", "source_turn_id");
        var sourceMessageId = message.Metadata.String("task_runner_async", "source_user_message_id");
        if (sourceMessageId?.StartsWith("temp_", StringComparison.Ordinal) == true)
        {
            sourceMessageId = null;
        }
        return turnId is null && sourceMessageId is null
            ? null
            : new MessageTaskLookup(conversationId, turnId, sourceMessageId);
    }

    public static ProjectExecutionContext? ProjectExecutionContext(this SessionMessageDto message)
    {
        var hasExecutionObject = message.Metadata.HasObject("project_requirement_execution");
        var mode = message.Metadata.String("task_runner_async", "mode");
        var executionKind = message.Metadata.String("task_runner_async", "execution_kind");
        if (!hasExecutionObject &&
            !string.Equals(mode, "project_requirement_execution", StringComparison.OrdinalIgnoreCase) &&
            !string.Equals(executionKind, "project_requirement_execution", StringComparison.OrdinalIgnoreCase))
        {
            return null;
        }

        return new ProjectExecutionContext(
            message.Metadata.String("project_requirement_execution", "project_id")
                ?? message.Metadata.String("task_runner_async", "project_id"),
            message.Metadata.String("project_requirement_execution", "requirement_id")
                ?? message.Metadata.String("task_runner_async", "requirement_id"),
            message.Metadata.String("project_requirement_execution", "execution_group_id")
                ?? message.Metadata.String("task_runner_async", "execution_group_id"),
            message.Metadata.String("project_requirement_execution", "replaced_execution_group_id")
                ?? message.Metadata.String("task_runner_async", "replaced_execution_group_id"),
            message.Metadata.String("project_requirement_execution", "contact_id")
                ?? message.Metadata.String("task_runner_async", "contact_id"),
            mode,
            executionKind,
            message.Metadata.String("task_runner_async", "confirmation_status"),
            message.Metadata.String("task_runner_async", "overall_status")
                ?? message.Metadata.String("task_runner_async", "status"));
    }

    private static string? NormalizeCallbackStatus(string? eventName, string? status)
    {
        var normalizedStatus = status?.ToLowerInvariant();
        if (normalizedStatus is "completed" or "succeeded" or "success" or "done") return "completed";
        if (normalizedStatus is "failed" or "error") return "failed";
        if (normalizedStatus == "blocked") return "blocked";
        if (normalizedStatus is "cancelled" or "canceled" or "stopped") return "cancelled";
        return eventName?.ToLowerInvariant() switch
        {
            "task.completed" => "completed",
            "task.failed" => "failed",
            "task.blocked" => "blocked",
            "task.cancelled" or "task.canceled" => "cancelled",
            "task.run.started" or "task.started" => normalizedStatus ?? "running",
            _ => normalizedStatus,
        };
    }
}

internal static class ConversationMetadataExtensions
{
    public static string? String(this JsonElement value, params string[] path)
    {
        if (!value.TryPath(path, out var child) || child.ValueKind != JsonValueKind.String)
        {
            return null;
        }

        return child.GetString().TrimmedOrNull();
    }

    public static int? Int32(this JsonElement value, params string[] path) =>
        value.TryPath(path, out var child) && child.TryGetInt32(out var number) ? number : null;

    public static IReadOnlyList<JsonElement> Array(this JsonElement value, params string[] path) =>
        value.TryPath(path, out var child) && child.ValueKind == JsonValueKind.Array
            ? child.EnumerateArray().Where(static item => item.ValueKind == JsonValueKind.Object).ToArray()
            : System.Array.Empty<JsonElement>();

    public static bool HasObject(this JsonElement value, params string[] path) =>
        value.TryPath(path, out var child) && child.ValueKind == JsonValueKind.Object;

    private static bool TryPath(
        this JsonElement value,
        IReadOnlyList<string> path,
        out JsonElement result)
    {
        result = value;
        foreach (var component in path)
        {
            if (result.ValueKind != JsonValueKind.Object ||
                !result.TryGetProperty(component, out result))
            {
                return false;
            }
        }

        return true;
    }
}
