using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

/// <summary>
/// Windows IPC adapter for the shared Local Agent task runtime. It resolves the
/// active account from the atomically restored projection on every operation and
/// asks the account session for the current Named Pipe client on every operation.
/// </summary>
public sealed class WindowsLocalAgentTaskService : ILocalAgentTaskService, IDisposable
{
    private readonly IWindowsLocalAgentProjectionStore _store;
    private readonly IWindowsLocalAgentAccountSession _accountSession;

    public WindowsLocalAgentTaskService(
        IWindowsLocalAgentProjectionStore store,
        IWindowsLocalAgentAccountSession accountSession)
    {
        _store = store;
        _accountSession = accountSession;
        _store.Cleared += OnProjectionCleared;
    }

    public event EventHandler? AccountProjectionCleared;

    public async Task<LocalAgentTaskGraphSnapshot> GetGraphAsync(
        string sourceThreadId,
        string sourceTurnId,
        CancellationToken cancellationToken = default)
    {
        RequireIdentifier(sourceThreadId, nameof(sourceThreadId));
        RequireIdentifier(sourceTurnId, nameof(sourceTurnId));
        var context = await ResolveContextAsync(cancellationToken).ConfigureAwait(false);
        var graph = await context.Client.GetTaskGraphAsync(
            sourceThreadId,
            sourceTurnId,
            cancellationToken).ConfigureAwait(false);
        ValidateGraph(context, graph, sourceThreadId, sourceTurnId);
        return graph;
    }

    public async Task<LocalAgentTaskSnapshot> GetTaskAsync(
        string taskId,
        CancellationToken cancellationToken = default)
    {
        RequireIdentifier(taskId, nameof(taskId));
        var context = await ResolveContextAsync(cancellationToken).ConfigureAwait(false);
        var task = await context.Client.GetTaskAsync(taskId, cancellationToken).ConfigureAwait(false);
        ValidateTask(context, task, taskId);
        return task;
    }

    public async Task<LocalAgentTaskRunDetail> GetRunDetailAsync(
        string taskId,
        string runId,
        uint eventLimit = 40,
        uint eventOffset = 0,
        CancellationToken cancellationToken = default)
    {
        RequireIdentifier(taskId, nameof(taskId));
        RequireIdentifier(runId, nameof(runId));
        var context = await ResolveContextAsync(cancellationToken).ConfigureAwait(false);
        var detail = await context.Client.GetTaskRunDetailAsync(
            taskId,
            runId,
            eventLimit,
            eventOffset,
            cancellationToken).ConfigureAwait(false);
        ValidateDetail(context, detail, taskId, runId);
        return detail;
    }

