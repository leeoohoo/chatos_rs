using System.Runtime.CompilerServices;
using System.Threading.Channels;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Presentation.Pet;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class PetOverlayViewModelTests
{
    [Fact]
    public async Task Completed_and_blocked_activities_remain_visible_until_the_user_acts()
    {
        var inbox = new FakeInboxService(
            Activity("complete", PetActivityKind.Succeeded, "Build finished"),
            Activity("blocked", PetActivityKind.Blocked, "Need a decision"));
        var viewModel = Create(inbox);

        await viewModel.RefreshAsync();

        Assert.Equal(2, viewModel.Activities.Count);
        Assert.Equal("blocked", viewModel.Activities[0].Id);
        Assert.Equal("已阻塞", viewModel.Activities[0].StatusLabel);
        Assert.Equal("complete", viewModel.Activities[1].Id);
        Assert.True(viewModel.HasActivities);
        Assert.Equal(PetAnimationState.Failed, viewModel.AnimationState);
    }

    [Fact]
    public async Task Ignore_removes_the_activity_and_records_a_stable_suppression()
    {
        var inbox = new FakeInboxService(Activity("blocked", PetActivityKind.Blocked, "Blocked"));
        var suppression = new FakeSuppressionStore();
        var viewModel = Create(inbox, suppression: suppression);
        await viewModel.RefreshAsync();
        var activity = Assert.Single(viewModel.Activities);

        await viewModel.IgnoreAsync(activity);

        Assert.Empty(viewModel.Activities);
        Assert.Equal(PetActivityDisposition.Ignored, inbox.AppliedDisposition);
        Assert.Contains(activity.Activity.StableIdentity, suppression.Suppressed);
    }

    [Fact]
    public async Task Running_task_cancel_uses_the_full_message_task_lookup()
    {
        var route = new PetActivityRoute(
            ConversationId: "conversation-one",
            TurnId: "turn-one",
            MessageId: "message-one",
            TaskId: "task-one",
            RunId: "run-one");
        var tasks = new FakeTaskGraphService();
        var viewModel = Create(
            new FakeInboxService(Activity("running", PetActivityKind.Working, "Running", route)),
            tasks: tasks);
        await viewModel.RefreshAsync();
        await viewModel.SelectAsync(Assert.Single(viewModel.Activities));

        await viewModel.CancelSelectedAsync();

        Assert.Equal("message-one", tasks.CancelledMessageId);
        Assert.Equal("task-one", tasks.CancelledTaskId);
        Assert.Equal(new MessageTaskLookup("conversation-one", "turn-one", "message-one"), tasks.Lookup);
        Assert.Contains("取消请求", viewModel.ActionMessage);
    }

    [Fact]
    public async Task Conversation_activity_can_cancel_the_active_turn()
    {
        var commands = new FakeConversationCommandService();
        var route = new PetActivityRoute(ConversationId: "conversation-one", TurnId: "turn-one");
        var viewModel = Create(
            new FakeInboxService(Activity("chat", PetActivityKind.Reviewing, "Reviewing", route)),
            commands: commands);
        await viewModel.RefreshAsync();
        await viewModel.SelectAsync(Assert.Single(viewModel.Activities));

        await viewModel.CancelSelectedAsync();

        Assert.Equal(("conversation-one", "turn-one"), commands.StoppedTurn);
    }

    [Fact]
    public async Task Ask_user_detail_loads_the_real_prompt_and_submission_marks_the_inbox_handled()
    {
        var activity = Activity(
            "ask",
            PetActivityKind.WaitingForUser,
            "Choose a target",
            new PetActivityRoute(
                ConversationId: "conversation-one",
                TurnId: "turn-one",
                PromptId: "prompt-one"));
        var inbox = new FakeInboxService(activity);
        var ask = new FakeAskUserPromptService();
        var viewModel = Create(inbox, ask: ask);
        await viewModel.RefreshAsync();

        await viewModel.SelectAsync(Assert.Single(viewModel.Activities));

        var prompt = Assert.IsType<ChatOS.Presentation.Chat.AskUserPromptViewModel>(viewModel.ActivePrompt);
        Assert.Equal("Choose a target", prompt.Title);
        prompt.SelectedSingleOption = prompt.Options[0];
        await prompt.SubmitCommand.ExecuteAsync(null);

        Assert.Equal("prompt-one", ask.SubmittedPromptId);
        Assert.Equal(PetActivityDisposition.Handled, inbox.AppliedDisposition);
        Assert.Empty(viewModel.Activities);
    }

    [Fact]
    public async Task Realtime_updates_replace_the_same_activity_without_creating_duplicates()
    {
        var realtime = new FakeRealtimeClient();
        var initial = Activity("task", PetActivityKind.Working, "Compile") with
        {
            EventSequence = 1,
        };
        var viewModel = Create(new FakeInboxService(initial), realtime: realtime);
        await viewModel.StartAsync();

        realtime.Publish(new PetActivityEvent.Upsert(initial with
        {
            Kind = PetActivityKind.Succeeded,
            EventSequence = 2,
            UpdatedAt = DateTimeOffset.UtcNow.AddSeconds(1),
        }));
        await WaitUntilAsync(() => viewModel.Activities.SingleOrDefault()?.Activity.Kind == PetActivityKind.Succeeded);

        Assert.Single(viewModel.Activities);
        Assert.Equal("已完成", viewModel.Activities[0].StatusLabel);
        viewModel.Stop();
    }

    private static PetOverlayViewModel Create(
        FakeInboxService inbox,
        FakeSuppressionStore? suppression = null,
        FakeRealtimeClient? realtime = null,
        FakeConversationCommandService? commands = null,
        FakeTaskGraphService? tasks = null,
        FakeAskUserPromptService? ask = null)
    {
        var dispatcher = new ImmediateUiDispatcher();
        var preferences = new AppPreferencesManager(new MemoryPreferencesStore());
        return new PetOverlayViewModel(
            new PetActivityCoordinator(inbox, suppression ?? new FakeSuppressionStore()),
            realtime ?? new FakeRealtimeClient(),
            commands ?? new FakeConversationCommandService(),
            tasks ?? new FakeTaskGraphService(),
            ask ?? new FakeAskUserPromptService(),
            new LocalizationViewModel(preferences, dispatcher),
            dispatcher);
    }

    private static PetActivity Activity(
        string id,
        PetActivityKind kind,
        string title,
        PetActivityRoute? route = null) => new(
            id,
            kind == PetActivityKind.WaitingForUser ? PetActivitySource.AskUserPrompt : PetActivitySource.TaskRunner,
            kind,
            title,
            $"Details for {title}",
            route,
            inboxId: $"inbox-{id}",
            inboxStatus: PetActivityInboxStatus.Unread,
            activityVersion: "run-one",
            updatedAt: DateTimeOffset.UtcNow);

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!condition())
        {
            await Task.Delay(10, timeout.Token);
        }
    }

    private sealed class FakeInboxService(params PetActivity[] activities) : IPetActivityInboxService
    {
        private readonly List<PetActivity> _activities = [.. activities];
        public PetActivityDisposition? AppliedDisposition { get; private set; }

        public Task<IReadOnlyList<PetActivity>> FetchOpenActivitiesAsync(
            int limit = 100,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<PetActivity>>(_activities.Take(limit).ToArray());

        public Task ApplyAsync(
            PetActivityDisposition disposition,
            PetActivity activity,
            CancellationToken cancellationToken = default)
        {
            AppliedDisposition = disposition;
            _activities.RemoveAll(value => value.Id == activity.Id);
            return Task.CompletedTask;
        }
    }

    private sealed class FakeSuppressionStore : IPetActivitySuppressionStore
    {
        public HashSet<string> Suppressed { get; } = new(StringComparer.Ordinal);

        public Task<bool> IsSuppressedAsync(
            string stableIdentity,
            DateTimeOffset now,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(Suppressed.Contains(stableIdentity));

        public Task SuppressAsync(
            string stableIdentity,
            PetActivityDisposition disposition,
            DateTimeOffset suppressedAt,
            DateTimeOffset? expiresAt,
            CancellationToken cancellationToken = default)
        {
            Suppressed.Add(stableIdentity);
            return Task.CompletedTask;
        }

        public Task RemoveAsync(string stableIdentity, CancellationToken cancellationToken = default)
        {
            Suppressed.Remove(stableIdentity);
            return Task.CompletedTask;
        }

        public Task PruneExpiredAsync(DateTimeOffset now, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
    }

    private sealed class FakeRealtimeClient : IRealtimeClient
    {
        private readonly Channel<PetActivityEvent> _events = Channel.CreateUnbounded<PetActivityEvent>();

        public void Publish(PetActivityEvent activityEvent) => _events.Writer.TryWrite(activityEvent);

        public async IAsyncEnumerable<PetActivityEvent> StreamPetActivitiesAsync(
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            await foreach (var item in _events.Reader.ReadAllAsync(cancellationToken))
            {
                yield return item;
            }
        }

        public async IAsyncEnumerable<ConversationRealtimeSignal> StreamConversationAsync(
            string conversationId,
            [EnumeratorCancellation] CancellationToken cancellationToken = default)
        {
            await Task.CompletedTask;
            yield break;
        }
    }

    private sealed class FakeConversationCommandService : IConversationCommandService
    {
        public (string ConversationId, string? TurnId)? StoppedTurn { get; private set; }

        public Task<ConversationCommandAck> SendNewTurnAsync(
            ConversationSendCommand command,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();

        public Task<ConversationCommandAck> SendGuidanceAsync(
            ConversationSendCommand command,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();

        public Task StopTurnAsync(
            string conversationId,
            string? turnId,
            CancellationToken cancellationToken = default)
        {
            StoppedTurn = (conversationId, turnId);
            return Task.CompletedTask;
        }
    }

    private sealed class FakeTaskGraphService : IMessageTaskGraphService
    {
        public string? CancelledMessageId { get; private set; }
        public string? CancelledTaskId { get; private set; }
        public MessageTaskLookup? Lookup { get; private set; }

        public Task CancelTaskAsync(
            string messageId,
            string taskId,
            MessageTaskLookup? lookup,
            string? reason,
            CancellationToken cancellationToken = default)
        {
            CancelledMessageId = messageId;
            CancelledTaskId = taskId;
            Lookup = lookup;
            return Task.CompletedTask;
        }

        public Task<MessageTaskGraphSnapshot> FetchGraphAsync(string messageId, MessageTaskLookup? lookup, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<MessageTask> FetchTaskAsync(string messageId, string taskId, MessageTaskLookup? lookup, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<MessageTaskRunDetail> FetchRunAsync(string messageId, string runId, MessageTaskLookup? lookup, bool includeEvents = true, int eventLimit = 40, int eventOffset = 0, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public Task<MessageTaskRun> RetryRunAsync(string messageId, string runId, MessageTaskLookup? lookup, string? instruction, CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }

    private sealed class FakeAskUserPromptService : IAskUserPromptService
    {
        public string? SubmittedPromptId { get; private set; }

        public Task<IReadOnlyList<AskUserPrompt>> FetchPromptsAsync(
            string conversationId,
            int limit = 100,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<AskUserPrompt>>(
            [new AskUserPrompt(
                "prompt-one",
                conversationId,
                "turn-one",
                null,
                "choice",
                AskUserPromptStatus.Pending,
                "Choose a target",
                "Select where to deploy.",
                true,
                null,
                [],
                new AskUserChoice(
                    false,
                    [new AskUserChoiceOption("staging", "Staging", null)],
                    [],
                    1,
                    1),
                DateTimeOffset.UtcNow,
                DateTimeOffset.UtcNow)]);

        public Task<AskUserPrompt> SubmitAsync(
            string promptId,
            string conversationId,
            AskUserSubmission submission,
            CancellationToken cancellationToken = default)
        {
            SubmittedPromptId = promptId;
            return Task.FromResult((FetchPromptsAsync(conversationId).Result[0]) with
            {
                Status = AskUserPromptStatus.Ok,
            });
        }

        public Task<AskUserPrompt> CancelAsync(
            string promptId,
            string conversationId,
            CancellationToken cancellationToken = default) =>
            Task.FromResult((FetchPromptsAsync(conversationId).Result[0]) with
            {
                Status = AskUserPromptStatus.Canceled,
            });
    }

    private sealed class MemoryPreferencesStore : IAppPreferencesStore
    {
        public Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<AppPreferences?>(AppPreferences.Default);

        public Task SaveAsync(AppPreferences preferences, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
    }
}
