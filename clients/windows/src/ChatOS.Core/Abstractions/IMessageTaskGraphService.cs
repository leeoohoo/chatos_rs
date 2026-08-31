using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public interface IMessageTaskGraphService
{
    Task<MessageTaskGraphSnapshot> FetchGraphAsync(
        string messageId,
        MessageTaskLookup? lookup,
        CancellationToken cancellationToken = default);

    Task<MessageTask> FetchTaskAsync(
        string messageId,
        string taskId,
        MessageTaskLookup? lookup,
        CancellationToken cancellationToken = default);

    Task<MessageTaskRunDetail> FetchRunAsync(
        string messageId,
        string runId,
        MessageTaskLookup? lookup,
        bool includeEvents = true,
        int eventLimit = 40,
        int eventOffset = 0,
        CancellationToken cancellationToken = default);

    Task<MessageTaskRun> RetryRunAsync(
        string messageId,
        string runId,
        MessageTaskLookup? lookup,
        string? instruction,
        CancellationToken cancellationToken = default);

    Task CancelTaskAsync(
        string messageId,
        string taskId,
        MessageTaskLookup? lookup,
        string? reason,
        CancellationToken cancellationToken = default);
}
