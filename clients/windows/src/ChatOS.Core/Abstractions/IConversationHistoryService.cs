using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IConversationHistoryService
{
    Task<HistoryPage> FetchHistoryAsync(
        ConversationHistoryQuery query,
        CancellationToken cancellationToken = default);
}
