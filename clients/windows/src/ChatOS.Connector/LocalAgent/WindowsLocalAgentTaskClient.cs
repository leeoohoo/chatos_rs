using System.Text.Json;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalTask(
    string GraphId, string OwnerUserId, string SourceEntityType, string SourceEntityId,
    string TaskId, string Title, string ModelConfigRef, JsonElement Input, string Status,
    string? ActiveRunId, ulong Version, long CreatedAtUnixMs, long UpdatedAtUnixMs);
internal sealed record WindowsLocalTaskDependency(string TaskId, string PrerequisiteTaskId);
internal sealed record WindowsLocalTaskGraph(
    string GraphId, string OwnerUserId, string SourceEntityType, string SourceEntityId,
    string Status, IReadOnlyList<WindowsLocalTask> Tasks,
    IReadOnlyList<WindowsLocalTaskDependency> Dependencies, long CreatedAtUnixMs);
internal sealed record WindowsLocalTaskGraphSummary(
    string GraphId, string SourceEntityType, string SourceEntityId, long UpdatedAtUnixMs);
internal sealed record WindowsLocalTaskGraphPage(
    IReadOnlyList<WindowsLocalTaskGraphSummary> Graphs,
    long? NextBeforeUpdatedAtUnixMs, string? NextBeforeGraphId);
internal sealed record ListLocalTaskGraphsCommand(
    string Type, string OwnerUserId, string Scope, string? SourceEntityType,
    string? SourceEntityId, long? BeforeUpdatedAtUnixMs, string? BeforeGraphId, uint Limit);
internal sealed record GetLocalTaskGraphCommand(string Type, string OwnerUserId, string GraphId);
internal sealed record GetLocalTaskRunsCommand(string Type, string OwnerUserId, string TaskId, uint Limit);
internal sealed record RetryLocalTaskCommand(
    string Type, string OwnerUserId, string TaskId, ulong ExpectedVersion,
    string? RetryInstruction);
internal sealed record RestartLocalTaskCommand(
    string Type, string OwnerUserId, string TaskId, ulong ExpectedVersion,
    string Reason);
internal sealed record LocalTaskGraphsResult(string Type, WindowsLocalTaskGraphPage Page);
internal sealed record LocalTaskGraphTypedResult(string Type, WindowsLocalTaskGraph Graph);
internal sealed record LocalTaskRunsResult(
    string Type, string TaskId, IReadOnlyList<WindowsLocalAgentRun> Runs);

public sealed class WindowsLocalAgentTaskClient(ILocalAgentHostClient host)
{
    internal async Task<IReadOnlyList<WindowsLocalTaskGraph>> MatchingGraphsAsync(
        string owner, string? turnId, string? requiredTaskId,
        CancellationToken cancellationToken)
    {
        var summaries = new List<WindowsLocalTaskGraphSummary>();
        long? beforeTimestamp = null;
        string? beforeId = null;
        do
        {
            var list = await host.SendAsync<ListLocalTaskGraphsCommand, LocalTaskGraphsResult>(new(
                "list_task_graphs", owner, "all",
                turnId is null ? null : "conversation_turn", turnId,
                beforeTimestamp, beforeId, 100), cancellationToken)
                .ConfigureAwait(false);
            if (list.Type != "task_graphs")
                throw new InvalidDataException("Invalid Task Graph list.");
            summaries.AddRange(list.Page.Graphs);
            beforeTimestamp = list.Page.NextBeforeUpdatedAtUnixMs;
            beforeId = list.Page.NextBeforeGraphId;
        } while (beforeTimestamp is not null && summaries.Count < 500);
        var graphs = new List<WindowsLocalTaskGraph>();
        foreach (var summary in summaries)
        {
            var graph = await GetGraphAsync(owner, summary.GraphId, cancellationToken)
                .ConfigureAwait(false);
            if (requiredTaskId is null || graph.Tasks.Any(value => value.TaskId == requiredTaskId))
                graphs.Add(graph);
        }
        return graphs;
    }

    internal async Task<WindowsLocalTaskGraph> GetGraphAsync(
        string owner, string graphId, CancellationToken cancellationToken)
    {
        var result = await host.SendAsync<GetLocalTaskGraphCommand, LocalTaskGraphTypedResult>(
            new("get_task_graph", owner, graphId), cancellationToken).ConfigureAwait(false);
        return result.Type == "task_graph" ? result.Graph
            : throw new InvalidDataException("Invalid Task Graph result.");
    }

    internal async Task<IReadOnlyList<WindowsLocalAgentRun>> RunsAsync(
        string owner, string taskId, CancellationToken cancellationToken)
    {
        var result = await host.SendAsync<GetLocalTaskRunsCommand, LocalTaskRunsResult>(
            new("get_task_runs", owner, taskId, 100), cancellationToken).ConfigureAwait(false);
        return result.Type == "task_runs" && result.TaskId == taskId ? result.Runs
            : throw new InvalidDataException("Invalid Task Runs result.");
    }

    internal async Task<WindowsLocalTaskGraph> RetryAsync(
        string owner, WindowsLocalTask task, string? instruction,
        CancellationToken cancellationToken)
    {
        var result = await host.SendAsync<RetryLocalTaskCommand, LocalTaskGraphTypedResult>(new(
            "retry_task", owner, task.TaskId, task.Version, instruction), cancellationToken)
            .ConfigureAwait(false);
        return result.Type == "task_graph" ? result.Graph
            : throw new InvalidDataException("Invalid Task retry result.");
    }

    internal async Task<WindowsLocalTaskGraph> RestartAsync(
        string owner, WindowsLocalTask task, string reason,
        CancellationToken cancellationToken)
    {
        var result = await host.SendAsync<RestartLocalTaskCommand, LocalTaskGraphTypedResult>(new(
            "restart_task", owner, task.TaskId, task.Version, reason), cancellationToken)
            .ConfigureAwait(false);
        return result.Type == "task_graph" ? result.Graph
            : throw new InvalidDataException("Invalid Task restart result.");
    }

    internal async Task CancelAsync(
        string owner, WindowsLocalTask task, string reason,
        CancellationToken cancellationToken)
    {
        var result = await host.SendAsync<CancelLocalTaskCommand, LocalTaskGraphTypedResult>(new(
            "cancel_task", owner, task.TaskId, task.Version, reason), cancellationToken)
            .ConfigureAwait(false);
        if (result.Type != "task_graph") throw new InvalidDataException("Invalid Task cancel result.");
    }
}
