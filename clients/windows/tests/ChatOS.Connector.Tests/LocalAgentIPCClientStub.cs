using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

internal abstract class LocalAgentIPCClientStub : ILocalAgentIPCClient
{
    public virtual Task<LocalAgentResponse> SendAsync(LocalAgentCommand command,
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
    public virtual Task<string> AcceptAsync(LocalAgentCommand command,
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
    public virtual Task<LocalAgentRunCreatedResponse> CreateMainChatTurnAsync(
        LocalAgentCreateMainChatTurn command, CancellationToken cancellationToken = default) =>
        throw new NotSupportedException();
    public virtual Task<LocalAgentRunCreatedResponse> CreateTaskAsync(LocalAgentCreateTask command,
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
    public virtual Task<LocalAgentRunCreatedResponse> RetryTaskAsync(LocalAgentRetryTask command,
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
    public virtual Task<LocalAgentRunSnapshot> GetRunAsync(string runId,
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
    public virtual Task<LocalAgentRunDetail> GetRunDetailAsync(string runId, uint eventLimit = 40,
        uint eventOffset = 0, CancellationToken cancellationToken = default) =>
        throw new NotSupportedException();
    public virtual Task<LocalAgentTaskSnapshot> GetTaskAsync(string taskId,
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
    public virtual Task<LocalAgentTaskGraphSnapshot> GetTaskGraphAsync(string sourceThreadId,
        string sourceTurnId, CancellationToken cancellationToken = default) =>
        throw new NotSupportedException();
    public virtual Task<LocalAgentTaskRunDetail> GetTaskRunDetailAsync(string taskId, string runId,
        uint eventLimit = 40, uint eventOffset = 0,
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
    public virtual Task<LocalAgentMainChatRunBinding> GetMainChatRunBindingAsync(string runId,
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
    public virtual Task<LocalAgentRunPage> ListRunsAsync(string? cursor = null, uint limit = 100,
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
    public virtual Task<LocalAgentTaskPage> ListTasksAsync(string? cursor = null, uint limit = 100,
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
    public virtual Task<LocalAgentEventPage> SubscribeRunEventsAsync(ulong afterSequence,
        uint limit = 200, CancellationToken cancellationToken = default) =>
        throw new NotSupportedException();
    public virtual Task<ulong> GetUIEventCursorAsync(
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
    public virtual Task<ulong> AcknowledgeUIEventsAsync(ulong throughSequence,
        CancellationToken cancellationToken = default) => throw new NotSupportedException();
}
