using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IConversationRuntimeSettingsService
{
    Task<ConversationRuntimeSettings> FetchAsync(
        string conversationId,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<ConversationModelOption>> FetchAvailableModelsAsync(
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<ConversationCapabilityOption>> FetchAvailableCapabilitiesAsync(
        CancellationToken cancellationToken = default) =>
        Task.FromResult<IReadOnlyList<ConversationCapabilityOption>>([]);

    Task<ConversationRuntimeSettings> UpdateModelAsync(
        string conversationId,
        string modelId,
        CancellationToken cancellationToken = default);

    Task<ConversationRuntimeSettings> UpdateReasoningAsync(
        string conversationId,
        bool enabled,
        CancellationToken cancellationToken = default);

    /// <summary>
    /// Persists a concrete reasoning level. Implementations that only support
    /// the legacy on/off contract can use the default fallback.
    /// </summary>
    async Task<ConversationRuntimeSettings> UpdateReasoningLevelAsync(
        string conversationId,
        string level,
        CancellationToken cancellationToken = default) =>
        await UpdateReasoningAsync(
            conversationId,
            !string.Equals(level, "none", StringComparison.OrdinalIgnoreCase),
            cancellationToken).ConfigureAwait(false);

    Task<ConversationRuntimeSettings> UpdateRemoteConnectionAsync(
        string conversationId,
        string? remoteConnectionId,
        CancellationToken cancellationToken = default) =>
        throw new NotSupportedException("Remote connection selection is not supported.");
}

public interface IConversationCommandService
{
    Task<ConversationCommandAck> SendNewTurnAsync(
        ConversationSendCommand command,
        CancellationToken cancellationToken = default);

    Task<ConversationCommandAck> SendGuidanceAsync(
        ConversationSendCommand command,
        CancellationToken cancellationToken = default);

    Task StopTurnAsync(
        string conversationId,
        string? turnId,
        CancellationToken cancellationToken = default);
}
