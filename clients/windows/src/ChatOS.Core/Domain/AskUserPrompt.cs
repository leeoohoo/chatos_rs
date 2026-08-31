namespace ChatOS.Core.Domain;

public enum AskUserPromptStatus
{
    Pending,
    Ok,
    Canceled,
    Timeout,
    Failed,
}

public sealed record AskUserField(
    string Key,
    string Label,
    string? Description,
    string? Placeholder,
    string DefaultValue,
    bool IsRequired,
    bool IsMultiline,
    bool IsSecret);

public sealed record AskUserChoiceOption(
    string Value,
    string Label,
    string? Description);

public sealed record AskUserChoice(
    bool AllowsMultiple,
    IReadOnlyList<AskUserChoiceOption> Options,
    IReadOnlyList<string> DefaultSelection,
    int MinimumSelectionCount,
    int MaximumSelectionCount);

public sealed record AskUserPrompt(
    string Id,
    string ConversationId,
    string TurnId,
    string? ToolCallId,
    string Kind,
    AskUserPromptStatus Status,
    string Title,
    string Message,
    bool AllowsCancel,
    long? TimeoutMilliseconds,
    IReadOnlyList<AskUserField> Fields,
    AskUserChoice? Choice,
    DateTimeOffset? CreatedAt,
    DateTimeOffset? UpdatedAt)
{
    public bool IsPending => Status == AskUserPromptStatus.Pending;
}

public abstract record AskUserSelection
{
    public sealed record Single(string Value) : AskUserSelection;

    public sealed record Multiple(IReadOnlyList<string> Values) : AskUserSelection;
}

public sealed record AskUserSubmission(
    IReadOnlyDictionary<string, string> Values,
    AskUserSelection? Selection = null)
{
    public static AskUserSubmission Empty { get; } = new(
        new Dictionary<string, string>());
}
