namespace ChatOS.Core.Domain;

public enum ConversationRealtimeKind
{
    Started,
    Updated,
    Persisted,
    Completed,
    Failed,
    Cancelled,
    Unknown,
}

public sealed record AskUserPromptRealtimeUpdate(
    string PromptId,
    string ConversationId,
    string? TurnId,
    string Action,
    string? Status);

public sealed record ConversationRealtimeProcessUpdate(
    string Id,
    string Title,
    string? Detail,
    string Status,
    DateTimeOffset Timestamp);

public sealed record ConversationRealtimeSignal(
    string EventId,
    long EventSequence,
    string ConversationId,
    string? TurnId,
    ConversationRealtimeKind Kind,
    string EventName,
    DateTimeOffset Timestamp,
    AskUserPromptRealtimeUpdate? AskUserPromptUpdate = null,
    ConversationRealtimeProcessUpdate? ProcessUpdate = null);
