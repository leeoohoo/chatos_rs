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
    public async Task Projection_changes_replace_activities_and_clear_stale_selection()
    {
        var activities = new FakePetActivityService(
            Activity("local-run:run-1", PetActivityKind.Working));
        using var viewModel = Create(activities);
        await viewModel.StartAsync();
        await viewModel.SelectAsync(Assert.Single(viewModel.Activities));

        activities.Replace(
            Activity("local-run:run-1", PetActivityKind.Succeeded) with
            {
                ActivityVersion = "run-version:2",
            });
        await WaitUntilAsync(() =>
            viewModel.Activities.SingleOrDefault()?.Activity.Kind == PetActivityKind.Succeeded);
        Assert.Single(viewModel.Activities);
        Assert.False(viewModel.CanCancelSelected);

        activities.Replace();
        await WaitUntilAsync(() => viewModel.Activities.Count == 0);
        Assert.Null(viewModel.SelectedActivity);
        Assert.False(viewModel.IsDetailOpen);
    }

    [Fact]
    public async Task Ignore_is_only_a_local_versioned_suppression()
    {
        var activities = new FakePetActivityService(
            Activity("local-run:run-1", PetActivityKind.Failed));
        using var viewModel = Create(activities);
        await viewModel.StartAsync();
        var item = Assert.Single(viewModel.Activities);

        await viewModel.IgnoreAsync(item);

        Assert.Equal((item.Activity.StableIdentity, PetActivityDisposition.Ignored),
            activities.Suppressed);
        Assert.Empty(viewModel.Activities);
    }

    [Fact]
    public async Task Cancel_uses_the_exact_local_run_and_conversation()
    {
        var controls = new FakeRunControlService();
        using var viewModel = Create(
            new FakePetActivityService(Activity("local-task:task-1:run-1",
                PetActivityKind.Working)), runControl: controls);
        await viewModel.StartAsync();
        await viewModel.SelectAsync(Assert.Single(viewModel.Activities));

        await viewModel.CancelSelectedAsync();

        Assert.Equal(("run-1", "thread-1"), controls.Cancelled);
        Assert.Contains("取消请求", viewModel.ActionMessage);
    }

    [Fact]
    public async Task Ask_user_selection_uses_the_exact_prompt()
    {
        var ask = new FakeAskUserPromptService();
        using var viewModel = Create(new FakePetActivityService(
            Activity("local-ask:prompt-1", PetActivityKind.WaitingForUser) with
            {
                Source = PetActivitySource.AskUserPrompt,
                Route = Route(promptId: "prompt-1"),
            }), ask: ask);
        await viewModel.StartAsync();

        await viewModel.SelectAsync(Assert.Single(viewModel.Activities));

        var prompt = Assert.IsType<ChatOS.Presentation.Chat.AskUserPromptViewModel>(
            viewModel.ActivePrompt);
        prompt.SelectedSingleOption = prompt.Options[0];
        await prompt.SubmitCommand.ExecuteAsync(null);
        Assert.Equal(("prompt-1", "thread-1"), ask.Submitted);
    }

    [Fact]
    public async Task Tool_approval_selection_exposes_one_exact_invocation()
    {
        var tools = new FakeToolApprovalService();
        using var viewModel = Create(new FakePetActivityService(
            Activity("local-tool:invocation-1", PetActivityKind.WaitingForApproval) with
            {
                Source = PetActivitySource.LocalAgentToolApproval,
                Route = Route(invocationId: "invocation-1"),
            }), tools: tools);
        await viewModel.StartAsync();

        await viewModel.SelectAsync(Assert.Single(viewModel.Activities));

        var approval = Assert.IsType<ChatOS.Presentation.Chat.LocalAgentToolApprovalViewModel>(
            viewModel.ActiveToolApproval);
        await approval.ApproveCommand.ExecuteAsync(null);
        Assert.Equal(("invocation-1", "thread-1", LocalAgentToolApprovalDecision.Approve),
            tools.Decision);
    }

    [Fact]
    public async Task Needs_review_selection_exposes_authoritative_run_controls()
    {
        var controls = new FakeRunControlService(LocalAgentRunStatus.NeedsReview);
        using var viewModel = Create(new FakePetActivityService(
            Activity("local-run:run-1", PetActivityKind.NeedsReview)), runControl: controls);
        await viewModel.StartAsync();

        await viewModel.SelectAsync(Assert.Single(viewModel.Activities));

        var control = Assert.IsType<ChatOS.Presentation.Chat.LocalAgentRunControlViewModel>(
            viewModel.ActiveRunControl);
        Assert.True(control.CanResume);
        Assert.Equal("需要人工复核", control.StatusLabel);
    }

    private static PetOverlayViewModel Create(
        FakePetActivityService activities,
        FakeAskUserPromptService? ask = null,
        FakeToolApprovalService? tools = null,
        FakeRunControlService? runControl = null)
    {
        var dispatcher = new ImmediateUiDispatcher();
        var preferences = new AppPreferencesManager(new MemoryPreferencesStore());
        return new PetOverlayViewModel(
            activities,
            ask ?? new FakeAskUserPromptService(),
            tools ?? new FakeToolApprovalService(),
            runControl ?? new FakeRunControlService(),
            new LocalizationViewModel(preferences, dispatcher),
            dispatcher);
    }

    private static PetActivity Activity(string id, PetActivityKind kind) => new(
        id,
        PetActivitySource.TaskRunner,
        kind,
        "Build",
        "Details",
        Route(),
        activityVersion: "run-version:1",
        updatedAt: DateTimeOffset.UtcNow);

    private static PetActivityRoute Route(
        string? promptId = null,
        string? invocationId = null) => new(
        ProjectId: "project-1",
        ConversationId: "thread-1",
        TurnId: "turn-1",
        TaskId: "task-1",
        RunId: "run-1",
        PromptId: promptId,
        InvocationId: invocationId);

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!condition()) await Task.Delay(10, timeout.Token);
    }

    private sealed class FakePetActivityService(params PetActivity[] values)
        : ILocalAgentPetActivityService
    {
        private readonly List<PetActivity> _values = [.. values];
        public event EventHandler? Changed;
        public (string Identity, PetActivityDisposition Disposition)? Suppressed { get; private set; }

        public Task<IReadOnlyList<PetActivity>> FetchAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<PetActivity>>(_values.ToArray());

        public Task SuppressAsync(PetActivity activity, PetActivityDisposition disposition,
            CancellationToken cancellationToken = default)
        {
            Suppressed = (activity.StableIdentity, disposition);
            _values.RemoveAll(value => value.Id == activity.Id);
            Changed?.Invoke(this, EventArgs.Empty);
            return Task.CompletedTask;
        }

        public void Replace(params PetActivity[] values)
        {
            _values.Clear();
            _values.AddRange(values);
            Changed?.Invoke(this, EventArgs.Empty);
        }
    }

    private sealed class FakeAskUserPromptService : IAskUserPromptService
    {
        public (string PromptId, string ConversationId)? Submitted { get; private set; }
        public Task<IReadOnlyList<AskUserPrompt>> FetchPromptsAsync(
            string conversationId, int limit = 100,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<AskUserPrompt>>(
            [new AskUserPrompt(
                "prompt-1", conversationId, "turn-1", null, "choice",
                AskUserPromptStatus.Pending, "Choose", "Choose one", true, null, [],
                new AskUserChoice(false,
                    [new AskUserChoiceOption("one", "One", null)], [], 1, 1),
                DateTimeOffset.UtcNow, DateTimeOffset.UtcNow, [])]);

        public Task<AskUserPrompt> SubmitAsync(
            string promptId, string conversationId, AskUserSubmission submission,
            CancellationToken cancellationToken = default)
        {
            Submitted = (promptId, conversationId);
            return FetchPromptsAsync(conversationId, cancellationToken: cancellationToken)
                .ContinueWith(task => task.Result[0] with { Status = AskUserPromptStatus.Ok },
                    cancellationToken);
        }

        public Task<AskUserPrompt> CancelAsync(
            string promptId, string conversationId,
            CancellationToken cancellationToken = default) =>
            SubmitAsync(promptId, conversationId, new AskUserSubmission(
                new Dictionary<string, string>(), null), cancellationToken);
    }

    private sealed class FakeToolApprovalService : ILocalAgentToolApprovalService
    {
        public (string InvocationId, string ConversationId,
            LocalAgentToolApprovalDecision Decision)? Decision { get; private set; }

        public Task<IReadOnlyList<LocalAgentToolApprovalRequest>> FetchPendingAsync(
            string conversationId, CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<LocalAgentToolApprovalRequest>>(
            [new("invocation-1", "run-1", conversationId, "turn-1", "filesystem.write",
                LocalAgentToolEffect.Write, "sha256:0123456789abcdef")]);

        public Task DecideAsync(string invocationId, string conversationId,
            LocalAgentToolApprovalDecision decision, string? reason,
            CancellationToken cancellationToken = default)
        {
            Decision = (invocationId, conversationId, decision);
            return Task.CompletedTask;
        }
    }

    private sealed class FakeRunControlService(
        LocalAgentRunStatus status = LocalAgentRunStatus.ModelRunning)
        : ILocalAgentRunControlService
    {
        public (string RunId, string ConversationId)? Cancelled { get; private set; }
        public Task<IReadOnlyList<LocalAgentRunControlState>> FetchRunControlsAsync(
            string conversationId, CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<LocalAgentRunControlState>>(
            [new("run-1", 7, conversationId, "turn-1", status, 2, 0,
                status == LocalAgentRunStatus.NeedsReview
                    ? "review_unknown_tool_outcome" : null,
                status == LocalAgentRunStatus.NeedsReview
                    ? "请核对工具结果。" : null,
                DateTimeOffset.UtcNow)]);
        public Task PauseRunAsync(string runId, string conversationId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task ResumeRunAsync(string runId, string conversationId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
        public Task CancelRunAsync(string runId, string conversationId,
            CancellationToken cancellationToken = default)
        {
            Cancelled = (runId, conversationId);
            return Task.CompletedTask;
        }
    }

    private sealed class MemoryPreferencesStore : IAppPreferencesStore
    {
        public Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<AppPreferences?>(AppPreferences.Default);

        public Task SaveAsync(AppPreferences preferences,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
    }
}
