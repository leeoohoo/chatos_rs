using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentMessageTaskGraphService : IMessageTaskGraphService
{
    private readonly WindowsLocalAgentTaskClient _tasks;
    private readonly WindowsLocalAgentRuntimeClient _runtime;
    private readonly object _gate = new();
    private string? _owner;

    public WindowsLocalAgentMessageTaskGraphService(
        WindowsLocalAgentTaskClient tasks, WindowsLocalAgentRuntimeClient runtime)
    {
        _tasks = tasks; _runtime = runtime;
    }

    public void Configure(string ownerUserId) { lock (_gate) _owner = ownerUserId; }
    public void Reset() { lock (_gate) _owner = null; }

    public async Task<MessageTaskGraphSnapshot> FetchGraphAsync(
        string messageId, MessageTaskLookup? lookup,
        CancellationToken cancellationToken = default)
    {
        if (await MessageGraphAsync(lookup, cancellationToken).ConfigureAwait(false) is { } graph)
            return MapMessageGraph(graph, messageId, lookup);
        var graphs = await GraphsAsync(lookup, null, cancellationToken).ConfigureAwait(false);
        var tasks = graphs.SelectMany(value => value.Tasks).ToArray();
        var dependencies = graphs.SelectMany(value => value.Dependencies).ToArray();
        var nonRoots = dependencies.Select(value => value.TaskId).ToHashSet(StringComparer.Ordinal);
        var roots = tasks.Select(value => value.TaskId).Where(value => !nonRoots.Contains(value)).ToArray();
        var depths = Depths(tasks, dependencies);
        return new MessageTaskGraphSnapshot(
            roots,
            tasks.Select(task => new MessageTaskGraphNode(
                MapTask(task, dependencies), depths.GetValueOrDefault(task.TaskId),
                roots.Contains(task.TaskId), true, [])).ToArray(),
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
            var mappedTask = MapTask(node.Task, MessageDependencies(messageGraph));
            var messageRuns = await _tasks.RunsAsync(RequireOwner(), taskId, cancellationToken)
                .ConfigureAwait(false);
            return messageRuns.FirstOrDefault() is { } messageRun
                ? Merge(mappedTask, MapRun(messageRun)) : mappedTask;
        }
        var graphs = await GraphsAsync(lookup, taskId, cancellationToken).ConfigureAwait(false);
        var graph = graphs.FirstOrDefault(value => value.Tasks.Any(task => task.TaskId == taskId))
            ?? throw new InvalidOperationException("The local task does not exist.");
        var task = graph.Tasks.First(value => value.TaskId == taskId);
        var mapped = MapTask(task, graph.Dependencies);
        var runs = await _tasks.RunsAsync(RequireOwner(), taskId, cancellationToken)
            .ConfigureAwait(false);
        return runs.FirstOrDefault() is { } run ? Merge(mapped, MapRun(run)) : mapped;
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
        return new(task, MapRun(run), selected.Select(MapEvent).ToArray(), events.Count,
            events.Count > offset + selected.Length);
    }

    public async Task<MessageTaskRun> RetryRunAsync(
        string messageId, string runId, MessageTaskLookup? lookup, string? instruction,
        CancellationToken cancellationToken = default)
    {
        var owner = RequireOwner();
        var run = await _runtime.GetRunAsync(owner, runId, cancellationToken).ConfigureAwait(false);
        WindowsLocalTask? task;
        if (await MessageGraphAsync(lookup, cancellationToken).ConfigureAwait(false) is { } messageGraph)
            task = messageGraph.Nodes.Select(value => value.Task)
                .FirstOrDefault(value => value.TaskId == run.OwnerEntityId);
        else
            task = (await GraphsAsync(lookup, run.OwnerEntityId, cancellationToken)
                .ConfigureAwait(false)).SelectMany(value => value.Tasks)
                .FirstOrDefault(value => value.TaskId == run.OwnerEntityId);
        if (task is null) throw new InvalidOperationException("The local task does not exist.");
        var graph = await _tasks.RetryAsync(
            owner, task, Normalize(instruction), cancellationToken).ConfigureAwait(false);
        var retried = graph.Tasks.First(value => value.TaskId == task.TaskId);
        return new(
            retried.ActiveRunId ?? $"pending-{retried.TaskId}-{retried.Version}",
            retried.TaskId, retried.Status, retried.Status,
            Date(retried.UpdatedAtUnixMs), null, null, null, null);
    }

    public async Task CancelTaskAsync(
        string messageId, string taskId, MessageTaskLookup? lookup, string? reason,
        CancellationToken cancellationToken = default)
    {
        WindowsLocalTask? task;
        if (await MessageGraphAsync(lookup, cancellationToken).ConfigureAwait(false) is { } messageGraph)
            task = messageGraph.Nodes.Select(value => value.Task)
                .FirstOrDefault(value => value.TaskId == taskId);
        else
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

    private static MessageTaskGraphSnapshot MapMessageGraph(
        WindowsLocalMessageTaskGraph graph, string messageId, MessageTaskLookup? lookup)
    {
        var dependencies = MessageDependencies(graph);
        return new MessageTaskGraphSnapshot(
            graph.RootTaskIds,
            graph.Nodes.Select(node => new MessageTaskGraphNode(
                MapTask(node.Task, dependencies), checked((int)node.Depth), node.IsRoot,
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

    private static MessageTask MapTask(
        WindowsLocalTask task, IReadOnlyList<WindowsLocalTaskDependency> dependencies) => new(
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
            PrerequisiteTaskIds: dependencies.Where(value => value.TaskId == task.TaskId)
                .Select(value => value.PrerequisiteTaskId).ToArray(),
            PrerequisiteTasks: [],
            ProjectTaskId: null,
            ExecutionClientRef: String(task.Input, "client_ref"),
            DependencyContextRefs: [],
            ScheduleJson: null,
            TaskToolStateJson: null,
            McpConfigJson: null,
            InputPayloadJson: task.Input.GetRawText(),
            CreatedAt: Date(task.CreatedAtUnixMs),
            UpdatedAt: Date(task.UpdatedAtUnixMs));

    private static MessageTask Merge(MessageTask task, MessageTaskRun run) => task with {
        LastRunId = run.Id,
        LastRunStatus = run.Status,
        LastRun = new MessageTaskLastRunSummary(
            run.Id, run.Status, run.ModelPhaseStatus, run.ResultSummary, run.ReportContent,
            run.ErrorMessage, run.StartedAt, run.FinishedAt),
        ResultSummary = run.ResultSummary,
    };

    private static MessageTaskRun MapRun(WindowsLocalAgentRun run) => new(
        run.RunId, run.OwnerEntityId, run.Status, run.Status,
        Date(run.CreatedAtUnixMs), IsTerminal(run.Status) ? Date(run.UpdatedAtUnixMs) : null,
        String(run.TerminalOutcome, "text"), String(run.TerminalOutcome, "report"),
        String(run.TerminalOutcome, "error"));

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
