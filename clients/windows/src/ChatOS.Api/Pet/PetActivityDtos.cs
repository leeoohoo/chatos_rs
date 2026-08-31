using System.Text.Json.Serialization;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Pet;

internal sealed record PetActivityInboxListDto(
    [property: JsonPropertyName("activities")] IReadOnlyList<PetActivityInboxRecordDto>? Activities);

internal sealed record PetActivityInboxMutationDto(
    [property: JsonPropertyName("success")] bool Success);

internal sealed record PetActivityInboxRecordDto
{
    [JsonPropertyName("id")]
    public required string Id { get; init; }

    [JsonPropertyName("activity_key")]
    public required string ActivityKey { get; init; }

    [JsonPropertyName("activity_version")]
    public required string ActivityVersion { get; init; }

    [JsonPropertyName("source")]
    public required string Source { get; init; }

    [JsonPropertyName("kind")]
    public required string Kind { get; init; }

    [JsonPropertyName("title")]
    public required string Title { get; init; }

    [JsonPropertyName("detail")]
    public string? Detail { get; init; }

    [JsonPropertyName("route")]
    public PetActivityRouteDto? Route { get; init; }

    [JsonPropertyName("inbox_status")]
    public required string InboxStatus { get; init; }

    [JsonPropertyName("event_id")]
    public string? EventId { get; init; }

    [JsonPropertyName("event_sequence")]
    public long? EventSequence { get; init; }

    [JsonPropertyName("occurred_at")]
    public DateTimeOffset OccurredAt { get; init; }

    [JsonPropertyName("updated_at")]
    public DateTimeOffset UpdatedAt { get; init; }

    [JsonPropertyName("expires_at")]
    public DateTimeOffset? ExpiresAt { get; init; }

    public PetActivity? ToDomain()
    {
        if (!TryMapSource(Source, out var source) ||
            !TryMapKind(Kind, out var kind) ||
            !TryMapStatus(InboxStatus, out var status))
        {
            return null;
        }

        return new PetActivity(
            ActivityKey,
            source,
            kind,
            Title,
            Detail,
            Route?.ToDomain(),
            EventId,
            EventSequence,
            Id,
            status,
            ActivityVersion,
            UpdatedAt == default ? OccurredAt : UpdatedAt,
            ExpiresAt);
    }

    private static bool TryMapSource(string value, out PetActivitySource source) =>
        EnumMaps.Sources.TryGetValue(value, out source);

    private static bool TryMapKind(string value, out PetActivityKind kind) =>
        EnumMaps.Kinds.TryGetValue(value, out kind);

    private static bool TryMapStatus(string value, out PetActivityInboxStatus status) =>
        EnumMaps.Statuses.TryGetValue(value, out status);

    private static class EnumMaps
    {
        public static readonly IReadOnlyDictionary<string, PetActivitySource> Sources =
            new Dictionary<string, PetActivitySource>(StringComparer.OrdinalIgnoreCase)
            {
                ["local_approval"] = PetActivitySource.LocalApproval,
                ["ask_user_prompt"] = PetActivitySource.AskUserPrompt,
                ["chat"] = PetActivitySource.Chat,
                ["task_board"] = PetActivitySource.TaskBoard,
                ["task_runner"] = PetActivitySource.TaskRunner,
                ["project_execution"] = PetActivitySource.ProjectExecution,
            };

        public static readonly IReadOnlyDictionary<string, PetActivityKind> Kinds =
            new Dictionary<string, PetActivityKind>(StringComparer.OrdinalIgnoreCase)
            {
                ["working"] = PetActivityKind.Working,
                ["reviewing"] = PetActivityKind.Reviewing,
                ["waiting_for_approval"] = PetActivityKind.WaitingForApproval,
                ["waiting_for_user"] = PetActivityKind.WaitingForUser,
                ["succeeded"] = PetActivityKind.Succeeded,
                ["failed"] = PetActivityKind.Failed,
                ["blocked"] = PetActivityKind.Blocked,
                ["cancelled"] = PetActivityKind.Cancelled,
                ["canceled"] = PetActivityKind.Cancelled,
            };

        public static readonly IReadOnlyDictionary<string, PetActivityInboxStatus> Statuses =
            new Dictionary<string, PetActivityInboxStatus>(StringComparer.OrdinalIgnoreCase)
            {
                ["unread"] = PetActivityInboxStatus.Unread,
                ["displayed"] = PetActivityInboxStatus.Displayed,
                ["acknowledged"] = PetActivityInboxStatus.Acknowledged,
                ["ignored"] = PetActivityInboxStatus.Ignored,
                ["handled"] = PetActivityInboxStatus.Handled,
                ["resolved"] = PetActivityInboxStatus.Resolved,
                ["expired"] = PetActivityInboxStatus.Expired,
            };
    }
}

internal sealed record PetActivityRouteDto
{
    [JsonPropertyName("project_id")]
    public string? ProjectId { get; init; }

    [JsonPropertyName("conversation_id")]
    public string? ConversationId { get; init; }

    [JsonPropertyName("turn_id")]
    public string? TurnId { get; init; }

    [JsonPropertyName("message_id")]
    public string? MessageId { get; init; }

    [JsonPropertyName("prompt_id")]
    public string? PromptId { get; init; }

    [JsonPropertyName("task_id")]
    public string? TaskId { get; init; }

    [JsonPropertyName("run_id")]
    public string? RunId { get; init; }

    public PetActivityRoute ToDomain() => new(
        ProjectId,
        ConversationId,
        TurnId,
        MessageId,
        PromptId,
        TaskId,
        RunId);
}
