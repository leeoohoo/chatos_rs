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

    Task<LocalAgentRunCreatedResponse> CreateMainChatTurnAsync(
        LocalAgentCreateMainChatTurn command,
        CancellationToken cancellationToken = default);

    Task<LocalAgentRunCreatedResponse> CreateTaskAsync(
        LocalAgentCreateTask command,
        CancellationToken cancellationToken = default);

    Task<LocalAgentRunCreatedResponse> RetryTaskAsync(
        LocalAgentRetryTask command,
        CancellationToken cancellationToken = default);

    Task<LocalAgentRunSnapshot> GetRunAsync(
        string runId,
        CancellationToken cancellationToken = default);

    Task<LocalAgentTaskSnapshot> GetTaskAsync(
        string taskId,
        CancellationToken cancellationToken = default);

    Task<LocalAgentTaskGraphSnapshot> GetTaskGraphAsync(
        string sourceThreadId,
        string sourceTurnId,
        CancellationToken cancellationToken = default);

    Task<LocalAgentTaskRunDetail> GetTaskRunDetailAsync(
        string taskId,
        string runId,
        uint eventLimit = 40,
        uint eventOffset = 0,
        CancellationToken cancellationToken = default);

    Task<LocalAgentMainChatRunBinding> GetMainChatRunBindingAsync(
        string runId,
        CancellationToken cancellationToken = default);

    Task<LocalAgentRunPage> ListRunsAsync(
        string? cursor = null,
        uint limit = 100,
        CancellationToken cancellationToken = default);

    Task<LocalAgentTaskPage> ListTasksAsync(
        string? cursor = null,
        uint limit = 100,
        CancellationToken cancellationToken = default);

    Task<LocalAgentEventPage> SubscribeRunEventsAsync(
        ulong afterSequence,
        uint limit = 200,
        CancellationToken cancellationToken = default);

    Task<ulong> GetUIEventCursorAsync(CancellationToken cancellationToken = default);

    Task<ulong> AcknowledgeUIEventsAsync(
        ulong throughSequence,
        CancellationToken cancellationToken = default);
}
