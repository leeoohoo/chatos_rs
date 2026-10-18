using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Tasks;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed partial class MessageTaskGraphViewModelTests
{
    [Fact]
    public void DependentNodeExposesRelationshipAndAccessibleSummary()
    {
        var prerequisite = new MessageTaskReference("task-a", "Prepare inputs", "completed");
        var task = TaskGraphServiceDouble.MakeTask("task-b", "Use inputs", "pending", string.Empty) with
        {
            LastRunId = null,
            LastRun = null,
            PrerequisiteTaskIds = ["task-a"],
            PrerequisiteTasks = [prerequisite],
        };
        var item = new MessageTaskGraphNodeItemViewModel(
            new MessageTaskGraphNode(task, 1, false, true, []));

        Assert.True(item.HasPrerequisites);
        Assert.False(item.HasRunIdentity);
        Assert.Equal(string.Empty, item.RunIdentity);
        Assert.Equal("← Prepare inputs", item.DependencySummary);
        Assert.Contains("Use inputs", item.AccessibilityName, StringComparison.Ordinal);
        Assert.Contains("Prepare inputs", item.AccessibilityName, StringComparison.Ordinal);
    }

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
    public async Task AcceptedRunExposesBoundEvidenceAndUnicodeCharacterLimit()
    {
        var contract = new MessageTaskAcceptanceContract(1, ["Inspect the artifact"], 20);
        var report = new MessageTaskAcceptanceReport(
            "succeeded", "checked", "完成😀", 3,
            [new MessageTaskAcceptanceCheck(0, "Inspect the artifact", "passed", "Opened the saved artifact")]);
        var service = new TaskGraphServiceDouble(taskStatus: "succeeded", runStatus: "succeeded");
        service.TaskHandler = (_, _, _) => Task.FromResult(
            TaskGraphServiceDouble.MakeTask("task-2", "验收", "succeeded", "run-2") with
            {
                AcceptanceContract = contract,
            });
        service.RunHandler = async (_, runId, _, _) =>
        {
            var page = await service.RunPage(runId, 0);
            return page with
            {
                Task = page.Task with { AcceptanceContract = contract },
                Run = page.Run with { Status = "succeeded", AcceptanceReport = report },
            };
        };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(Request());

        Assert.True(viewModel.HasAcceptanceReport);
        Assert.False(viewModel.ShowsMissingAcceptanceReport);
        Assert.Same(report, viewModel.AcceptanceReport);
        Assert.Equal("3 / 20", viewModel.AcceptanceCharacterCountLabel);
    }

    [Fact]
    public async Task SucceededContractRunWithoutValidReportFailsClosedInPresentation()
    {
        var contract = new MessageTaskAcceptanceContract(1, ["Inspect the artifact"], null);
        var service = new TaskGraphServiceDouble(taskStatus: "succeeded", runStatus: "succeeded");
        service.TaskHandler = (_, _, _) => Task.FromResult(
            TaskGraphServiceDouble.MakeTask("task-2", "验收", "succeeded", "run-2") with
            {
                AcceptanceContract = contract,
            });
        service.RunHandler = async (_, runId, _, _) =>
        {
            var page = await service.RunPage(runId, 0);
            return page with
            {
                Task = page.Task with { AcceptanceContract = contract },
                Run = page.Run with { Status = "succeeded", AcceptanceReport = null },
            };
        };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(Request());

        Assert.False(viewModel.HasAcceptanceReport);
        Assert.True(viewModel.ShowsMissingAcceptanceReport);
        Assert.Equal(string.Empty, viewModel.AcceptanceCharacterCountLabel);
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

        Assert.Equal("1 / 2", viewModel.EventsCountLabel);

        await viewModel.LoadMoreEventsCommand.ExecuteAsync(null);

        Assert.Equal(new[] { "event-1", "event-2" }, viewModel.RunEvents.Select(static value => value.Id));
        Assert.False(viewModel.EventsHasMore);
        Assert.Equal(2, viewModel.EventsTotal);
        Assert.Equal("2 / 2", viewModel.EventsCountLabel);
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

    [Fact]
    public async Task RetryUsesLoadedRunIdentityAndInstructionThenRefreshesAuthoritativeGraph()
    {
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(new MessageTaskGraphRequest(
            "message-1",
            "task-2",
            "run-2",
            new MessageTaskLookup("conversation-1", "turn-1", "message-1")));
        viewModel.RetryInstruction = "再执行一次";

        await viewModel.RetryRunCommand.ExecuteAsync(null);

        Assert.Equal(("message-1", "run-2", "再执行一次"), service.LastRetry);
        Assert.Equal("run-new", viewModel.RequestedRunId);
        Assert.Equal(string.Empty, viewModel.RetryInstruction);
        Assert.True(service.GraphFetchCount >= 2);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task SucceededTaskDoesNotOfferRetryEvenWhenItsRunIsTerminal()
    {
        var service = new TaskGraphServiceDouble(taskStatus: "succeeded", runStatus: "succeeded");
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync(new MessageTaskGraphRequest(
            "message-1",
            "task-2",
            "run-2",
            new MessageTaskLookup("conversation-1", "turn-1", "message-1")));

        Assert.False(viewModel.CanRetry);
    }

    [Fact]
    public async Task InlineInspectorLoadsTheLatestTaskRunWithoutOpeningTheGraphPanel()
    {
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());

        var snapshot = await viewModel.LoadInspectorAsync(new MessageTaskGraphRequest(
            "message-1",
            "task-2",
            "run-old",
            new MessageTaskLookup("conversation-1", "turn-1", "message-1")));

        Assert.False(viewModel.IsOpen);
        Assert.Equal("task-2", snapshot.Task.Id);
        Assert.Equal("run-2", snapshot.Run?.Run.Id);
        Assert.Single(snapshot.Run!.Events);
    }

    [Fact]
    public async Task CancelledInlineInspectorDoesNotStartAnyFetch()
    {
        var service = new TaskGraphServiceDouble();
        var taskFetches = 0;
        service.TaskHandler = async (_, _, _) =>
        {
            taskFetches++;
            return (await service.RunPage("run-2", 0)).Task;
        };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => viewModel.LoadInspectorAsync(Request(), cancellation.Token));

        Assert.Equal(0, taskFetches);
        Assert.Equal(0, service.RunFetchCount);
    }

    [Fact]
    public async Task InlineInspectorCancelledDuringTaskFetchDoesNotFetchTheRun()
    {
        var service = new TaskGraphServiceDouble();
        var pending = new TaskCompletionSource<MessageTask>(TaskCreationOptions.RunContinuationsAsynchronously);
        service.TaskHandler = (_, _, _) => pending.Task;
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        using var cancellation = new CancellationTokenSource();
        var loading = viewModel.LoadInspectorAsync(Request(), cancellation.Token);
        cancellation.Cancel();
        pending.SetResult((await service.RunPage("run-2", 0)).Task);

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => loading);

        Assert.Equal(0, service.RunFetchCount);
    }

    [Fact]
    public async Task InlineInspectorCancelledDuringRunFetchDoesNotReturnTheLateSnapshot()
    {
        var service = new TaskGraphServiceDouble();
        var pending = new TaskCompletionSource<MessageTaskRunDetail>(TaskCreationOptions.RunContinuationsAsynchronously);
        var entered = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        service.RunHandler = (_, _, _, _) =>
        {
            entered.SetResult();
            return pending.Task;
        };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        using var cancellation = new CancellationTokenSource();
        var loading = viewModel.LoadInspectorAsync(Request(), cancellation.Token);
        await entered.Task.WaitAsync(TimeSpan.FromSeconds(2));
        cancellation.Cancel();
        pending.SetResult(await service.RunPage("run-2", 0));

        await Assert.ThrowsAnyAsync<OperationCanceledException>(() => loading);
        Assert.Equal(1, service.RunFetchCount);
    }

    [Fact]
    public async Task FailedNodeSelectionClearsOldDetailsAndReportsErrorWithoutThrowing()
    {
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        service.TaskHandler = (_, _, _) => throw new IOException("node unavailable");

        var error = await Record.ExceptionAsync(() => viewModel.SelectNodeCommand.ExecuteAsync(viewModel.Nodes[0]));

        Assert.Null(error);
        Assert.Null(viewModel.SelectedTask);
        Assert.Null(viewModel.RunDetail);
        Assert.Empty(viewModel.RunEvents);
        Assert.False(viewModel.CanCancel);
        Assert.False(viewModel.CanRetry);
        Assert.False(viewModel.IsLoading);
        Assert.Equal("node unavailable", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task RefreshPreservesSelectedNodeAndItsRun()
    {
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        await viewModel.SelectNodeCommand.ExecuteAsync(viewModel.Nodes[0]);

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Equal("task-1", viewModel.SelectedNode?.Id);
        Assert.Equal("task-1", viewModel.SelectedTask?.Id);
        Assert.Equal("run-1", viewModel.RunDetail?.Run.Id);
    }

    [Fact]
    public async Task OldGraphErrorCannotAppearInReopenedPanel()
    {
        var pending = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new TaskGraphServiceDouble { GraphHandler = (_, _) => pending.Task };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        var first = viewModel.OpenAsync(Request());
        service.GraphHandler = null;
        await viewModel.OpenAsync(Request("message-2"));

        pending.SetException(new IOException("old graph failed"));
        await first;

        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal("message-2", viewModel.MessageId);
        Assert.False(viewModel.IsLoading);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ClosingDuringLoadEndsBusyStateAndIgnoresLateFailure(bool dispose)
    {
        var pending = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new TaskGraphServiceDouble { GraphHandler = (_, _) => pending.Task };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        var opening = viewModel.OpenAsync(Request());

        if (dispose) viewModel.Dispose();
        else viewModel.CloseCommand.Execute(null);

        Assert.False(viewModel.IsLoading);
        pending.SetException(new IOException("closed graph failed"));
        await opening;
        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.IsOpen);
        Assert.False(viewModel.CanCancel);
        Assert.False(viewModel.CanRetry);
    }

    [Fact]
    public async Task OldEventPageFailureCannotAffectAnotherSelectedTask()
    {
        var pending = new TaskCompletionSource<MessageTaskRunDetail>(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        service.RunHandler = (_, _, offset, _) => offset > 0 ? pending.Task : service.RunPage("run-1", 0);
        var paging = viewModel.LoadMoreEventsCommand.ExecuteAsync(null);
        await viewModel.SelectNodeCommand.ExecuteAsync(viewModel.Nodes[0]);

        pending.SetException(new IOException("old page failed"));
        await paging;

        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal("task-1", viewModel.SelectedTask?.Id);
        Assert.False(viewModel.IsLoadingMoreEvents);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldActionFailureCannotAffectReopenedPanel(bool retry)
    {
        var pending = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new TaskGraphServiceDouble
        {
            CancelHandler = async _ => await pending.Task,
            RetryHandler = async _ => { await pending.Task; return NewRun(); },
        };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        var action = retry ? viewModel.RetryRunCommand.ExecuteAsync(null) : viewModel.CancelTaskCommand.ExecuteAsync(null);
        await viewModel.OpenAsync(Request("message-2"));

        pending.SetException(new IOException("old action failed"));
        await action;

        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.IsApplyingAction);
        Assert.Equal("message-2", viewModel.MessageId);
    }

    [Fact]
    public async Task OldRetrySuccessCannotRefreshNewSelectionOrClearItsDraft()
    {
        var pending = new TaskCompletionSource<MessageTaskRun>(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new TaskGraphServiceDouble { RetryHandler = _ => pending.Task };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        viewModel.RetryInstruction = "original instruction";
        var retry = viewModel.RetryRunCommand.ExecuteAsync(null);
        await viewModel.SelectNodeCommand.ExecuteAsync(viewModel.Nodes[0]);
        viewModel.RetryInstruction = "new task draft";
        var graphFetches = service.GraphFetchCount;

        pending.SetResult(NewRun());
        await retry;

        Assert.Equal("task-1", viewModel.SelectedTask?.Id);
        Assert.Equal("new task draft", viewModel.RetryInstruction);
        Assert.Equal(graphFetches, service.GraphFetchCount);
        Assert.False(viewModel.IsApplyingAction);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldNodeCompletionCannotEndNewSelectionLoadingOrFetchItsRun(bool fail)
    {
        var service = new TaskGraphServiceDouble();
        var firstTask = await service.FetchTaskAsync("message-1", "task-1", null);
        var secondTask = await service.FetchTaskAsync("message-1", "task-2", null);
        var firstResult = new TaskCompletionSource<MessageTask>(TaskCreationOptions.RunContinuationsAsynchronously);
        var secondResult = new TaskCompletionSource<MessageTask>(TaskCreationOptions.RunContinuationsAsynchronously);
        CancellationToken firstToken = default;
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        service.TaskHandler = (_, id, token) =>
        {
            if (id == "task-1") { firstToken = token; return firstResult.Task; }
            return secondResult.Task;
        };
        var first = viewModel.SelectNodeCommand.ExecuteAsync(viewModel.Nodes[0]);
        Assert.True(viewModel.IsLoading);
        Assert.Null(viewModel.SelectedTask);
        Assert.False(viewModel.CanCancel);
        Assert.False(viewModel.CanRetry);
        var second = viewModel.SelectNodeCommand.ExecuteAsync(viewModel.Nodes[1]);
        Assert.True(firstToken.IsCancellationRequested);
        var runFetches = service.RunFetchCount;

        if (fail) firstResult.SetException(new IOException("old task failed"));
        else firstResult.SetResult(firstTask);
        await first;

        Assert.True(viewModel.IsLoading);
        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal(runFetches, service.RunFetchCount);
        secondResult.SetResult(secondTask);
        await second;
        Assert.False(viewModel.IsLoading);
        Assert.Equal("task-2", viewModel.SelectedTask?.Id);
    }

    [Fact]
    public async Task QueuedOldTaskResultCannotReplaceNewSelection()
    {
        var service = new TaskGraphServiceDouble();
        var task = await service.FetchTaskAsync("message-1", "task-1", null);
        var pending = new TaskCompletionSource<MessageTask>(TaskCreationOptions.RunContinuationsAsynchronously);
        var dispatcher = new PausingUiDispatcher();
        using var viewModel = new MessageTaskGraphViewModel(service, dispatcher);
        await viewModel.OpenAsync(Request());
        service.TaskHandler = (_, _, _) => pending.Task;
        var selection = viewModel.SelectNodeCommand.ExecuteAsync(viewModel.Nodes[0]);
        dispatcher.PauseNext = true;
        pending.SetResult(task with { LastRunId = null });
        await dispatcher.Paused.Task.WaitAsync(TimeSpan.FromSeconds(5));
        service.TaskHandler = null;
        await viewModel.SelectNodeCommand.ExecuteAsync(viewModel.Nodes[1]);

        dispatcher.Release.SetResult();
        await selection;

        Assert.Equal("task-2", viewModel.SelectedTask?.Id);
        Assert.Equal("run-2", viewModel.RunDetail?.Run.Id);
        Assert.False(viewModel.IsLoading);
    }

    [Fact]
    public async Task QueuedOldErrorCannotReplaceReopenedPanelState()
    {
        var pending = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new TaskGraphServiceDouble { GraphHandler = (_, _) => pending.Task };
        var dispatcher = new PausingUiDispatcher();
        using var viewModel = new MessageTaskGraphViewModel(service, dispatcher);
        var opening = viewModel.OpenAsync(Request());
        dispatcher.PauseNext = true;
        pending.SetException(new IOException("queued old failure"));
        await dispatcher.Paused.Task.WaitAsync(TimeSpan.FromSeconds(5));
        service.GraphHandler = null;
        await viewModel.OpenAsync(Request("message-2"));

        dispatcher.Release.SetResult();
        await opening;

        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal("message-2", viewModel.MessageId);
        Assert.False(viewModel.IsLoading);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task OldEventPageCannotClearAnotherPagesBusyState(bool fail)
    {
        var firstPage = new TaskCompletionSource<MessageTaskRunDetail>(TaskCreationOptions.RunContinuationsAsynchronously);
        var secondPage = new TaskCompletionSource<MessageTaskRunDetail>(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        service.RunHandler = (_, id, offset, _) => offset == 0 ? service.RunPage(id, 0)
            : id == "run-2" ? firstPage.Task : secondPage.Task;
        var first = viewModel.LoadMoreEventsCommand.ExecuteAsync(null);
        await viewModel.SelectNodeCommand.ExecuteAsync(viewModel.Nodes[0]);
        var second = viewModel.LoadMoreEventsCommand.ExecuteAsync(null);

        if (fail) firstPage.SetException(new IOException("old page failed"));
        else firstPage.SetResult(await service.RunPage("run-2", 1));
        await first;

        Assert.True(viewModel.IsLoadingMoreEvents);
        Assert.Null(viewModel.ErrorMessage);
        secondPage.SetResult(await service.RunPage("run-1", 1));
        await second;
        Assert.False(viewModel.IsLoadingMoreEvents);
        Assert.Equal("run-1", viewModel.RunDetail?.Run.Id);
        Assert.Equal(2, viewModel.RunEvents.Count);
    }

    [Fact]
    public async Task CurrentEventPageFailureRetainsEventsAndCanBeRetried()
    {
        var service = new TaskGraphServiceDouble();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        service.RunHandler = (_, _, _, _) => throw new IOException("page unavailable");
        await viewModel.LoadMoreEventsCommand.ExecuteAsync(null);
        Assert.Equal("page unavailable", viewModel.ErrorMessage);
        Assert.Single(viewModel.RunEvents);
        Assert.True(viewModel.EventsHasMore);
        Assert.False(viewModel.IsLoadingMoreEvents);

        service.RunHandler = null;
        await viewModel.LoadMoreEventsCommand.ExecuteAsync(null);

        Assert.Null(viewModel.ErrorMessage);
        Assert.Equal(2, viewModel.RunEvents.Count);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CurrentActionFailurePreservesDraftAndReenablesActions(bool retry)
    {
        var pending = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new TaskGraphServiceDouble
        {
            CancelHandler = _ => pending.Task,
            RetryHandler = async _ => { await pending.Task; return NewRun(); },
        };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        viewModel.RetryInstruction = "retry draft";
        viewModel.CancelReason = "cancel draft";
        var action = retry ? viewModel.RetryRunCommand.ExecuteAsync(null) : viewModel.CancelTaskCommand.ExecuteAsync(null);
        Assert.True(viewModel.IsApplyingAction);
        Assert.False(viewModel.CanCancel);
        Assert.False(viewModel.CanRetry);

        pending.SetException(new IOException("action unavailable"));
        await action;

        Assert.Equal("action unavailable", viewModel.ErrorMessage);
        Assert.Equal("retry draft", viewModel.RetryInstruction);
        Assert.Equal("cancel draft", viewModel.CancelReason);
        Assert.False(viewModel.IsApplyingAction);
        Assert.True(viewModel.CanCancel);
        Assert.True(viewModel.CanRetry);
        service.CancelHandler = null;
        service.RetryHandler = null;
        if (retry) await viewModel.RetryRunCommand.ExecuteAsync(null);
        else await viewModel.CancelTaskCommand.ExecuteAsync(null);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task ActionInProgressBlocksDuplicateAndCompetingActions()
    {
        var pending = new TaskCompletionSource<MessageTaskRun>(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new TaskGraphServiceDouble { RetryHandler = _ => pending.Task };
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        var action = viewModel.RetryRunCommand.ExecuteAsync(null);

        await viewModel.RetryRunCommand.ExecuteAsync(null);
        await viewModel.CancelTaskCommand.ExecuteAsync(null);
        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Equal(1, service.RetryCount);
        Assert.Null(service.LastCancellation);
        Assert.Equal(1, service.GraphFetchCount);
        pending.SetResult(NewRun());
        await action;
        Assert.False(viewModel.IsApplyingAction);
    }

    [Fact]
    public async Task CancellationDuringLoadIgnoresSuccessfulResponseAndEndsLoading()
    {
        var service = new TaskGraphServiceDouble();
        var graph = await service.FetchGraphAsync("message-1", null);
        var pending = new TaskCompletionSource<MessageTaskGraphSnapshot>(TaskCreationOptions.RunContinuationsAsynchronously);
        service.GraphHandler = (_, _) => pending.Task;
        using var cancellation = new CancellationTokenSource();
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        var opening = viewModel.OpenAsync(Request(), cancellation.Token);

        cancellation.Cancel();
        pending.SetResult(graph);
        await opening;

        Assert.Empty(viewModel.Nodes);
        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.IsLoading);
        Assert.False(viewModel.CanCancel);
        Assert.False(viewModel.CanRetry);
        Assert.Equal(0, service.RunFetchCount);
    }

    [Fact]
    public async Task RemovedTaskFallsBackToRemainingNodeAndItsOwnRun()
    {
        var service = new TaskGraphServiceDouble();
        var graph = await service.FetchGraphAsync("message-1", null);
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync(Request());
        service.GraphHandler = (_, _) => Task.FromResult(graph with { Nodes = graph.Nodes.Take(1).ToArray() });

        await viewModel.RefreshCommand.ExecuteAsync(null);

        Assert.Equal("task-1", viewModel.SelectedTask?.Id);
        Assert.Equal("run-1", viewModel.RunDetail?.Run.Id);
        Assert.Equal("task-1", viewModel.RequestedTaskId);
        Assert.Null(viewModel.RequestedRunId);
    }

    [Fact]
    public async Task InitialEmptyGraphHasNoDetailsOrTaskActions()
    {
        var service = new TaskGraphServiceDouble();
        var graph = await service.FetchGraphAsync("message-1", null);
        using var viewModel = new MessageTaskGraphViewModel(service, new ImmediateUiDispatcher());
        service.GraphHandler = (_, _) => Task.FromResult(graph with { Nodes = [] });
        await viewModel.OpenAsync(Request());

        Assert.True(viewModel.IsEmpty);
        Assert.Null(viewModel.SelectedNode);
        Assert.Null(viewModel.SelectedTask);
        Assert.Null(viewModel.RunDetail);
        Assert.Empty(viewModel.RunEvents);
        Assert.False(viewModel.CanCancel);
        Assert.False(viewModel.CanRetry);
    }

    private sealed class PausingUiDispatcher : IUiDispatcher
    {
        public bool PauseNext { get; set; }
        public TaskCompletionSource Paused { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);
        public TaskCompletionSource Release { get; } = new(TaskCreationOptions.RunContinuationsAsynchronously);

        public async Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
        {
            if (PauseNext)
            {
                PauseNext = false;
                Paused.TrySetResult();
                await Release.Task;
            }
            cancellationToken.ThrowIfCancellationRequested();
            action();
        }
    }

    private static MessageTaskGraphRequest Request(string messageId = "message-1") => new(
        messageId, "task-2", "run-2", new MessageTaskLookup("conversation-1", "turn-1", messageId));

    private static MessageTaskRun NewRun() => new(
        "run-new", "task-2", "queued", null, null, null, null, null, null);

    private sealed class TaskGraphServiceDouble : IMessageTaskGraphService
    {
        private readonly string _taskStatus;
        private readonly string _runStatus;

        public TaskGraphServiceDouble(string taskStatus = "blocked", string runStatus = "failed")
        {
            _taskStatus = taskStatus;
            _runStatus = runStatus;
        }

        public int GraphFetchCount { get; private set; }
        public int RunFetchCount { get; private set; }
        public int RetryCount { get; private set; }

        public Func<string, CancellationToken, Task<MessageTaskGraphSnapshot>>? GraphHandler { get; set; }
        public Func<string, string, CancellationToken, Task<MessageTask>>? TaskHandler { get; set; }
        public Func<string, string, int, CancellationToken, Task<MessageTaskRunDetail>>? RunHandler { get; set; }
        public Func<CancellationToken, Task<MessageTaskRun>>? RetryHandler { get; set; }
        public Func<CancellationToken, Task>? CancelHandler { get; set; }

        public (string MessageId, string TaskId, string? Reason)? LastCancellation { get; private set; }

        public (string MessageId, string RunId, string? Instruction)? LastRetry { get; private set; }

        public Task<MessageTaskGraphSnapshot> FetchGraphAsync(
            string messageId,
            MessageTaskLookup? lookup,
            CancellationToken cancellationToken = default)
        {
            GraphFetchCount++;
            if (GraphHandler is not null) return GraphHandler(messageId, cancellationToken);
            return Task.FromResult(new MessageTaskGraphSnapshot(
                new[] { "task-2" },
                new[]
                {
                    Node(MakeTask("task-1", "准备环境", "completed", "run-1"), 0),
                    Node(MakeTask("task-2", "实现 Windows 客户端", _taskStatus, "run-2"), 1),
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
            CancellationToken cancellationToken = default) => TaskHandler?.Invoke(messageId, taskId, cancellationToken) ?? Task.FromResult(
            taskId == "task-2"
                ? MakeTask("task-2", "实现 Windows 客户端", _taskStatus, "run-2")
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
            RunFetchCount++;
            return RunHandler?.Invoke(messageId, runId, eventOffset, cancellationToken) ?? RunPage(runId, eventOffset);
        }

        public Task<MessageTaskRunDetail> RunPage(string runId, int eventOffset)
        {
            var task = MakeTask("task-2", "实现 Windows 客户端", _taskStatus, runId);
            var events = eventOffset == 0
                ? new[] { new MessageTaskRunEvent("event-1", "thinking", "开始", null) }
                : new[]
                {
                    new MessageTaskRunEvent("event-1", "thinking", "开始", null),
                    new MessageTaskRunEvent("event-2", "tool", "完成", null),
                };
            return Task.FromResult(new MessageTaskRunDetail(
                task,
                new MessageTaskRun(runId, task.Id, _runStatus, null, null, null, null, null, "blocked"),
                events,
                2,
                eventOffset == 0));
        }

        public Task<MessageTaskRun> RetryRunAsync(
            string messageId,
            string runId,
            MessageTaskLookup? lookup,
            string? instruction,
            CancellationToken cancellationToken = default)
        {
            LastRetry = (messageId, runId, instruction);
            RetryCount++;
            if (RetryHandler is not null) return RetryHandler(cancellationToken);
            return Task.FromResult(
                new MessageTaskRun("run-new", "task-2", "queued", null, null, null, null, null, null));
        }

        public Task CancelTaskAsync(
            string messageId,
            string taskId,
            MessageTaskLookup? lookup,
            string? reason,
            CancellationToken cancellationToken = default)
        {
            LastCancellation = (messageId, taskId, reason);
            if (CancelHandler is not null) return CancelHandler(cancellationToken);
            return Task.CompletedTask;
        }

        private static MessageTaskGraphNode Node(MessageTask task, int depth) => new(
            task,
            depth,
            depth == 1,
            true,
            Array.Empty<MessageTask>());

        internal static MessageTask MakeTask(string id, string title, string status, string runId) => new(
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
