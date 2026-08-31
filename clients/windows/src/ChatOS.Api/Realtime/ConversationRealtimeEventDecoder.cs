using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Realtime;

public static class ConversationRealtimeEventDecoder
{
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    public static ConversationRealtimeSignal? Decode(string json, string expectedConversationId)
    {
        RealtimeEnvelopeDto? envelope;
        try
        {
            envelope = JsonSerializer.Deserialize<RealtimeEnvelopeDto>(json, JsonOptions);
        }
        catch (JsonException)
        {
            return null;
        }

        if (envelope is null ||
            !string.Equals(envelope.Type, "event", StringComparison.Ordinal) ||
            envelope.Payload is null)
        {
            return null;
        }

        var conversationId = envelope.ConversationId ?? envelope.Payload.ConversationId ?? string.Empty;
        if (!string.Equals(conversationId, expectedConversationId, StringComparison.Ordinal))
        {
            return null;
        }

        var timestamp = DateTimeOffset.TryParse(envelope.Timestamp, out var parsedTimestamp)
            ? parsedTimestamp
            : DateTimeOffset.MinValue;
        if (string.Equals(envelope.Payload.Kind, "ask_user_prompt", StringComparison.Ordinal))
        {
            if (string.IsNullOrWhiteSpace(envelope.Payload.PromptId))
            {
                return null;
            }

            return new ConversationRealtimeSignal(
                envelope.EventId,
                envelope.EventSequence,
                conversationId,
                envelope.Payload.TurnId,
                ConversationRealtimeKind.Unknown,
                envelope.Event,
                timestamp,
                new AskUserPromptRealtimeUpdate(
                    envelope.Payload.PromptId,
                    conversationId,
                    envelope.Payload.TurnId,
                    envelope.Payload.Action ?? string.Empty,
                    envelope.Payload.Status?.ToLowerInvariant()));
        }

        if (!string.Equals(envelope.Payload.Kind, "chat_stream", StringComparison.Ordinal))
        {
            return null;
        }

        var normalizedType = FirstString(envelope.Payload.Raw, "type")
            ?? envelope.Payload.StreamType
            ?? envelope.Event;
        normalizedType = normalizedType.ToLowerInvariant().Replace('-', '_');
        return new ConversationRealtimeSignal(
            envelope.EventId,
            envelope.EventSequence,
            conversationId,
            envelope.Payload.TurnId,
            MapKind(normalizedType),
            envelope.Event,
            timestamp,
            ProcessUpdate: ProcessUpdate(envelope, normalizedType, timestamp));
    }

    private static ConversationRealtimeProcessUpdate? ProcessUpdate(
        RealtimeEnvelopeDto envelope,
        string eventType,
        DateTimeOffset timestamp)
    {
        var raw = envelope.Payload?.Raw;
        string title;
        string? detail = null;
        string status;

        if (eventType == "start" || envelope.Event.Contains("turn.started", StringComparison.Ordinal))
        {
            title = "AI 已开始生成执行计划";
            detail = "正在读取需求、技术文档和项目任务";
            status = "running";
        }
        else if (eventType.Contains("thinking", StringComparison.Ordinal))
        {
            title = "正在分析需求与任务依赖";
            status = "running";
        }
        else if (eventType.Contains("turn_phase", StringComparison.Ordinal) || eventType == "phase")
        {
            title = "AI 进入新的处理阶段";
            detail = FirstString(raw, "data", "phase")
                ?? FirstString(raw, "data", "status")
                ?? FirstString(raw, "data", "name");
            status = "running";
        }
        else if (eventType.Contains("tools_start", StringComparison.Ordinal) ||
                 envelope.Event.Contains("tool.started", StringComparison.Ordinal))
        {
            var names = ToolNames(raw);
            title = names.Count == 0 ? "正在调用规划工具" : $"正在调用工具：{string.Join('、', names)}";
            detail = "正在读取上下文或创建任务节点";
            status = "running";
        }
        else if (eventType.Contains("tools_end", StringComparison.Ordinal) ||
                 eventType.Contains("tool_completed", StringComparison.Ordinal) ||
                 envelope.Event.Contains("tool.completed", StringComparison.Ordinal))
        {
            title = "工具调用已完成";
            status = "completed";
        }
        else if (eventType.Contains("complete", StringComparison.Ordinal) ||
                 eventType.Contains("finish", StringComparison.Ordinal))
        {
            title = "AI 执行计划已生成";
            detail = "正在同步任务流程图和确认状态";
            status = "completed";
        }
        else if (eventType.Contains("fail", StringComparison.Ordinal) ||
                 eventType.Contains("error", StringComparison.Ordinal))
        {
            title = "AI 生成执行计划失败";
            detail = FirstString(raw, "error")
                ?? FirstString(raw, "message")
                ?? FirstString(raw, "data", "error")
                ?? FirstString(raw, "data", "message");
            status = "failed";
        }
        else if (eventType.Contains("cancel", StringComparison.Ordinal))
        {
            title = "AI 生成过程已取消";
            status = "cancelled";
        }
        else
        {
            return null;
        }

        return new ConversationRealtimeProcessUpdate(
            envelope.EventId,
            title,
            detail,
            status,
            timestamp);
    }

