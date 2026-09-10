namespace ChatOS.Core.Domain;

public sealed record ConversationRuntimeSettings(
    string? SelectedModelId,
    string? SelectedModelName,
    string? SelectedThinkingLevel,
    bool ReasoningEnabled);

public sealed record ConversationModelOption(
    string Id,
    string DisplayName,
    string ModelName,
    string? ThinkingLevel,
    bool TaskEnabled = true,
    bool HasApiKey = true);

public sealed record ConversationSendCommand(
    string ConversationId,
    string TurnId,
    string Content,
    IReadOnlyList<ConversationAttachmentDraft> Attachments,
    bool? ReasoningEnabled = null);

public sealed record ConversationCommandAck(
    bool Accepted,
    string TurnId,
    string? UserMessageId);

public sealed class GuidanceTargetInactiveException : Exception
{
    public GuidanceTargetInactiveException()
        : base("The target turn has ended. Send the content as a new message instead.")
    {
    }
}
