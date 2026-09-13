using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

/// <summary>
/// Native task workspace contract backed exclusively by the local Agent Host.
/// Implementations must preserve the task's frozen project and execution snapshots.
/// </summary>
public interface ILocalAgentTaskService
{
    event EventHandler? AccountProjectionCleared;

    Task<LocalAgentTaskGraphSnapshot> GetGraphAsync(
        string sourceThreadId,
        string sourceTurnId,
        CancellationToken cancellationToken = default);

    Task<LocalAgentTaskSnapshot> GetTaskAsync(
        string taskId,
        CancellationToken cancellationToken = default);

    Task<LocalAgentTaskRunDetail> GetRunDetailAsync(
        string taskId,
        string runId,
        uint eventLimit = 40,
        uint eventOffset = 0,
        CancellationToken cancellationToken = default);

    Task<LocalAgentRunCreatedResponse> RetryCurrentRunAsync(
        string taskId,
        string expectedRunId,
        string? instruction,
        CancellationToken cancellationToken = default);
}
