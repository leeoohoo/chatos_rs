using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IConversationCacheStore
{
    Task<IReadOnlyList<ConversationTurn>> LoadAsync(
        string conversationId,
        CancellationToken cancellationToken = default);

    Task SaveAsync(
        string conversationId,
        IReadOnlyList<ConversationTurn> turns,
        CancellationToken cancellationToken = default);

    Task DeleteAsync(
        string conversationId,
        CancellationToken cancellationToken = default);
}