    private static ConversationRealtimeKind MapKind(string value)
    {
        if (value.Contains("cancel", StringComparison.Ordinal)) return ConversationRealtimeKind.Cancelled;
        if (value.Contains("fail", StringComparison.Ordinal) || value.Contains("error", StringComparison.Ordinal)) return ConversationRealtimeKind.Failed;
        if (value.Contains("complete", StringComparison.Ordinal) || value.Contains("finish", StringComparison.Ordinal) || value.Contains("final", StringComparison.Ordinal)) return ConversationRealtimeKind.Completed;
        if (value.Contains("persist", StringComparison.Ordinal) || value.Contains("callback", StringComparison.Ordinal)) return ConversationRealtimeKind.Persisted;
        if (value.Contains("start", StringComparison.Ordinal)) return ConversationRealtimeKind.Started;
        if (value.Contains("delta", StringComparison.Ordinal) || value.Contains("stream", StringComparison.Ordinal) || value.Contains("update", StringComparison.Ordinal)) return ConversationRealtimeKind.Updated;
        return ConversationRealtimeKind.Unknown;
    }

    private static string? FirstString(JsonElement? element, params string[] path)
    {
        if (element is not { } current)
        {
            return null;
        }

        foreach (var component in path)
        {
            if (current.ValueKind != JsonValueKind.Object ||
                !current.TryGetProperty(component, out current))
            {
                return null;
            }
        }

        return current.ValueKind == JsonValueKind.String ? current.GetString() : null;
    }

    private static IReadOnlyList<string> ToolNames(JsonElement? raw)
    {
        if (raw is not { } root || root.ValueKind != JsonValueKind.Object ||
            !root.TryGetProperty("data", out var data) || data.ValueKind != JsonValueKind.Object ||
            !data.TryGetProperty("tool_calls", out var calls))
        {
            return Array.Empty<string>();
        }

        IEnumerable<JsonElement> values = calls.ValueKind == JsonValueKind.Array
            ? calls.EnumerateArray().ToArray()
            : new[] { calls };
        var names = new List<string>();
        var seen = new HashSet<string>(StringComparer.Ordinal);
        foreach (var value in values)
        {
            if (value.ValueKind != JsonValueKind.Object)
            {
                continue;
            }

            var name = FirstString(value, "name")
                ?? FirstString(value, "function", "name")
                ?? FirstString(value, "tool_name");
            if (!string.IsNullOrWhiteSpace(name) && seen.Add(name))
            {
                names.Add(name);
            }
        }

        return names;
    }
}

internal sealed record RealtimeEnvelopeDto
{
    [JsonPropertyName("type")]
    public required string Type { get; init; }

    [JsonPropertyName("event")]
    public required string Event { get; init; }

    [JsonPropertyName("event_id")]
    public required string EventId { get; init; }

    [JsonPropertyName("event_sequence")]
    public long EventSequence { get; init; }

    [JsonPropertyName("conversation_id")]
    public string? ConversationId { get; init; }

    [JsonPropertyName("payload")]
    public RealtimePayloadDto? Payload { get; init; }

    [JsonPropertyName("ts")]
    public string? Timestamp { get; init; }
}

internal sealed record RealtimePayloadDto
{
    [JsonPropertyName("kind")]
    public required string Kind { get; init; }

    [JsonPropertyName("conversation_id")]
    public string? ConversationId { get; init; }

    [JsonPropertyName("conversation_turn_id")]
    public string? TurnId { get; init; }

    [JsonPropertyName("stream_type")]
    public string? StreamType { get; init; }

    [JsonPropertyName("raw")]
    public JsonElement? Raw { get; init; }

    [JsonPropertyName("prompt_id")]
    public string? PromptId { get; init; }

    [JsonPropertyName("action")]
    public string? Action { get; init; }

    [JsonPropertyName("status")]
    public string? Status { get; init; }
}
