using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface ILocalAgentIPCClientFactory
{
    ILocalAgentIPCClient Create(string ownerUserId, string pipeName);
}

public interface ILocalAgentIPCClient
{
    Task<LocalAgentResponse> SendAsync(
        LocalAgentCommand command,
        CancellationToken cancellationToken = default);

    Task<string> AcceptAsync(
        LocalAgentCommand command,
        CancellationToken cancellationToken = default);

    Task<LocalAgentRunSnapshot> GetRunAsync(
        string runId,
        CancellationToken cancellationToken = default);

    Task<LocalAgentRunPage> ListRunsAsync(
        string? cursor = null,
        uint limit = 100,
        CancellationToken cancellationToken = default);

    Task<LocalAgentEventPage> SubscribeRunEventsAsync(
        ulong afterSequence,
        uint limit = 200,
        CancellationToken cancellationToken = default);
}
