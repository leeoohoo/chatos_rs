using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface ILocalAgentRunControlService
{
    Task<IReadOnlyList<LocalAgentRunControlState>> FetchRunControlsAsync(
        string conversationId,
        CancellationToken cancellationToken = default);

    Task PauseRunAsync(string runId, string conversationId,
        CancellationToken cancellationToken = default);
    Task ResumeRunAsync(string runId, string conversationId,
        CancellationToken cancellationToken = default);
    Task CancelRunAsync(string runId, string conversationId,
        CancellationToken cancellationToken = default);
}
