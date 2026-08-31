using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IConversationAttachmentService
{
    Task<IReadOnlyList<ConversationAttachmentReference>> UploadAsync(
        IReadOnlyList<ConversationAttachmentDraft> attachments,
        string conversationId,
        CancellationToken cancellationToken = default);
}

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

    Task<ConversationRuntimeSettings> UpdatePlanModeAsync(
        string conversationId,
        bool enabled,
        CancellationToken cancellationToken = default);

    Task<ConversationRuntimeSettings> UpdateReasoningAsync(
        string conversationId,
        bool enabled,
        CancellationToken cancellationToken = default);
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
