using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Tasks;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class MessageTaskGraphViewModelTests
{
    [Fact]
    public async Task OpenSelectsRequestedTaskByStableIdAndLoadsItsRun()
    {
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(new MessageTaskGraphRequest(
            "message-1",
            "task-2",
            "run-2",
            new MessageTaskLookup("conversation-1", "turn-1", "message-1")));

        Assert.True(viewModel.IsOpen);
        Assert.Equal("task-2", viewModel.SelectedTask?.Id);
        Assert.Equal("实现 Windows 客户端", viewModel.SelectedTask?.Title);
        Assert.Equal("blocked", viewModel.SelectedTask?.Status);
        Assert.Equal("run-2", viewModel.RunDetail?.Run.Id);
        Assert.Equal("task-2", viewModel.RunDetail?.Run.TaskId);
    }

    [Fact]
    public async Task LoadMoreEventsAppendsByEventIdentityWithoutRepeatingOlderPage()
    {
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new MessageTaskGraphRequest(
            "message-1",
            "task-2",
            "run-2",
            new MessageTaskLookup("conversation-1", null, null)));

        await viewModel.LoadMoreEventsCommand.ExecuteAsync(null);

        Assert.Equal(new[] { "event-1", "event-2" }, viewModel.RunEvents.Select(static value => value.Id));
        Assert.False(viewModel.EventsHasMore);
        Assert.Equal(2, viewModel.EventsTotal);
    }

    [Fact]
    public async Task CancelUsesSelectedTaskIdentityAndRefreshesAuthoritativeGraph()
    {
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new MessageTaskGraphRequest(
            "message-1",
            "task-2",
            "run-2",
            new MessageTaskLookup("conversation-1", null, null)));
        viewModel.CancelReason = "用户取消";

        await viewModel.CancelTaskCommand.ExecuteAsync(null);

        Assert.Equal(("message-1", "task-2", "用户取消"), service.LastCancellation);
        Assert.True(service.GraphFetchCount >= 2);
    }

    private sealed class TaskGraphServiceDouble : IMessageTaskGraphService
    {
        public int GraphFetchCount { get; private set; }

        public (string MessageId, string TaskId, string? Reason)? LastCancellation { get; private set; }

        public Task<MessageTaskGraphSnapshot> FetchGraphAsync(
            string messageId,
            MessageTaskLookup? lookup,
            CancellationToken cancellationToken = default)
        {
            GraphFetchCount++;
            return Task.FromResult(new MessageTaskGraphSnapshot(
                new[] { "task-2" },
                new[]
                {
                    Node(MakeTask("task-1", "准备环境", "completed", "run-1"), 0),
                    Node(MakeTask("task-2", "实现 Windows 客户端", "blocked", "run-2"), 1),
                },
                new[] { new MessageTaskGraphEdge("edge-1", "task-1", "task-2", "prerequisite") },
                lookup?.ConversationId,
                lookup?.TurnId,
                lookup?.SourceUserMessageId));
        }

        public Task<MessageTask> FetchTaskAsync(
            string messageId,
            string taskId,
            MessageTaskLookup? lookup,
            CancellationToken cancellationToken = default) => Task.FromResult(
            taskId == "task-2"
                ? MakeTask("task-2", "实现 Windows 客户端", "blocked", "run-2")
                : MakeTask("task-1", "准备环境", "completed", "run-1"));

        public Task<MessageTaskRunDetail> FetchRunAsync(
            string messageId,
            string runId,
            MessageTaskLookup? lookup,
            bool includeEvents = true,
            int eventLimit = 40,
            int eventOffset = 0,
            CancellationToken cancellationToken = default)
        {
            var task = MakeTask("task-2", "实现 Windows 客户端", "blocked", runId);
            var events = eventOffset == 0
                ? new[] { new MessageTaskRunEvent("event-1", "thinking", "开始", null) }
                : new[]
                {
                    new MessageTaskRunEvent("event-1", "thinking", "开始", null),
                    new MessageTaskRunEvent("event-2", "tool", "完成", null),
                };
            return Task.FromResult(new MessageTaskRunDetail(
                task,
                new MessageTaskRun(runId, task.Id, "failed", null, null, null, null, null, "blocked"),
                events,
                2,
                eventOffset == 0));
        }

        public Task<MessageTaskRun> RetryRunAsync(
            string messageId,
            string runId,
            MessageTaskLookup? lookup,
            string? instruction,
            CancellationToken cancellationToken = default) => Task.FromResult(
            new MessageTaskRun("run-new", "task-2", "queued", null, null, null, null, null, null));

        public Task CancelTaskAsync(
            string messageId,
            string taskId,
            MessageTaskLookup? lookup,
            string? reason,
            CancellationToken cancellationToken = default)
        {
            LastCancellation = (messageId, taskId, reason);
            return Task.CompletedTask;
        }

        private static MessageTaskGraphNode Node(MessageTask task, int depth) => new(
            task,
            depth,
            depth == 1,
            true,
            Array.Empty<MessageTask>());

        private static MessageTask MakeTask(string id, string title, string status, string runId) => new(
            Id: id,
            Title: title,
            Description: null,
            Objective: null,
            Status: status,
            Priority: null,
            Tags: Array.Empty<string>(),
            DefaultModelConfigId: null,
            DefaultModelConfig: null,
            CreatorUserId: null,
            CreatorUsername: null,
            CreatorDisplayName: null,
            ResultSummary: null,
            ProcessLog: null,
            LastRunId: runId,
            LastRunStatus: status,
            LastRun: new MessageTaskLastRunSummary(runId, status, null, null, null, null, null, null),
            ParentTaskId: null,
            ParentTask: null,
            SourceRunId: null,
            SourceRun: null,
            SourceConversationId: "conversation-1",
            SourceTurnId: null,
            SourceUserMessageId: null,
            PrerequisiteTaskIds: Array.Empty<string>(),
            PrerequisiteTasks: Array.Empty<MessageTaskReference>(),
            ProjectTaskId: null,
            ExecutionClientRef: null,
            DependencyContextRefs: Array.Empty<string>(),
            ScheduleJson: null,
            TaskToolStateJson: null,
            McpConfigJson: null,
            InputPayloadJson: null,
            CreatedAt: null,
            UpdatedAt: null);
    }
}
