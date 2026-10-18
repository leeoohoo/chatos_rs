namespace ChatOS.Core.Domain;

public sealed record ConversationRuntimeSettings(
    string? SelectedModelId,
    string? SelectedModelName,
    string? SelectedThinkingLevel,
    bool ReasoningEnabled,
    string? RemoteConnectionId = null);

public sealed record ConversationModelOption(
    string Id,
    string DisplayName,
    string ModelName,
    string? ThinkingLevel,
    bool TaskEnabled = true,
    bool HasApiKey = true,
    IReadOnlyList<string>? ThinkingLevels = null)
{
    /// <summary>
    /// The reasoning levels exposed by the provider for this model.  Older
    /// connector snapshots only carry a default thinking level, so keep the
    /// property optional and let callers treat that value as the only
    /// supported level when no explicit catalog was published.
    /// </summary>
    public IReadOnlyList<string> EffectiveThinkingLevels =>
        ThinkingLevels is { Count: > 0 }
            ? ThinkingLevels
            : string.IsNullOrWhiteSpace(ThinkingLevel)
                ? Array.Empty<string>()
                : new[] { "none", ThinkingLevel.Trim().ToLowerInvariant() };
}

public sealed record ConversationRemoteConnectionOption(
    string? Id,
    string DisplayName);

public sealed record ConversationCapabilityOption(
    string Kind,
    string DisplayName,
    string Description,
    string? PluginKey = null,
    string? ComponentKey = null);

public sealed record ConversationExplicitCapabilitySelection(
    string Kind,
    string DisplayName,
    string? PluginKey = null,
    string? ComponentKey = null);

public sealed record ConversationSendCommand(
    string ConversationId,
    string TurnId,
    string Content,
    IReadOnlyList<ConversationAttachmentDraft> Attachments,
    bool? ReasoningEnabled = null,
    ConversationExplicitCapabilitySelection? ExplicitCapabilitySelection = null);

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
