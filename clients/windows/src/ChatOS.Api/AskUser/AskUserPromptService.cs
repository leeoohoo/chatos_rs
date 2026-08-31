using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.AskUser;

public sealed class AskUserPromptService : IAskUserPromptService
{
    private readonly ChatOSApiClient _client;

    public AskUserPromptService(ChatOSApiClient client)
    {
        _client = client;
    }

    public async Task<IReadOnlyList<AskUserPrompt>> FetchPromptsAsync(
        string conversationId,
        int limit = 100,
        CancellationToken cancellationToken = default)
    {
        var normalizedLimit = Math.Clamp(limit, 1, 500);
        var response = await _client.GetAsync<AskUserPromptListDto>(
            $"ask-user-prompts?conversation_id={Uri.EscapeDataString(conversationId)}&include_pending=true&limit={normalizedLimit}",
            cancellationToken).ConfigureAwait(false);
        return (response.Prompts ?? Array.Empty<AskUserPromptRecordDto>())
            .Select(static prompt => prompt.ToDomain())
            .OfType<AskUserPrompt>()
            .ToArray();
    }

    public async Task<AskUserPrompt> SubmitAsync(
        string promptId,
        string conversationId,
        AskUserSubmission submission,
        CancellationToken cancellationToken = default)
    {
        object? selection = submission.Selection switch
        {
            AskUserSelection.Single single => single.Value,
            AskUserSelection.Multiple multiple => multiple.Values,
            _ => null,
        };
        var body = new AskUserPromptSubmissionDto(
            conversationId,
            submission.Values.Count == 0 ? null : submission.Values,
            selection);
        var response = await _client.PostAsync<AskUserPromptMutationDto>(
            $"ask-user-prompts/{Uri.EscapeDataString(promptId)}/submit",
            body,
            cancellationToken).ConfigureAwait(false);
        return response.Prompt.ToDomain()
            ?? throw new ChatOSApiException("The gateway returned an unsupported Ask User prompt.");
    }

    public async Task<AskUserPrompt> CancelAsync(
        string promptId,
        string conversationId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.PostAsync<AskUserPromptMutationDto>(
            $"ask-user-prompts/{Uri.EscapeDataString(promptId)}/cancel",
            new AskUserPromptCancelDto(conversationId, "user_cancelled"),
            cancellationToken).ConfigureAwait(false);
        return response.Prompt.ToDomain()
            ?? throw new ChatOSApiException("The gateway returned an unsupported Ask User prompt.");
    }
}

internal sealed record AskUserPromptListDto
{
    [JsonPropertyName("prompts")]
    public IReadOnlyList<AskUserPromptRecordDto>? Prompts { get; init; }
}

internal sealed record AskUserPromptMutationDto(
    [property: JsonPropertyName("prompt")] AskUserPromptRecordDto Prompt);

internal sealed record AskUserPromptSubmissionDto(
    [property: JsonPropertyName("conversation_id")] string ConversationId,
    [property: JsonPropertyName("values")] IReadOnlyDictionary<string, string>? Values,
    [property: JsonPropertyName("selection")] object? Selection);

internal sealed record AskUserPromptCancelDto(
    [property: JsonPropertyName("conversation_id")] string ConversationId,
    [property: JsonPropertyName("reason")] string Reason);