    public async Task<LocalAgentRunCreatedResponse> RetryCurrentRunAsync(
        string taskId,
        string expectedRunId,
        string? instruction,
        CancellationToken cancellationToken = default)
    {
        RequireIdentifier(taskId, nameof(taskId));
        RequireIdentifier(expectedRunId, nameof(expectedRunId));
        var context = await ResolveContextAsync(cancellationToken).ConfigureAwait(false);
        var before = await context.Client.GetTaskRunDetailAsync(
            taskId,
            expectedRunId,
            1,
            0,
            cancellationToken).ConfigureAwait(false);
        ValidateDetail(context, before, taskId, expectedRunId);
        if (!string.Equals(before.Task.CurrentRunId, expectedRunId, StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Only the current Local Agent task run can be retried.");
        }

        var response = await context.Client.RetryTaskAsync(
            new LocalAgentRetryTask(
                taskId,
                expectedRunId,
                string.IsNullOrWhiteSpace(instruction) ? null : instruction.Trim()),
            cancellationToken).ConfigureAwait(false);
        ValidateRetry(context.AccountId, before, response);

        var after = await context.Client.GetTaskAsync(taskId, cancellationToken).ConfigureAwait(false);
        ValidateRetriedTask(before.Task, after, expectedRunId, response.Run.RunId);
        return response;
    }

    public void Dispose() => _store.Cleared -= OnProjectionCleared;

    private async Task<OperationContext> ResolveContextAsync(CancellationToken cancellationToken)
    {
        var projection = await _store.GetAsync(cancellationToken).ConfigureAwait(false)
            ?? throw new InvalidOperationException("The Local Agent account projection is not available.");
        RequireIdentifier(projection.AccountId, nameof(projection.AccountId));
        var client = await _accountSession.GetClientAsync(projection.AccountId, cancellationToken)
            .ConfigureAwait(false);
        return new OperationContext(projection.AccountId, projection, client);
    }

    private static void ValidateGraph(
        OperationContext context,
        LocalAgentTaskGraphSnapshot graph,
        string sourceThreadId,
        string sourceTurnId)
    {
        if (!string.Equals(graph.SourceThreadId, sourceThreadId, StringComparison.Ordinal)
            || !string.Equals(graph.SourceTurnId, sourceTurnId, StringComparison.Ordinal))
        {
            throw new InvalidDataException("The Local Agent task graph source identity changed.");
        }

        var taskIds = new HashSet<string>(StringComparer.Ordinal);
        foreach (var node in graph.Nodes)
        {
            if (!taskIds.Add(node.Task.Task.TaskId))
            {
                throw new InvalidDataException("The Local Agent task graph contains duplicate task identities.");
            }
            if (!string.Equals(node.Task.Task.SourceThreadId, sourceThreadId, StringComparison.Ordinal)
                || !string.Equals(node.Task.Task.SourceTurnId, sourceTurnId, StringComparison.Ordinal))
            {
                throw new InvalidDataException("The Local Agent task graph contains a task from another source turn.");
            }
            ValidateTask(context, node.Task.Task, node.Task.Task.TaskId);
            if (!string.Equals(
                    node.Task.CurrentRun.Run.RunId,
                    node.Task.Task.CurrentRunId,
                    StringComparison.Ordinal))
            {
                throw new InvalidDataException("The Local Agent task graph current run is inconsistent.");
            }
            ValidateRun(context.AccountId, node.Task.Task, node.Task.CurrentRun.Run);
        }
        if (graph.RootTaskIds.Distinct(StringComparer.Ordinal).Count() != graph.RootTaskIds.Count
            || graph.RootTaskIds.Any(id => !taskIds.Contains(id)))
        {
            throw new InvalidDataException("The Local Agent task graph references a missing root task.");
        }
        var edgeIds = new HashSet<string>(StringComparer.Ordinal);
        foreach (var edge in graph.Edges)
        {
            if (!edgeIds.Add(edge.EdgeId)
                || !taskIds.Contains(edge.SourceTaskId)
                || !taskIds.Contains(edge.TargetTaskId))
            {
                throw new InvalidDataException("The Local Agent task graph contains an invalid edge.");
            }
        }
    }

    private static void ValidateTask(
        OperationContext context,
        LocalAgentTaskSnapshot task,
        string expectedTaskId)
    {
        if (!string.Equals(task.TaskId, expectedTaskId, StringComparison.Ordinal))
        {
            throw new InvalidDataException("The Local Agent returned a different task identity.");
        }
        ValidateTaskShape(task);
        if (context.Projection.Tasks.TryGetValue(task.TaskId, out var restored))
        {
            RequireFrozenTaskIdentity(restored, task);
        }
    }

    private static void ValidateDetail(
        OperationContext context,
        LocalAgentTaskRunDetail detail,
        string taskId,
        string runId)
    {
        ValidateTask(context, detail.Task, taskId);
        if (!detail.Task.RunIds.Contains(runId, StringComparer.Ordinal)
            || !string.Equals(detail.Run.Run.RunId, runId, StringComparison.Ordinal))
        {
            throw new InvalidDataException("The Local Agent task run identity changed.");
        }
        ValidateRun(context.AccountId, detail.Task, detail.Run.Run);
        if (detail.Events.Select(value => value.EventId).Distinct(StringComparer.Ordinal).Count()
            != detail.Events.Count)
        {
            throw new InvalidDataException("The Local Agent task run contains duplicate event identities.");
        }
    }

    private static void ValidateRun(
        string accountId,
        LocalAgentTaskSnapshot task,
        LocalAgentRunSnapshot run)
    {
        if (!string.Equals(run.OwnerUserId, accountId, StringComparison.Ordinal)
            || !string.Equals(run.OwnerEntityType, "task", StringComparison.Ordinal)
            || !string.Equals(run.OwnerEntityId, task.TaskId, StringComparison.Ordinal)
            || !string.Equals(run.ProjectId, task.ProjectId, StringComparison.Ordinal)
            || !string.Equals(run.ModelConfigId, task.ModelConfigId, StringComparison.Ordinal)
            || run.ModelConfigRevision != task.ModelConfigRevision)
        {
            throw new InvalidDataException("The Local Agent task run did not preserve its frozen identity.");
        }
    }

    private static void ValidateRetry(
        string accountId,
        LocalAgentTaskRunDetail before,
        LocalAgentRunCreatedResponse response)
    {
        var previous = before.Run.Run;
        var next = response.Run;
        if (string.IsNullOrWhiteSpace(response.OperationId)
            || string.Equals(next.RunId, previous.RunId, StringComparison.Ordinal)
            || !string.Equals(next.OwnerUserId, accountId, StringComparison.Ordinal)
            || !string.Equals(next.OwnerEntityType, "task", StringComparison.Ordinal)
            || !string.Equals(next.OwnerEntityId, before.Task.TaskId, StringComparison.Ordinal)
            || !string.Equals(next.ProjectId, previous.ProjectId, StringComparison.Ordinal)
            || !string.Equals(next.ModelConfigId, previous.ModelConfigId, StringComparison.Ordinal)
            || next.ModelConfigRevision != previous.ModelConfigRevision
            || !string.Equals(next.PromptRevision, previous.PromptRevision, StringComparison.Ordinal)
            || !string.Equals(next.CapabilitySnapshotRef, previous.CapabilitySnapshotRef, StringComparison.Ordinal)
            || !string.Equals(next.ContextStrategy, previous.ContextStrategy, StringComparison.Ordinal)
            || !string.Equals(
                next.ModelRuntimeSnapshot.GetRawText(),
                previous.ModelRuntimeSnapshot.GetRawText(),
                StringComparison.Ordinal))
        {
            throw new InvalidDataException("The retried Local Agent run changed a frozen execution snapshot.");
        }
    }

    private static void ValidateRetriedTask(
        LocalAgentTaskSnapshot before,
        LocalAgentTaskSnapshot after,
        string previousRunId,
        string newRunId)
    {
        RequireFrozenTaskIdentity(before, after);
        if (after.Revision <= before.Revision
            || !string.Equals(after.InitialRunId, before.InitialRunId, StringComparison.Ordinal)
            || !string.Equals(after.CurrentRunId, newRunId, StringComparison.Ordinal)
            || !string.Equals(before.CurrentRunId, previousRunId, StringComparison.Ordinal)
            || after.RunIds.Count != before.RunIds.Count + 1
            || !after.RunIds.Take(before.RunIds.Count).SequenceEqual(
                before.RunIds,
                StringComparer.Ordinal)
            || !string.Equals(after.RunIds[^1], newRunId, StringComparison.Ordinal))
        {
            throw new InvalidDataException("The retried Local Agent task did not preserve its run history.");
        }
    }

    private static void RequireFrozenTaskIdentity(
        LocalAgentTaskSnapshot expected,
        LocalAgentTaskSnapshot actual)
    {
        if (!string.Equals(actual.TaskId, expected.TaskId, StringComparison.Ordinal)
            || !string.Equals(actual.SourceThreadId, expected.SourceThreadId, StringComparison.Ordinal)
            || !string.Equals(actual.SourceTurnId, expected.SourceTurnId, StringComparison.Ordinal)
            || !string.Equals(actual.ProjectId, expected.ProjectId, StringComparison.Ordinal)
            || !string.Equals(actual.Objective, expected.Objective, StringComparison.Ordinal)
            || !actual.AcceptanceCriteria.SequenceEqual(expected.AcceptanceCriteria, StringComparer.Ordinal)
            || !string.Equals(actual.ModelConfigId, expected.ModelConfigId, StringComparison.Ordinal)
            || actual.ModelConfigRevision != expected.ModelConfigRevision)
        {
            throw new InvalidDataException("The Local Agent task changed a frozen task field.");
        }
    }

    private static void ValidateTaskShape(LocalAgentTaskSnapshot task)
    {
        RequireIdentifier(task.TaskId, nameof(task.TaskId));
        RequireIdentifier(task.SourceThreadId, nameof(task.SourceThreadId));
        RequireIdentifier(task.SourceTurnId, nameof(task.SourceTurnId));
        RequireIdentifier(task.ProjectId, nameof(task.ProjectId));
        if (!task.RunIds.Contains(task.InitialRunId, StringComparer.Ordinal)
            || !task.RunIds.Contains(task.CurrentRunId, StringComparer.Ordinal)
            || task.RunIds.Distinct(StringComparer.Ordinal).Count() != task.RunIds.Count)
        {
            throw new InvalidDataException("The Local Agent task run history is invalid.");
        }
    }

    private static bool IsTerminal(LocalAgentRunStatus status) => status is
        LocalAgentRunStatus.Succeeded or
        LocalAgentRunStatus.Failed or
        LocalAgentRunStatus.Cancelled;

    private static void RequireIdentifier(string value, string name)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            throw new ArgumentException("A Local Agent identity cannot be empty.", name);
        }
    }

    private void OnProjectionCleared(object? sender, EventArgs e) =>
        AccountProjectionCleared?.Invoke(this, EventArgs.Empty);

    private sealed record OperationContext(
        string AccountId,
        WindowsLocalAgentProjectionSnapshot Projection,
        ILocalAgentIPCClient Client);
}
