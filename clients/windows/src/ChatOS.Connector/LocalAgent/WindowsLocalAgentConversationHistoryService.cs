using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentConversationHistoryService : IConversationHistoryService
{
    private readonly WindowsLocalAgentConversationClient _client;
    private readonly WindowsLocalAgentAttachmentVault _attachments;
    private readonly object _contextGate = new();
    private string? _ownerUserId;

    public WindowsLocalAgentConversationHistoryService(
        WindowsLocalAgentConversationClient client,
        WindowsLocalAgentAttachmentVault attachments)
    {
        _client = client;
        _attachments = attachments;
    }

    public void Configure(string ownerUserId) => SetOwner(ownerUserId);

    public void Reset() => SetOwner(null);

    public async Task<HistoryPage> FetchHistoryAsync(
        ConversationHistoryQuery query,
        CancellationToken cancellationToken = default)
    {
        var ownerUserId = RequireOwner();
        ulong? before = null;
        if (!string.IsNullOrWhiteSpace(query.Before))
        {
            if (!ulong.TryParse(query.Before, out var parsed) || parsed == 0)
            {
                throw new InvalidOperationException("Local Agent history cursor is invalid.");
            }
            before = parsed;
        }
        var limit = (uint)Math.Clamp(query.Limit, 1, 100);
        WindowsLocalConversationHistoryPage page;
        try
        {
            page = await _client.HistoryAsync(
                ownerUserId,
                query.ConversationId,
                before,
                limit,
                cancellationToken).ConfigureAwait(false);
        }
        catch (LocalAgentHostRequestException exception) when (exception.Code == "not_found")
        {
            try
            {
                _ = await _client.CreateAsync(
                    ownerUserId,
                    query.ConversationId,
                    cancellationToken).ConfigureAwait(false);
            }
            catch (LocalAgentHostRequestException conflict) when (conflict.Code == "conflict")
            {
            }
            page = await _client.HistoryAsync(
                ownerUserId,
                query.ConversationId,
                before,
                limit,
                cancellationToken).ConfigureAwait(false);
        }
        return Map(page, query.RequestGeneration, ownerUserId);
    }

    private HistoryPage Map(
        WindowsLocalConversationHistoryPage page,
        long requestGeneration,
        string ownerUserId)
    {
        var messagesByTurn = page.Messages
            .GroupBy(message => message.TurnId)
            .ToDictionary(group => group.Key, group => group.OrderBy(value => value.Ordinal).ToArray());
        var attachmentsByMessage = page.Attachments
            .GroupBy(value => value.MessageId)
            .ToDictionary(group => group.Key, group => group.ToArray());
        var turns = page.Turns.Select(turn =>
        {
            var messages = messagesByTurn.GetValueOrDefault(turn.TurnId) ?? [];
            var user = messages.FirstOrDefault(message => message.Role == "user");
            var additionalUsers = messages.Where(message => message.Role == "user").Skip(1)
                .Select(message => MapMessage(
                    message,
                    message.MessageId,
                    attachmentsByMessage.GetValueOrDefault(message.MessageId) ?? [],
                    ownerUserId,
                    turn.ConversationId))
                .ToArray();
            var assistants = CoalesceTaskGraphTerminalMessages(
                messages.Where(message => message.Role == "assistant"));
            var userMessage = MapMessage(
                user,
                turn.UserMessageId,
                attachmentsByMessage.GetValueOrDefault(turn.UserMessageId) ?? [],
                ownerUserId,
                turn.ConversationId);
            var projectedReplies = assistants.SelectMany(message =>
                ReplyProjections(message, turn.UserMessageId).Select(projection =>
                    new ConversationAssistantReply(
                        MapMessage(
                            message,
                            projection.MessageId,
                            attachmentsByMessage.GetValueOrDefault(message.MessageId) ?? [],
                            ownerUserId,
                            turn.ConversationId,
                            projection.Text),
                        projection.TaskCallback)))
                .ToArray();
            // Match macOS and the original Task Runner contract: the Main Chat handoff
            // is shown before asynchronous Task callbacks and graph deliveries even if
            // storage ordinals interleave while local scheduling starts immediately.
            var replies = projectedReplies
                .Where(static reply => !IsAsyncTaskReply(reply))
                .Concat(projectedReplies.Where(static reply => IsAsyncTaskReply(reply)))
                .ToArray();
            var finalReplies = replies.Where(static reply => reply.TaskCallback is null).ToArray();
            var status = MapStatus(turn.Status);
            return new ConversationTurn(
                turn.TurnId,
                turn.ConversationId,
                Clamp(user?.Ordinal ?? 0),
                Clamp(page.Conversation.Version),
                userMessage,
                [new TurnProcessEvent(
                    $"local-process:{turn.RunId}:{turn.UpdatedAtUnixMs}",
                    ProcessTitle(status),
                    null,
                    status)],
                finalReplies.LastOrDefault()?.Message,
                replies,
                new MessageTaskLookup(
                    turn.ConversationId,
                    turn.TurnId,
                    userMessage.Id),
                true,
                status,
                DateTimeOffset.FromUnixTimeMilliseconds(turn.CreatedAtUnixMs),
                status == TurnStatus.Streaming
                    ? null
                    : DateTimeOffset.FromUnixTimeMilliseconds(turn.UpdatedAtUnixMs))
            {
                AdditionalUserMessages = additionalUsers,
            };
        }).ToArray();
        return new HistoryPage(
            turns,
            page.NextBeforeOrdinal?.ToString(),
            page.NextBeforeOrdinal is not null,
            Clamp(page.Conversation.Version),
            requestGeneration);
    }

    private ChatMessage MapMessage(
        WindowsLocalConversationMessageRecord? message,
        string fallbackId,
        IReadOnlyList<WindowsLocalConversationAttachmentRecord> attachments,
        string ownerUserId,
        string conversationId,
        string? projectedText = null) => new(
            fallbackId,
            message?.Role == "assistant" ? ChatMessageRole.Assistant : ChatMessageRole.User,
            projectedText ?? (message is null ? string.Empty : Text(message.Content)),
            DateTimeOffset.FromUnixTimeMilliseconds(message?.CreatedAtUnixMs ?? 0),
            attachments.Select(value =>
            {
                var displayUri = _attachments.ResolveDisplayUri(value, ownerUserId, conversationId);
                return new ConversationAttachmentReference(
                    value.AttachmentId,
                    value.DisplayName,
                    value.MediaType,
                    value.ByteSize > int.MaxValue ? int.MaxValue : (int)value.ByteSize,
                    value.MediaType.StartsWith("image/", StringComparison.OrdinalIgnoreCase)
                        ? ConversationAttachmentKind.Image
                        : value.MediaType.StartsWith("audio/", StringComparison.OrdinalIgnoreCase)
                            ? ConversationAttachmentKind.Audio
                            : ConversationAttachmentKind.File,
                    "local-agent",
                    null,
                    null,
                    displayUri?.AbsoluteUri,
                    displayUri?.AbsoluteUri,
                    value.Sha256);
            }).ToArray())
        {
            StorageOrdinal = message?.Ordinal,
        };

    private static IReadOnlyList<ReplyProjection> ReplyProjections(
        WindowsLocalConversationMessageRecord message,
        string sourceUserMessageId)
    {
        if (TaskCallback(message, sourceUserMessageId) is { } callback)
        {
            return [new(message.MessageId, Text(message.Content), callback)];
        }

        if (message.Content.ValueKind != JsonValueKind.Object ||
            String(message.Content, "type") != "task_graph_terminal" ||
            !message.Content.TryGetProperty("tasks", out var tasks) ||
            tasks.ValueKind != JsonValueKind.Array)
        {
            return [new(message.MessageId, Text(message.Content), null)];
        }

        var aggregate = String(message.Content, "delivery_text") ??
            String(message.Content, "summary");
        if (aggregate is not null)
        {
            return [new(message.MessageId, aggregate, null)];
        }

        var projections = tasks.EnumerateArray()
            .Where(static task => task.ValueKind == JsonValueKind.Object)
            .Select(task =>
            {
                var taskId = String(task, "task_id");
                if (taskId is null) return null;
                var status = String(task, "status");
                var displayStatus = String(task, "display_status");
                var text = task.TryGetProperty("terminal_outcome", out var outcome)
                    ? Text(outcome).Trim()
                    : string.Empty;
                if (text.Length == 0) text = String(task, "title") ?? taskId;
                return new ReplyProjection(
                    $"{message.MessageId}::{taskId}",
                    text,
                    new TaskExecutionCallbackReference(
                        taskId,
                        null,
                        TerminalEvent(status),
                        CallbackStatus(displayStatus ?? status),
                        message.ConversationId,
                        message.TurnId,
                        sourceUserMessageId));
            })
            .Where(static projection => projection is not null)
            .Select(static projection => projection!)
            .ToArray();
        return projections.Length == 0
            ? [new(message.MessageId, Text(message.Content), null)]
            : projections;
    }

    private static TaskExecutionCallbackReference? TaskCallback(
        WindowsLocalConversationMessageRecord message,
        string sourceUserMessageId)
    {
        if (message.Metadata.ValueKind != JsonValueKind.Object ||
            !message.Metadata.TryGetProperty("task_runner_async", out var taskRunner) ||
            taskRunner.ValueKind != JsonValueKind.Object ||
            String(taskRunner, "task_id") is not { } taskId)
        {
            return null;
        }

        var eventName = String(taskRunner, "event");
        var status = String(taskRunner, "display_status") ?? String(taskRunner, "status");
        return new TaskExecutionCallbackReference(
            taskId,
            String(taskRunner, "run_id"),
            eventName,
            CallbackStatus(status, eventName),
            String(taskRunner, "source_session_id") ?? message.ConversationId,
            String(taskRunner, "source_turn_id") ?? message.TurnId,
            String(taskRunner, "source_user_message_id") ?? sourceUserMessageId);
    }

    private static string? CallbackStatus(string? status, string? eventName = null)
    {
        var normalized = status?.Trim().ToLowerInvariant();
        if (normalized is "execution_failed" or "acceptance_failed" or "evidence_incomplete")
        {
            return normalized;
        }
        if (normalized is "completed" or "succeeded" or "success" or "done") return "completed";
        if (normalized is "failed" or "error") return "failed";
        if (normalized == "blocked") return "blocked";
        if (normalized is "cancelled" or "canceled" or "stopped") return "cancelled";
        if (normalized is "running" or "processing" or "in_progress") return "running";
        return eventName?.Trim().ToLowerInvariant() switch
        {
            "task.completed" => "completed",
            "task.failed" => "failed",
            "task.blocked" => "blocked",
            "task.cancelled" or "task.canceled" => "cancelled",
            "task.run.started" or "task.started" => "running",
            _ => status,
        };
    }

    private static string? TerminalEvent(string? status) =>
        status?.Trim().ToLowerInvariant() switch
        {
            "completed" or "succeeded" or "success" or "done" => "task.completed",
            "failed" or "error" => "task.failed",
            "blocked" => "task.blocked",
            "cancelled" or "canceled" or "stopped" => "task.cancelled",
            _ => null,
        };

    /// Storage retains every terminal generation for audit and retry history. Main Chat only
    /// presents the newest generation for each Task Graph so a later cancellation or recovery
    /// does not repeat a previous terminal summary and its full delivery.
    private static WindowsLocalConversationMessageRecord[] CoalesceTaskGraphTerminalMessages(
        IEnumerable<WindowsLocalConversationMessageRecord> messages)
    {
        var materialized = messages.ToArray();
        var latestOrdinalByGraphId = new Dictionary<string, ulong>(StringComparer.Ordinal);
        foreach (var message in materialized)
        {
            if (TaskGraphTerminalGraphId(message) is not { } graphId) continue;
            if (!latestOrdinalByGraphId.TryGetValue(graphId, out var latest) ||
                message.Ordinal > latest)
            {
                latestOrdinalByGraphId[graphId] = message.Ordinal;
            }
        }

        return materialized.Where(message =>
        {
            var graphId = TaskGraphTerminalGraphId(message);
            return graphId is null || latestOrdinalByGraphId[graphId] == message.Ordinal;
        }).ToArray();
    }

    private static string? TaskGraphTerminalGraphId(
        WindowsLocalConversationMessageRecord message)
    {
        if (message.Content.ValueKind != JsonValueKind.Object ||
            String(message.Content, "type") != "task_graph_terminal")
        {
            return null;
        }

        return String(message.Metadata, "graph_id") ?? String(message.Content, "graph_id");
    }

    private static bool IsAsyncTaskReply(ConversationAssistantReply reply) =>
        reply.TaskCallback is not null ||
        reply.Message.Id.StartsWith("task-graph:", StringComparison.Ordinal);

    private static string? String(JsonElement value, string propertyName)
    {
        if (value.ValueKind != JsonValueKind.Object ||
            !value.TryGetProperty(propertyName, out var property) ||
            property.ValueKind != JsonValueKind.String)
        {
            return null;
        }
        var result = property.GetString()?.Trim();
        return string.IsNullOrEmpty(result) ? null : result;
    }

    private static string Text(JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.String) return value.GetString() ?? string.Empty;
        if (value.ValueKind == JsonValueKind.Array)
        {
            return string.Join('\n', value.EnumerateArray()
                .Select(Text)
                .Where(static text => text.Length > 0));
        }
        if (value.ValueKind == JsonValueKind.Object)
        {
            if (String(value, "type") == "task_graph_terminal")
            {
                if (String(value, "delivery_text") is { } delivery) return delivery;
                if (String(value, "summary") is { } summary) return summary;
                if (!value.TryGetProperty("tasks", out var tasks) ||
                    tasks.ValueKind != JsonValueKind.Array)
                {
                    return string.Empty;
                }
                var rendered = tasks.EnumerateArray()
                    .Where(static task => task.ValueKind == JsonValueKind.Object)
                    .Select(task =>
                    {
                        var title = String(task, "title") ?? string.Empty;
                        var body = task.TryGetProperty("terminal_outcome", out var outcome)
                            ? Text(outcome)
                            : string.Empty;
                        if (body.Length == 0)
                        {
                            body = string.Join(": ", new[] { title, String(task, "status") }
                                .Where(static part => !string.IsNullOrEmpty(part)));
                        }
                        return (title, body);
                    })
                    .ToArray();
                if (rendered.Length == 1) return rendered[0].body;
                return string.Join("\n\n", rendered.Select(static item =>
                    item.title.Length == 0 ? item.body : $"### {item.title}\n\n{item.body}"));
            }
            foreach (var key in new[] { "text", "content", "report", "answer", "error", "message" })
            {
                if (!value.TryGetProperty(key, out var candidate)) continue;
                var text = Text(candidate).Trim();
                if (text.Length > 0) return text;
            }
        }
        return value.ValueKind is JsonValueKind.True or JsonValueKind.False or JsonValueKind.Number
            ? value.ToString()
            : string.Empty;
    }

    private sealed record ReplyProjection(
        string MessageId,
        string Text,
        TaskExecutionCallbackReference? TaskCallback);

    private static TurnStatus MapStatus(string status) => status switch
    {
        "running" => TurnStatus.Streaming,
        "succeeded" => TurnStatus.Completed,
        "cancelled" => TurnStatus.Cancelled,
        _ => TurnStatus.Failed,
    };

    private static string ProcessTitle(TurnStatus status) => status switch
    {
        TurnStatus.Streaming => "本地执行进行中",
        TurnStatus.Completed => "本地执行已完成",
        TurnStatus.Cancelled => "本地执行已取消",
        TurnStatus.Failed => "本地执行失败",
        _ => "本地执行等待中",
    };

    private static long Clamp(ulong value) =>
        value > long.MaxValue ? long.MaxValue : (long)value;

    private string RequireOwner()
    {
        lock (_contextGate)
        {
            return _ownerUserId ?? throw new InvalidOperationException(
                "Local Agent conversation history is not configured.");
        }
    }

    private void SetOwner(string? ownerUserId)
    {
        lock (_contextGate) _ownerUserId = ownerUserId;
    }
}
