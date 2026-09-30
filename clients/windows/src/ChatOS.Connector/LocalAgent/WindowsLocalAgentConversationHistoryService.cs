using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentConversationHistoryService : IConversationHistoryService
{
    private readonly WindowsLocalAgentConversationClient _client;
    private readonly object _contextGate = new();
    private string? _ownerUserId;

    public WindowsLocalAgentConversationHistoryService(
        WindowsLocalAgentConversationClient client)
    {
        _client = client;
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
        return Map(page, query.RequestGeneration);
    }

    private static HistoryPage Map(
        WindowsLocalConversationHistoryPage page,
        long requestGeneration)
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
            var assistants = messages.Where(message => message.Role == "assistant").ToArray();
            var userMessage = MapMessage(
                user,
                turn.UserMessageId,
                attachmentsByMessage.GetValueOrDefault(turn.UserMessageId) ?? []);
            var replies = assistants.Select(message => new ConversationAssistantReply(
                MapMessage(
                    message,
                    message.MessageId,
                    attachmentsByMessage.GetValueOrDefault(message.MessageId) ?? []),
                null)).ToArray();
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
                replies.LastOrDefault()?.Message,
                replies,
                null,
                true,
                status,
                DateTimeOffset.FromUnixTimeMilliseconds(turn.CreatedAtUnixMs),
                status == TurnStatus.Streaming
                    ? null
                    : DateTimeOffset.FromUnixTimeMilliseconds(turn.UpdatedAtUnixMs));
        }).ToArray();
        return new HistoryPage(
            turns,
            page.NextBeforeOrdinal?.ToString(),
            page.NextBeforeOrdinal is not null,
            Clamp(page.Conversation.Version),
            requestGeneration);
    }

    private static ChatMessage MapMessage(
        WindowsLocalConversationMessageRecord? message,
        string fallbackId,
        IReadOnlyList<WindowsLocalConversationAttachmentRecord> attachments) => new(
            message?.MessageId ?? fallbackId,
            message?.Role == "assistant" ? ChatMessageRole.Assistant : ChatMessageRole.User,
            message is null ? string.Empty : Text(message.Content),
            DateTimeOffset.FromUnixTimeMilliseconds(message?.CreatedAtUnixMs ?? 0),
            attachments.Select(value => new ConversationAttachmentReference(
                value.AttachmentId,
                value.DisplayName,
                value.MediaType,
                value.ByteSize > int.MaxValue ? int.MaxValue : (int)value.ByteSize,
                value.MediaType.StartsWith("image/", StringComparison.OrdinalIgnoreCase)
                    ? ConversationAttachmentKind.Image
                    : value.MediaType.StartsWith("audio/", StringComparison.OrdinalIgnoreCase)
                        ? ConversationAttachmentKind.Audio
                        : ConversationAttachmentKind.File)).ToArray());

    private static string Text(JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.String) return value.GetString() ?? string.Empty;
        if (value.ValueKind == JsonValueKind.Object)
        {
            if (value.TryGetProperty("text", out var text)) return Text(text);
            if (value.TryGetProperty("content", out var content)) return Text(content);
        }
        return value.ValueKind is JsonValueKind.Null or JsonValueKind.Undefined
            ? string.Empty
            : value.ToString();
    }

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
