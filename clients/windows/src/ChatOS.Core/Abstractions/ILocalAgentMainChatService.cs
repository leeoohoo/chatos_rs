using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface ILocalAgentContactRuntimeContextService
{
    Task<LocalAgentContactRuntimeContext> FetchAsync(
        string agentId,
        CancellationToken cancellationToken = default);
}

public interface ILocalAgentMainChatService
{
    event EventHandler? ProjectionChanged;
    event EventHandler? ProjectionCleared;

    Task<LocalAgentConversationSnapshot> GetConversationAsync(
        string threadId,
        CancellationToken cancellationToken = default);

    Task<LocalAgentRunCreatedResponse> CreateTurnAsync(
        LocalAgentCreateConversationTurn command,
        CancellationToken cancellationToken = default);

}
