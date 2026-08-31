namespace ChatOS.Core.Domain;

public enum TurnStatus
{
    Queued,
    Streaming,
    Completed,
    Failed,
    Cancelled,
}

public enum ChatMessageRole
{
    User,
    Assistant,
    System,
}

public sealed record ChatMessage(
    string Id,
    ChatMessageRole Role,
    string Text,
    DateTimeOffset CreatedAt,
    IReadOnlyList<ConversationAttachmentReference> Attachments);

public sealed record TurnProcessEvent(
    string Id,
    string Title,
    string? Detail,
    TurnStatus Status);

public sealed record TaskRunnerCallbackReference(
    string TaskId,
    string? RunId,
    string? Event,
    string? Status,
    string? SourceConversationId,
    string? SourceTurnId,
    string? SourceUserMessageId);

public sealed record ConversationAssistantReply(
    ChatMessage Message,
    TaskRunnerCallbackReference? TaskCallback);

public sealed record MessageTaskLookup(
    string ConversationId,
    string? TurnId,
    string? SourceUserMessageId);

public sealed record ProjectExecutionContext(
    string? ProjectId = null,
    string? RequirementId = null,
    string? ExecutionGroupId = null,
    string? ReplacedExecutionGroupId = null,
    string? ContactId = null,
    string? Mode = null,
    string? ExecutionKind = null,
    string? ConfirmationStatus = null,
    string? OverallStatus = null);

public sealed record ConversationTurn(
    string Id,
    string ConversationId,
    long Sequence,
    long Revision,
    ChatMessage UserMessage,
    IReadOnlyList<TurnProcessEvent> ProcessEvents,
    ChatMessage? FinalAssistantMessage,
    IReadOnlyList<ConversationAssistantReply> AssistantReplies,
    MessageTaskLookup? MessageTaskLookup,
    bool IsTaskGraphAvailable,
    TurnStatus Status,
    DateTimeOffset StartedAt,
    DateTimeOffset? CompletedAt,
    ProjectExecutionContext? ProjectExecutionContext = null);

public sealed record ConversationHistoryQuery(
    string ConversationId,
    int Limit,
    string? Before,
    long RequestGeneration);

public sealed record HistoryPage(
    IReadOnlyList<ConversationTurn> Turns,
    string? OlderCursor,
    bool HasOlder,
    long SnapshotRevision,
    long RequestGeneration);

public enum ConversationHistoryPageOrigin
{
    Latest,
    Older,
}

public sealed record RealtimeTurnEvent(
    string EventId,
    long EventSequence,
    ConversationTurn Turn);

public sealed record ViewportAnchor(
    string TurnId,
    double RelativeOffset,
    bool IsPinnedToBottom);

public sealed record ConversationHistorySnapshot(
    string ConversationId,
    IReadOnlyList<ConversationTurn> Turns,
    string? OlderCursor,
    bool HasOlder,
    long SnapshotRevision,
    ViewportAnchor? ViewportAnchor,
    int UnreadNewerCount);