internal sealed record AskUserPromptRecordDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("conversation_id")]
    public required string ConversationId { get; init; }

    [JsonPropertyName("conversation_turn_id")]
    public required string ConversationTurnId { get; init; }

    [JsonPropertyName("tool_call_id")]
    public string? ToolCallId { get; init; }

    [JsonPropertyName("kind")]
    public required string Kind { get; init; }

    [JsonPropertyName("status")]
    public required string Status { get; init; }

    [JsonPropertyName("prompt")]
    public JsonElement Prompt { get; init; }

    [JsonPropertyName("created_at")]
    public string? CreatedAt { get; init; }

    [JsonPropertyName("updated_at")]
    public string? UpdatedAt { get; init; }

    public AskUserPrompt? ToDomain()
    {
        if (!TryMapStatus(Status, out var status))
        {
            return null;
        }

        var stored = Prompt.ValueKind == JsonValueKind.Object ? Prompt : default;
        var payload = stored.ChildObject("payload");
        var fields = payload.ChildArray("fields")
            .Select(MapField)
            .OfType<AskUserField>()
            .ToArray();
        var choice = MapChoice(payload.ChildObject("choice"));
        return new AskUserPrompt(
            Id,
            ConversationId,
            ConversationTurnId,
            ToolCallId.TrimmedOrNull() ?? stored.String("tool_call_id"),
            Kind,
            status,
            stored.UntrimmedString("title") ?? string.Empty,
            stored.UntrimmedString("message") ?? string.Empty,
            stored.Boolean("allow_cancel") ?? true,
            stored.Int64("timeout_ms"),
            fields,
            choice,
            ParseDate(CreatedAt),
            ParseDate(UpdatedAt));
    }

    private static AskUserField? MapField(JsonElement value, int index)
    {
        if (value.ValueKind != JsonValueKind.Object)
        {
            return null;
        }

        var label = value.UntrimmedString("label")?.Trim();
        var key = value.String("key")
            ?? value.String("name")
            ?? value.String("id")
            ?? NormalizeFieldKey(label)
            ?? $"field_{index + 1}";
        if (key.Length == 0)
        {
            return null;
        }

        return new AskUserField(
            key,
            string.IsNullOrWhiteSpace(label) ? key : label,
            value.String("description"),
            value.UntrimmedString("placeholder"),
            value.UntrimmedString("default_value") ?? value.UntrimmedString("default") ?? string.Empty,
            value.Boolean("required") ?? false,
            value.Boolean("multiline") ?? false,
            value.Boolean("secret") ?? false);
    }

    private static AskUserChoice? MapChoice(JsonElement value)
    {
        if (value.ValueKind != JsonValueKind.Object)
        {
            return null;
        }

        var options = value.ChildArray("options")
            .Select(option => new AskUserChoiceOption(
                option.String("value") ?? string.Empty,
                option.UntrimmedString("label") ?? option.String("value") ?? string.Empty,
                option.String("description")))
            .Where(static option => option.Value.Length > 0)
            .ToArray();
        if (options.Length == 0)
        {
            return null;
        }

        var allowsMultiple = value.Boolean("multiple") ?? false;
        var defaults = value.TryGetProperty("default", out var defaultValue) &&
                       defaultValue.ValueKind == JsonValueKind.Array
            ? defaultValue.EnumerateArray()
                .Where(static item => item.ValueKind == JsonValueKind.String)
                .Select(static item => item.GetString())
                .OfType<string>()
                .ToArray()
            : value.String("default") is { } single ? new[] { single } : Array.Empty<string>();
        var minimum = Math.Max(0, value.Int32("min_selections") ?? 0);
        var maximum = Math.Max(
            minimum,
            value.Int32("max_selections") ?? (allowsMultiple ? options.Length : 1));
        return new AskUserChoice(
            allowsMultiple,
            options,
            defaults,
            minimum,
            maximum);
    }

    private static string? NormalizeFieldKey(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return null;
        }

        var chars = value.ToLowerInvariant()
            .Select(static character => char.IsLetterOrDigit(character) || character == '_'
                ? character
                : '_')
            .ToArray();
        return new string(chars).Trim('_');
    }

    private static bool TryMapStatus(string value, out AskUserPromptStatus status)
    {
        switch (value.Trim().ToLowerInvariant())
        {
            case "pending": status = AskUserPromptStatus.Pending; return true;
            case "ok": status = AskUserPromptStatus.Ok; return true;
            case "canceled":
            case "cancelled": status = AskUserPromptStatus.Canceled; return true;
            case "timeout": status = AskUserPromptStatus.Timeout; return true;
            case "failed": status = AskUserPromptStatus.Failed; return true;
            default: status = default; return false;
        }
    }

    private static DateTimeOffset? ParseDate(string? value) =>
        DateTimeOffset.TryParse(value, out var date) ? date : null;
}

internal static class AskUserJsonExtensions
{
    public static JsonElement ChildObject(this JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var child) &&
        child.ValueKind == JsonValueKind.Object
            ? child
            : default;

    public static IEnumerable<JsonElement> ChildArray(this JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var child) &&
        child.ValueKind == JsonValueKind.Array
            ? child.EnumerateArray().ToArray()
            : Array.Empty<JsonElement>();

    public static string? String(this JsonElement value, string name) =>
        value.UntrimmedString(name).TrimmedOrNull();

    public static string? UntrimmedString(this JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var child) &&
        child.ValueKind == JsonValueKind.String
            ? child.GetString()
            : null;

    public static bool? Boolean(this JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var child) &&
        child.ValueKind is JsonValueKind.True or JsonValueKind.False
            ? child.GetBoolean()
            : null;

    public static int? Int32(this JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var child) &&
        child.TryGetInt32(out var number)
            ? number
            : null;

    public static long? Int64(this JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var child) &&
        child.TryGetInt64(out var number)
            ? number
            : null;

    public static string? TrimmedOrNull(this string? value)
    {
        value = value?.Trim();
        return string.IsNullOrEmpty(value) ? null : value;
    }
}
