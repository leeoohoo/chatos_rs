using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Tasks;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class MessageTaskGraphViewModelTests
{
    [Fact]
    public async Task OpensGraphBySourceIdentityAndAllowsCurrentAndHistoricalRuns()
    {
        var service = new LocalTaskServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(new MessageTaskGraphRequest(
            "thread-1", "turn-1", "task-1", "run-2"));

        Assert.Equal(("thread-1", "turn-1"), service.GraphSource);
        Assert.Equal(["run-2", "run-1"], viewModel.Runs.Select(value => value.RunId));
        Assert.Equal("run-2", viewModel.RunDetail?.Run.Run.RunId);

        await viewModel.SelectRunAsync(viewModel.Runs[1]);

        Assert.Equal("run-1", viewModel.RunDetail?.Run.Run.RunId);
        Assert.False(viewModel.CanCancel);
        Assert.False(viewModel.CanRetry);
    }

    [Fact]
    public async Task RetryCreatesNewCurrentRunAndPreservesProjectAndHistory()
    {
        var service = new LocalTaskServiceDouble(currentStatus: LocalAgentRunStatus.Failed);
        using var viewModel = new MessageTaskGraphViewModel(service, service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new MessageTaskGraphRequest(
            "thread-1", "turn-1", "task-1", "run-2"));
        viewModel.RetryInstruction = "use the recovered compiler";

        await viewModel.RetryRunCommand.ExecuteAsync(null);

        Assert.Equal(("task-1", "run-2", "use the recovered compiler"), service.RetryRequest);
        Assert.Equal("project-1", viewModel.SelectedTask?.ProjectId);
        Assert.Equal("run-3", viewModel.SelectedTask?.CurrentRunId);
        Assert.Equal(["run-1", "run-2", "run-3"], viewModel.SelectedTask?.RunIds);
        Assert.Equal("run-3", viewModel.RunDetail?.Run.Run.RunId);
    }

    [Fact]
    public async Task CancelUsesExactCurrentRunAndVersion()
    {
        var service = new LocalTaskServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new MessageTaskGraphRequest(
            "thread-1", "turn-1", "task-1", "run-2"));

        await viewModel.CancelTaskCommand.ExecuteAsync(null);

        Assert.Equal(("task-1", "run-2", (ulong)7), service.CancelRequest);
    }

    [Fact]
    public async Task AccountProjectionClearImmediatelyClosesAndDropsTaskData()
    {
        var service = new LocalTaskServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new MessageTaskGraphRequest(
            "thread-1", "turn-1", "task-1", "run-2"));

        service.ClearAccountProjection();

        Assert.False(viewModel.IsOpen);
        Assert.Empty(viewModel.Nodes);
        Assert.Empty(viewModel.Runs);
        Assert.Null(viewModel.SelectedTask);
        Assert.Null(viewModel.RunDetail);
    }

    [Fact]
    public async Task NeedsReviewTaskShowsReasonAndResumesThroughUnifiedRunControl()
    {
        var service = new LocalTaskServiceDouble(LocalAgentRunStatus.NeedsReview);
        using var viewModel = new MessageTaskGraphViewModel(
            service, service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new MessageTaskGraphRequest(
            "thread-1", "turn-1", "task-1", "run-2"));

        Assert.True(viewModel.CanResume);
        Assert.Contains("无法确认", viewModel.ReviewReason);

        await viewModel.ResumeRunCommand.ExecuteAsync(null);

        Assert.Equal(("run-2", "thread-1", "resume"), service.RunControlRequest);
    }

    private sealed class LocalTaskServiceDouble : ILocalAgentTaskService, ILocalAgentRunControlService
    {
        private readonly DateTimeOffset _now = DateTimeOffset.Parse("2026-09-13T00:00:00Z");
        private LocalAgentTaskSnapshot _task;
        private readonly Dictionary<string, LocalAgentRunSnapshot> _runs;

        public LocalTaskServiceDouble(LocalAgentRunStatus currentStatus = LocalAgentRunStatus.ModelRunning)
        {
            _runs = new Dictionary<string, LocalAgentRunSnapshot>(StringComparer.Ordinal)
            {
                ["run-1"] = Run("run-1", LocalAgentRunStatus.Succeeded, 4),
                ["run-2"] = Run("run-2", currentStatus, 7),
            };
            _task = TaskSnapshot("run-2", ["run-1", "run-2"]);
        }

        public event EventHandler? AccountProjectionCleared;
        public (string ThreadId, string TurnId)? GraphSource { get; private set; }
        public (string TaskId, string RunId, string? Instruction)? RetryRequest { get; private set; }
        public (string TaskId, string RunId, ulong Version)? CancelRequest { get; private set; }
        public (string RunId, string ConversationId, string Action)? RunControlRequest
        {
            get;
            private set;
        }

        public void ClearAccountProjection() => AccountProjectionCleared?.Invoke(this, EventArgs.Empty);

        public Task<LocalAgentTaskGraphSnapshot> GetGraphAsync(
            string sourceThreadId,
            string sourceTurnId,
            CancellationToken cancellationToken = default)
        {
            GraphSource = (sourceThreadId, sourceTurnId);
            var run = _runs[_task.CurrentRunId];
            return Task.FromResult(new LocalAgentTaskGraphSnapshot(
                sourceThreadId,
                sourceTurnId,
                [_task.TaskId],
                [new LocalAgentTaskGraphNode(
                    new LocalAgentTaskProjection(
                        _task,
                        new LocalAgentTaskRunSummary(run, null, null, null)),
                    0,
                    true)],
                []));
        }

        public Task<LocalAgentTaskSnapshot> GetTaskAsync(
            string taskId,
            CancellationToken cancellationToken = default) => Task.FromResult(_task);

        public Task<LocalAgentTaskRunDetail> GetRunDetailAsync(
            string taskId,
            string runId,
            uint eventLimit = 40,
            uint eventOffset = 0,
            CancellationToken cancellationToken = default)
        {
            var run = _runs[runId];
            return Task.FromResult(new LocalAgentTaskRunDetail(
                _task,
                new LocalAgentTaskRunSummary(run, $"result-{runId}", null, null),
                [new LocalAgentRunTimelineEvent($"event-{runId}", "model", runId, _now)],
                1,
                false));
        }

        public Task<LocalAgentRunCreatedResponse> RetryCurrentRunAsync(
            string taskId,
            string expectedRunId,
            string? instruction,
            CancellationToken cancellationToken = default)
        {
            RetryRequest = (taskId, expectedRunId, instruction);
            var run = Run("run-3", LocalAgentRunStatus.Queued, 1);
            _runs.Add(run.RunId, run);
            _task = TaskSnapshot(run.RunId, ["run-1", "run-2", run.RunId]);
            return Task.FromResult(new LocalAgentRunCreatedResponse("operation-3", run));
        }

        public Task CancelCurrentRunAsync(
            string taskId,
            string runId,
            ulong expectedVersion,
            CancellationToken cancellationToken = default)
        {
            CancelRequest = (taskId, runId, expectedVersion);
            return Task.CompletedTask;
        }

        public Task<IReadOnlyList<LocalAgentRunControlState>> FetchRunControlsAsync(
            string conversationId, CancellationToken cancellationToken = default)
        {
            var run = _runs[_task.CurrentRunId];
            var terminal = run.Status is LocalAgentRunStatus.Succeeded
                or LocalAgentRunStatus.Failed or LocalAgentRunStatus.Cancelled;
            IReadOnlyList<LocalAgentRunControlState> result = terminal
                ? []
                : [new LocalAgentRunControlState(run.RunId, run.Version, conversationId,
                    _task.SourceTurnId, run.Status, run.Iteration, run.RetryCount,
                    run.Status == LocalAgentRunStatus.NeedsReview
                        ? "review_unknown_tool_outcome" : null,
                    run.Status == LocalAgentRunStatus.NeedsReview
                        ? "工具执行结果无法确认，请复核后继续。" : null,
                    _now)];
            return Task.FromResult(result);
        }

        public Task PauseRunAsync(string runId, string conversationId,
            CancellationToken cancellationToken = default)
        {
            RunControlRequest = (runId, conversationId, "pause");
            return Task.CompletedTask;
        }
        public Task ResumeRunAsync(string runId, string conversationId,
            CancellationToken cancellationToken = default)
        {
            RunControlRequest = (runId, conversationId, "resume");
            return Task.CompletedTask;
        }
        public Task CancelRunAsync(string runId, string conversationId,
            CancellationToken cancellationToken = default)
        {
            RunControlRequest = (runId, conversationId, "cancel");
            CancelRequest = (_task.TaskId, runId, _runs[runId].Version);
            return Task.CompletedTask;
        }

        private LocalAgentTaskSnapshot TaskSnapshot(string currentRunId, IReadOnlyList<string> runIds) => new(
            "task-1", 1, "thread-1", "turn-1", "project-1", "run-1", currentRunId,
            runIds, "Build Windows", ["Tests pass"], "running", "model-1", 3, _now, _now);

        private LocalAgentRunSnapshot Run(string id, LocalAgentRunStatus status, ulong version) => new(
            id, "task_runner", "user-1", "task", "task-1", "project-1", status,
            version, 0, 0, 0, "model-1", 3, EmptyJson(), "provider_compaction",
            "prompt-1", "capability-1", null, null, null, null, _now, _now);
    }

    private static JsonElement EmptyJson() => JsonDocument.Parse("{}").RootElement.Clone();
}
