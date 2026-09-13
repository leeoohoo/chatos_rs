using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IConversationRuntimeSettingsService
{
    Task<ConversationRuntimeSettings> FetchAsync(
        string conversationId,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<ConversationModelOption>> FetchAvailableModelsAsync(
        CancellationToken cancellationToken = default);

    Task<ConversationRuntimeSettings> UpdateModelAsync(
        string conversationId,
        string modelId,
        CancellationToken cancellationToken = default);

    Task<ConversationRuntimeSettings> UpdateReasoningAsync(
        string conversationId,
        bool enabled,
        CancellationToken cancellationToken = default);
}

public interface IPetConversationControl
{
    Task StopTurnAsync(
        string conversationId,
        string? turnId,
        CancellationToken cancellationToken = default);
}
