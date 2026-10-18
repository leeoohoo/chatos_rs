using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Tasks;

namespace ChatOS.Presentation.Tests;

public sealed class ConversationReplyInspectorTests
{
    [Theory]
    [InlineData(TaskReplyInspectorSection.Process)]
    [InlineData(TaskReplyInspectorSection.Detail)]
    public async Task CurrentLoadShowsTaskAndRunAndStopsLoading(TaskReplyInspectorSection section)
    {
        var reply = Reply();
        var snapshot = Snapshot("current");
        MessageTaskGraphRequest? requested = null;
        await reply.ToggleInspectorAsync(section, (request, _) =>
        {
            requested = request;
            Assert.True(reply.IsInspectorLoading);
            return Task.FromResult(snapshot);
        });

        Assert.Same(reply.TaskGraphRequest, requested);
        Assert.Same(snapshot.Task, reply.InspectorTask);
        Assert.Same(snapshot.Run, reply.InspectorRun);
        Assert.Equal(section, reply.InspectorSection);
        Assert.False(reply.IsInspectorLoading);
        Assert.Null(reply.InspectorError);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task CollapseCancelsPendingLoadAndIgnoresLateSuccessOrError(bool fails)
    {
        var reply = Reply();
        var pending = Pending();
        CancellationToken token = default;
        var loading = reply.ToggleInspectorAsync(TaskReplyInspectorSection.Process, (_, cancellation) =>
        {
            token = cancellation;
            return pending.Task;
        });

        await reply.ToggleInspectorAsync(TaskReplyInspectorSection.Process, (_, _) => throw new Exception("Must not load on collapse."));

        Assert.True(token.IsCancellationRequested);
        Finish(pending, fails, "stale");
        await loading;
        AssertClosed(reply);
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public async Task ReturningToSameSectionRejectsOldResultsAndPreservesNewestLoadingState(bool oldFails, bool newestFirst)
    {
        var reply = Reply();
        var first = Pending();
        var middle = Pending();
        var latest = Pending();
        CancellationToken firstToken = default, middleToken = default;
        var firstLoad = reply.ToggleInspectorAsync(TaskReplyInspectorSection.Process, (_, token) =>
        {
            firstToken = token;
            return first.Task;
        });
        var middleLoad = reply.ToggleInspectorAsync(TaskReplyInspectorSection.Detail, (_, token) =>
        {
            middleToken = token;
            return middle.Task;
        });
        var latestLoad = reply.ToggleInspectorAsync(TaskReplyInspectorSection.Process, (_, _) => latest.Task);
        Assert.True(firstToken.IsCancellationRequested);
        Assert.True(middleToken.IsCancellationRequested);

        if (newestFirst)
        {
            latest.SetResult(Snapshot("newest"));
            await latestLoad;
        }
        Finish(first, oldFails, "old process");
        Finish(middle, oldFails, "old detail");
        await Task.WhenAll(firstLoad, middleLoad);
        Assert.Null(reply.InspectorError);
        Assert.Equal(TaskReplyInspectorSection.Process, reply.InspectorSection);
        if (!newestFirst)
        {
            Assert.True(reply.IsInspectorLoading);
            Assert.Null(reply.InspectorTask);
            latest.SetResult(Snapshot("newest"));
            await latestLoad;
        }

        Assert.Equal("newest", reply.InspectorTask?.Title);
        Assert.Equal("newest", reply.InspectorRun?.Run.ReportContent);
        Assert.False(reply.IsInspectorLoading);
        Assert.Null(reply.InspectorError);
    }

    [Theory]
    [InlineData(TaskReplyInspectorSection.Process)]
    [InlineData(TaskReplyInspectorSection.Detail)]
    public async Task CurrentFailureIsVisibleAndCanBeReopened(TaskReplyInspectorSection section)
    {
        var reply = Reply();
        await reply.ToggleInspectorAsync(section, (_, _) => throw new IOException("current failure"));
        Assert.Equal("current failure", reply.InspectorError);
        Assert.False(reply.IsInspectorLoading);
        Assert.True(reply.HasInspector);

        reply.CloseInspector();
        await reply.ToggleInspectorAsync(section, (_, _) => Task.FromResult(Snapshot("recovered")));
        Assert.Equal("recovered", reply.InspectorTask?.Title);
        Assert.Null(reply.InspectorError);
    }

    [Fact]
    public async Task CooperativeCancellationIsQuietAndTheNewLoadStillCompletes()
    {
        var reply = Reply();
        var firstLoad = reply.ToggleInspectorAsync(TaskReplyInspectorSection.Process, async (_, token) =>
        {
            await Task.Delay(Timeout.Infinite, token);
            return Snapshot("must not be applied");
        });
        await reply.ToggleInspectorAsync(TaskReplyInspectorSection.Detail, (_, _) => Task.FromResult(Snapshot("newest")));
        await firstLoad;

        Assert.Equal("newest", reply.InspectorTask?.Title);
        Assert.True(reply.IsDetailExpanded);
        Assert.Null(reply.InspectorError);
        Assert.False(reply.IsInspectorLoading);
    }

    [Fact]
    public async Task CloseClearsLoadedDetailsAndIsIdempotent()
    {
        var reply = Reply();
        await reply.ToggleInspectorAsync(TaskReplyInspectorSection.Detail, (_, _) => Task.FromResult(Snapshot("loaded")));
        reply.CloseInspector();
        reply.CloseInspector();
        AssertClosed(reply);
    }

    [Fact]
    public async Task AReplyWithoutTaskContextDoesNotStartAnInspectorLoad()
    {
        var reply = new ConversationReplyItemViewModel("plain", "reply", DateTimeOffset.UtcNow, null, null, null, null);
        await reply.ToggleInspectorAsync(TaskReplyInspectorSection.Detail, (_, _) => throw new Exception("Must not fetch."));
        AssertClosed(reply);
    }

    [Fact]
    public async Task SeparateRepliesDoNotCancelEachOthersLoads()
    {
        var first = Reply();
        var second = Reply();
        var pending = Pending();
        CancellationToken token = default;
        var firstLoad = first.ToggleInspectorAsync(TaskReplyInspectorSection.Process, (_, cancellation) =>
        {
            token = cancellation;
            return pending.Task;
        });
        await second.ToggleInspectorAsync(TaskReplyInspectorSection.Detail, (_, _) => Task.FromResult(Snapshot("second")));
        Assert.False(token.IsCancellationRequested);
        pending.SetResult(Snapshot("first"));
        await firstLoad;
        Assert.Equal("first", first.InspectorTask?.Title);
        Assert.Equal("second", second.InspectorTask?.Title);
    }

    internal static ConversationReplyItemViewModel Reply() => new(
        "reply", "task reply", DateTimeOffset.UtcNow, "task", "run", "completed",
        new("message", "task", "run", new("conversation", "turn", "message")));

    internal static TaskReplyInspectorSnapshot Snapshot(string title)
    {
        var task = new MessageTask(
            Id: "task", Title: title, Description: null, Objective: null, Status: "completed",
            Priority: null, Tags: [], DefaultModelConfigId: null, DefaultModelConfig: null,
            CreatorUserId: null, CreatorUsername: null, CreatorDisplayName: null,
            ResultSummary: title, ProcessLog: title, LastRunId: "run", LastRunStatus: "completed",
            LastRun: null, ParentTaskId: null, ParentTask: null, SourceRunId: null, SourceRun: null,
            SourceConversationId: "conversation", SourceTurnId: "turn", SourceUserMessageId: "message",
            PrerequisiteTaskIds: [], PrerequisiteTasks: [], ProjectTaskId: null, ExecutionClientRef: null,
            DependencyContextRefs: [], ScheduleJson: null, TaskToolStateJson: null, McpConfigJson: null,
            InputPayloadJson: null, CreatedAt: null, UpdatedAt: null);
        return new(task, new(task, new("run", "task", "completed", null, null, null, null, title, null), [], 0, false));
    }

    private static TaskCompletionSource<TaskReplyInspectorSnapshot> Pending() => new(TaskCreationOptions.RunContinuationsAsynchronously);

    private static void Finish(TaskCompletionSource<TaskReplyInspectorSnapshot> pending, bool fails, string title)
    {
        if (fails) pending.SetException(new IOException(title));
        else pending.SetResult(Snapshot(title));
    }

    internal static void AssertClosed(ConversationReplyItemViewModel reply)
    {
        Assert.False(reply.HasInspector);
        Assert.False(reply.IsProcessExpanded);
        Assert.False(reply.IsDetailExpanded);
        Assert.False(reply.IsInspectorLoading);
        Assert.Null(reply.InspectorTask);
        Assert.Null(reply.InspectorRun);
        Assert.Null(reply.InspectorError);
    }
}
