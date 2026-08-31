using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IRealtimeClient
{
    IAsyncEnumerable<ConversationRealtimeSignal> StreamConversationAsync(
        string conversationId,
        CancellationToken cancellationToken = default);

    IAsyncEnumerable<PetActivityEvent> StreamPetActivitiesAsync(
        CancellationToken cancellationToken = default);
}
