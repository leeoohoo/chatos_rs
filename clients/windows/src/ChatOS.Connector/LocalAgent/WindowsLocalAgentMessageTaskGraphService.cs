using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentMessageTaskGraphService : IMessageTaskGraphService
{
    private readonly WindowsLocalAgentTaskClient _tasks;
    private readonly WindowsLocalAgentRuntimeClient _runtime;
    private readonly WindowsLocalAgentControlPlaneClient _controlPlane;
    private readonly object _gate = new();
    private string? _owner;

    public WindowsLocalAgentMessageTaskGraphService(
        WindowsLocalAgentTaskClient tasks,
        WindowsLocalAgentRuntimeClient runtime,
        WindowsLocalAgentControlPlaneClient controlPlane)
    {
        _tasks = tasks;
        _runtime = runtime;
        _controlPlane = controlPlane;
    }

    public void Configure(string ownerUserId) { lock (_gate) _owner = ownerUserId; }
    public void Reset() { lock (_gate) _owner = null; }

    public async Task<MessageTaskGraphSnapshot> FetchGraphAsync(
        string messageId, MessageTaskLookup? lookup,
        CancellationToken cancellationToken = default)
    {
        if (await MessageGraphAsync(lookup, cancellationToken).ConfigureAwait(false) is
            { Nodes.Count: > 0 } graph)
            return await MapMessageGraphAsync(graph, messageId, lookup, cancellationToken)
                .ConfigureAwait(false);
        var graphs = await GraphsAsync(lookup, null, cancellationToken).ConfigureAwait(false);
        var tasks = graphs.SelectMany(value => value.Tasks).ToArray();
        var dependencies = graphs.SelectMany(value => value.Dependencies).ToArray();
        var nonRoots = dependencies.Select(value => value.TaskId).ToHashSet(StringComparer.Ordinal);
        var roots = tasks.Select(value => value.TaskId).Where(value => !nonRoots.Contains(value)).ToArray();
        var depths = Depths(tasks, dependencies);
        var mappedTasks = await Task.WhenAll(tasks.Select(task =>
            MapTaskAsync(task, dependencies, tasks, cancellationToken))).ConfigureAwait(false);
        return new MessageTaskGraphSnapshot(
            roots,
            mappedTasks.Select(task => new MessageTaskGraphNode(
                task, depths.GetValueOrDefault(task.Id),
                roots.Contains(task.Id), true, [])).ToArray(),
            dependencies.Select(value => new MessageTaskGraphEdge(
                $"{value.PrerequisiteTaskId}->{value.TaskId}",
                value.PrerequisiteTaskId, value.TaskId, "prerequisite")).ToArray(),
            lookup?.ConversationId, lookup?.TurnId, lookup?.SourceUserMessageId ?? messageId);
    }

    public async Task<MessageTask> FetchTaskAsync(
        string messageId, string taskId, MessageTaskLookup? lookup,
        CancellationToken cancellationToken = default)
    {
        if (await MessageGraphAsync(lookup, cancellationToken).ConfigureAwait(false) is { } messageGraph &&
            messageGraph.Nodes.FirstOrDefault(value => value.Task.TaskId == taskId) is { } node)
        {
            var messageTasks = messageGraph.Nodes.Select(value => value.Task).ToArray();
            var mappedTask = await MapTaskAsync(
                node.Task, MessageDependencies(messageGraph), messageTasks, cancellationToken)
                .ConfigureAwait(false);
            var messageRuns = await _tasks.RunsAsync(RequireOwner(), taskId, cancellationToken)
                .ConfigureAwait(false);
            return messageRuns.FirstOrDefault() is { } messageRun
                ? Merge(mappedTask, MapRun(messageRun, mappedTask.AcceptanceContract)) : mappedTask;
        }
        var graphs = await GraphsAsync(lookup, taskId, cancellationToken).ConfigureAwait(false);
        var graph = graphs.FirstOrDefault(value => value.Tasks.Any(task => task.TaskId == taskId))
            ?? throw new InvalidOperationException("The local task does not exist.");
        var task = graph.Tasks.First(value => value.TaskId == taskId);
        var mapped = await MapTaskAsync(task, graph.Dependencies, graph.Tasks, cancellationToken)
            .ConfigureAwait(false);
        var runs = await _tasks.RunsAsync(RequireOwner(), taskId, cancellationToken)
            .ConfigureAwait(false);
        return runs.FirstOrDefault() is { } run
            ? Merge(mapped, MapRun(run, mapped.AcceptanceContract)) : mapped;
    }

    public async Task<MessageTaskRunDetail> FetchRunAsync(
        string messageId, string runId, MessageTaskLookup? lookup,
        bool includeEvents = true, int eventLimit = 40, int eventOffset = 0,
        CancellationToken cancellationToken = default)
    {
        var run = await _runtime.GetRunAsync(RequireOwner(), runId, cancellationToken)
            .ConfigureAwait(false);
        var task = await FetchTaskAsync(messageId, run.OwnerEntityId, lookup, cancellationToken)
            .ConfigureAwait(false);
        IReadOnlyList<WindowsLocalAgentEvent> events = includeEvents
            ? await _runtime.ListEventsAsync(RequireOwner(), runId, cancellationToken).ConfigureAwait(false)
            : Array.Empty<WindowsLocalAgentEvent>();
        var offset = Math.Max(0, eventOffset);
        var selected = events.Skip(offset).Take(Math.Clamp(eventLimit, 1, 100)).ToArray();
        return new(task, MapRun(run, task.AcceptanceContract), selected.Select(MapEvent).ToArray(), events.Count,
            events.Count > offset + selected.Length);
    }

    public async Task<MessageTaskRun> RetryRunAsync(
        string messageId, string runId, MessageTaskLookup? lookup, string? instruction,
        CancellationToken cancellationToken = default)
    {
        var owner = RequireOwner();
        var run = await _runtime.GetRunAsync(owner, runId, cancellationToken).ConfigureAwait(false);
        WindowsLocalTask? task = null;
        if (await MessageGraphAsync(lookup, cancellationToken).ConfigureAwait(false) is { } messageGraph)
            task = messageGraph.Nodes.Select(value => value.Task)
                .FirstOrDefault(value => value.TaskId == run.OwnerEntityId);
        if (task is null)
            task = (await GraphsAsync(lookup, run.OwnerEntityId, cancellationToken)
                .ConfigureAwait(false)).SelectMany(value => value.Tasks)
                .FirstOrDefault(value => value.TaskId == run.OwnerEntityId);
        if (task is null) throw new InvalidOperationException("The local task does not exist.");
        var knownRunIds = (await _tasks.RunsAsync(owner, task.TaskId, cancellationToken)
            .ConfigureAwait(false)).Select(value => value.RunId).ToHashSet(StringComparer.Ordinal);
        var graph = await _tasks.RetryAsync(
            owner, task, Normalize(instruction), cancellationToken).ConfigureAwait(false);
        var retried = graph.Tasks.First(value => value.TaskId == task.TaskId);
        if (retried.ActiveRunId is null)
        {
            for (var attempt = 0; attempt < 8; attempt++)
            {
                var runs = await _tasks.RunsAsync(owner, task.TaskId, cancellationToken)
                    .ConfigureAwait(false);
                if (runs.FirstOrDefault(value => !knownRunIds.Contains(value.RunId)) is { } createdRun)
                {
                    return MapRun(createdRun, null);
                }

                await Task.Delay(TimeSpan.FromMilliseconds(50 * (attempt + 1)), cancellationToken)
                    .ConfigureAwait(false);
            }
        }

        return new(
            retried.ActiveRunId ?? $"pending-{retried.TaskId}-{retried.Version}",
            retried.TaskId, retried.Status, retried.Status,
            Date(retried.UpdatedAtUnixMs), null, null, null, null);
    }

    public async Task CancelTaskAsync(
        string messageId, string taskId, MessageTaskLookup? lookup, string? reason,
        CancellationToken cancellationToken = default)
    {
        var task = (await MessageGraphAsync(lookup, cancellationToken).ConfigureAwait(false))?
            .Nodes.Select(value => value.Task)
            .FirstOrDefault(value => value.TaskId == taskId);
        if (task is null)
            task = (await GraphsAsync(lookup, taskId, cancellationToken).ConfigureAwait(false))
                .SelectMany(value => value.Tasks).FirstOrDefault(value => value.TaskId == taskId);
        if (task is null) throw new InvalidOperationException("The local task does not exist.");
        await _tasks.CancelAsync(RequireOwner(), task,
            Normalize(reason) ?? "user requested cancellation", cancellationToken).ConfigureAwait(false);
    }

    private Task<IReadOnlyList<WindowsLocalTaskGraph>> GraphsAsync(
        MessageTaskLookup? lookup, string? taskId, CancellationToken cancellationToken) =>
        _tasks.MatchingGraphsAsync(RequireOwner(), lookup?.TurnId, taskId, cancellationToken);

    private async Task<WindowsLocalMessageTaskGraph?> MessageGraphAsync(
        MessageTaskLookup? lookup, CancellationToken cancellationToken)
    {
        if (lookup is null || string.IsNullOrWhiteSpace(lookup.ConversationId) ||
            string.IsNullOrWhiteSpace(lookup.TurnId)) return null;
        return await _tasks.MessageGraphAsync(
            RequireOwner(), lookup.ConversationId, lookup.TurnId!, lookup.SourceUserMessageId,
            cancellationToken).ConfigureAwait(false);
    }

    private async Task<MessageTaskGraphSnapshot> MapMessageGraphAsync(
        WindowsLocalMessageTaskGraph graph,
        string messageId,
        MessageTaskLookup? lookup,
        CancellationToken cancellationToken)
    {
        var dependencies = MessageDependencies(graph);
        var sourceTasks = graph.Nodes.Select(value => value.Task).ToArray();
        var mappedTasks = await Task.WhenAll(sourceTasks.Select(task =>
            MapTaskAsync(task, dependencies, sourceTasks, cancellationToken))).ConfigureAwait(false);
        var tasksById = mappedTasks.ToDictionary(task => task.Id, StringComparer.Ordinal);
        return new MessageTaskGraphSnapshot(
            graph.RootTaskIds,
            graph.Nodes.Select(node => new MessageTaskGraphNode(
                tasksById[node.Task.TaskId],
                checked((int)node.Depth), node.IsRoot,
                node.IsCurrentMessage, [])).ToArray(),
            graph.Edges.Select(edge => new MessageTaskGraphEdge(
                $"{edge.SourceTaskId}->{edge.TargetTaskId}", edge.SourceTaskId,
                edge.TargetTaskId, edge.Kind)).ToArray(),
            graph.SourceConversationId, graph.SourceTurnId,
            graph.SourceUserMessageId ?? lookup?.SourceUserMessageId ?? messageId);
    }

    private static IReadOnlyList<WindowsLocalTaskDependency> MessageDependencies(
        WindowsLocalMessageTaskGraph graph) => graph.Edges
        .Where(value => value.Kind == "prerequisite")
        .Select(value => new WindowsLocalTaskDependency(value.TargetTaskId, value.SourceTaskId))
        .ToArray();

    private async Task<MessageTask> MapTaskAsync(
        WindowsLocalTask task,
        IReadOnlyList<WindowsLocalTaskDependency> dependencies,
        IReadOnlyList<WindowsLocalTask>? graphTasks,
        CancellationToken cancellationToken)
    {
        var mapped = MapTask(task, dependencies, graphTasks);
        if (string.IsNullOrWhiteSpace(task.ModelConfigRevision)) return mapped;
        try
        {
            var snapshot = await _controlPlane.ModelAsync(
                RequireOwner(), task.ModelConfigRef, task.ModelConfigRevision, cancellationToken)
                .ConfigureAwait(false);
            return mapped with
            {
                DefaultModelConfig = new MessageTaskModelConfigSummary(
                    snapshot.ModelConfigRef, null, snapshot.Provider, snapshot.Model),
                ThinkingLevel = mapped.ThinkingLevel ?? snapshot.ThinkingLevel,
                DefaultModelConfigRevision = snapshot.ModelConfigRevision,
            };
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch
        {
            // Historical Task metadata remains usable even if its exact model snapshot
            // predates this Host or cannot currently be loaded. Never substitute latest.
            return mapped;
        }
    }

    private static MessageTask MapTask(
        WindowsLocalTask task,
        IReadOnlyList<WindowsLocalTaskDependency> dependencies,
        IReadOnlyList<WindowsLocalTask>? graphTasks = null)
    {
        var prerequisiteIds = dependencies.Where(value => value.TaskId == task.TaskId)
            .Select(value => value.PrerequisiteTaskId).Distinct(StringComparer.Ordinal).ToArray();
        var graphTasksById = (graphTasks ?? [])
            .ToDictionary(value => value.TaskId, StringComparer.Ordinal);
        var prerequisiteTasks = prerequisiteIds.Select(id =>
        {
            graphTasksById.TryGetValue(id, out var prerequisite);
            return new MessageTaskReference(id, prerequisite?.Title, prerequisite?.Status);
        }).ToArray();

        return new(
            Id: task.TaskId,
            Title: task.Title,
            Description: String(task.Input, "description"),
            Objective: String(task.Input, "objective"),
            Status: task.Status == "ready" && task.ActiveRunId is not null ? "queued" : task.Status,
            Priority: null,
            Tags: [],
            DefaultModelConfigId: task.ModelConfigRef,
            DefaultModelConfig: null,
            CreatorUserId: null,
            CreatorUsername: null,
            CreatorDisplayName: null,
            ResultSummary: null,
            ProcessLog: null,
            LastRunId: task.ActiveRunId,
            LastRunStatus: null,
            LastRun: null,
            ParentTaskId: null,
            ParentTask: null,
            SourceRunId: null,
            SourceRun: null,
            SourceConversationId: String(task.Input, "source_conversation_id"),
            SourceTurnId: task.SourceEntityType == "conversation_turn" ? task.SourceEntityId
                : String(task.Input, "source_turn_id"),
            SourceUserMessageId: null,
            PrerequisiteTaskIds: prerequisiteIds,
            PrerequisiteTasks: prerequisiteTasks,
            ProjectTaskId: null,
            ExecutionClientRef: String(task.Input, "client_ref"),
            DependencyContextRefs: [],
            ScheduleJson: null,
            TaskToolStateJson: null,
            McpConfigJson: null,
            InputPayloadJson: task.Input.GetRawText(),
            CreatedAt: Date(task.CreatedAtUnixMs),
            UpdatedAt: Date(task.UpdatedAtUnixMs),
            AcceptanceContract: MessageTaskAcceptanceContract.Decode(task.Input),
            RequirementPlan: MessageTaskRequirementPlan.Decode(task.Input, task.Title),
            ThinkingLevel: String(task.Input, "thinking_level"),
            DefaultModelConfigRevision: task.ModelConfigRevision);
    }

    private static MessageTask Merge(MessageTask task, MessageTaskRun run) => task with {
        LastRunId = run.Id,
        LastRunStatus = run.Status,
        LastRun = new MessageTaskLastRunSummary(
            run.Id, run.Status, run.ModelPhaseStatus, run.ResultSummary, run.ReportContent,
            run.ErrorMessage, run.StartedAt, run.FinishedAt, run.AcceptanceReport),
        ResultSummary = run.ResultSummary,
    };

    private static MessageTaskRun MapRun(
        WindowsLocalAgentRun run,
        MessageTaskAcceptanceContract? acceptanceContract) => new(
        run.RunId, run.OwnerEntityId, run.Status, run.Status,
        Date(run.CreatedAtUnixMs), IsTerminal(run.Status) ? Date(run.UpdatedAtUnixMs) : null,
        String(run.TerminalOutcome, "text"), String(run.TerminalOutcome, "report"),
        String(run.TerminalOutcome, "error"),
        MessageTaskAcceptanceReport.Decode(run.TerminalOutcome, acceptanceContract));

    private static MessageTaskRunEvent MapEvent(WindowsLocalAgentEvent value) => new(
        value.EventId, value.EventType,
        value.Payload is { } payload
            ? String(payload, "message") ?? String(payload, "reason")
            : null,
        Date(value.CreatedAtUnixMs));

    private static Dictionary<string, int> Depths(
        IReadOnlyList<WindowsLocalTask> tasks,
        IReadOnlyList<WindowsLocalTaskDependency> dependencies)
    {
        var result = tasks.ToDictionary(value => value.TaskId, _ => 0, StringComparer.Ordinal);
        for (var index = 0; index < tasks.Count; index++)
        {
            var changed = false;
            foreach (var edge in dependencies)
            {
                var depth = result.GetValueOrDefault(edge.PrerequisiteTaskId) + 1;
                if (depth <= result.GetValueOrDefault(edge.TaskId)) continue;
                result[edge.TaskId] = depth; changed = true;
            }
            if (!changed) break;
        }
        return result;
    }

    private string RequireOwner() { lock (_gate) return _owner ?? throw new InvalidOperationException(
        "Local Agent task graph is not configured."); }
    private static string? String(JsonElement? value, string name) => value is { } element &&
        element.ValueKind == JsonValueKind.Object && element.TryGetProperty(name, out var child) &&
        child.ValueKind == JsonValueKind.String ? child.GetString() : null;
    private static DateTimeOffset Date(long value) => DateTimeOffset.FromUnixTimeMilliseconds(value);
    private static bool IsTerminal(string value) => value is "succeeded" or "failed" or "cancelled";
    private static string? Normalize(string? value) => string.IsNullOrWhiteSpace(value) ? null : value.Trim();
}
