using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;

namespace ChatOS.Presentation.Tests;

public sealed class AskUserPromptViewModelTests
{
    [Fact]
    public void LabelsAndSelectionHintMatchMacPresentation()
    {
        var single = CreateViewModel(choice: Choice(false, 1, 1));
        Assert.Equal("任务正在等待你的答复", single.WaitingLabel);
        Assert.Equal("待处理", single.PendingLabel);
        Assert.Equal("请选择", single.ChoiceLabel);
        Assert.Equal("选择 1 项", single.ChoiceHint);
        Assert.Equal("取消请求", single.CancelLabel);
        Assert.Equal("确认提交", single.SubmitLabel);

        var multiple = CreateViewModel(choice: Choice(true, 1, 2));
        Assert.Equal("请选择（可多选）", multiple.ChoiceLabel);
        Assert.Equal("选择 1–2 项", multiple.ChoiceHint);
    }

    [Fact]
    public void SubmitAvailabilityTracksRequiredFieldsAndSingleChoice()
    {
        var viewModel = CreateViewModel(fields: [RequiredField()], choice: Choice(false, 1, 1));
        var notifications = 0;
        viewModel.SubmitCommand.CanExecuteChanged += (_, _) => notifications++;

        Assert.False(viewModel.SubmitCommand.CanExecute(null));
        viewModel.Fields[0].Value = "answer";
        Assert.False(viewModel.SubmitCommand.CanExecute(null));
        viewModel.SelectedSingleOption = viewModel.Options[0];
        Assert.True(viewModel.SubmitCommand.CanExecute(null));
        viewModel.Fields[0].Value = " \r\n\t";
        Assert.False(viewModel.SubmitCommand.CanExecute(null));
        Assert.True(notifications >= 3);
    }

    [Fact]
    public async Task OptionalSingleChoiceCanBeSubmittedWithoutASelection()
    {
        var service = new StubPromptService();
        var viewModel = CreateViewModel(service, choice: Choice(false, 0, 1));

        Assert.True(viewModel.SubmitCommand.CanExecute(null));
        await viewModel.SubmitCommand.ExecuteAsync(null);

        var selection = Assert.IsType<AskUserSelection.Single>(service.Submission?.Selection);
        Assert.Equal(string.Empty, selection.Value);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public void MultipleChoiceRejectsOnlyTheNewSelectionWhenAtMaximum()
    {
        var viewModel = CreateViewModel(choice: Choice(true, 1, 1));
        Assert.False(viewModel.SubmitCommand.CanExecute(null));

        viewModel.Options[0].IsSelected = true;
        Assert.True(viewModel.SubmitCommand.CanExecute(null));
        viewModel.Options[1].IsSelected = true;
        Assert.True(viewModel.Options[0].IsSelected);
        Assert.False(viewModel.Options[1].IsSelected);

        viewModel.Options[0].IsSelected = false;
        Assert.False(viewModel.SubmitCommand.CanExecute(null));
        viewModel.Options[1].IsSelected = true;
        Assert.True(viewModel.SubmitCommand.CanExecute(null));
    }

    [Fact]
    public async Task MultipleChoiceSubmissionFollowsOptionOrder()
    {
        var service = new StubPromptService();
        var viewModel = CreateViewModel(service, choice: Choice(true, 2, 2));
        viewModel.Options[2].IsSelected = true;
        Assert.False(viewModel.SubmitCommand.CanExecute(null));
        viewModel.Options[0].IsSelected = true;
        Assert.True(viewModel.SubmitCommand.CanExecute(null));

        await viewModel.SubmitCommand.ExecuteAsync(null);

        Assert.Equal(new[] { "first", "third" },
            Assert.IsType<AskUserSelection.Multiple>(service.Submission?.Selection).Values);
    }

    [Fact]
    public void DefaultSelectionEnablesAValidRequiredChoice()
    {
        var viewModel = CreateViewModel(choice: Choice(false, 1, 1) with { DefaultSelection = ["second"] });

        Assert.Same(viewModel.Options[1], viewModel.SelectedSingleOption);
        Assert.True(viewModel.SubmitCommand.CanExecute(null));
    }

    [Fact]
    public async Task SubmitInProgressDisablesCancelAndResumesAfterFailure()
    {
        var completion = new TaskCompletionSource<AskUserPrompt>(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new StubPromptService { SubmitHandler = _ => completion.Task };
        var viewModel = CreateViewModel(service, fields: [RequiredField()]);
        viewModel.Fields[0].Value = "draft answer";
        var submit = viewModel.SubmitCommand.ExecuteAsync(null);

        Assert.True(viewModel.IsSubmitting);
        Assert.False(viewModel.CancelCommand.CanExecute(null));
        await viewModel.CancelCommand.ExecuteAsync(null);
        Assert.Equal(0, service.CancelCalls);

        completion.SetException(new InvalidOperationException("offline"));
        await submit;
        Assert.False(viewModel.IsSubmitting);
        Assert.True(viewModel.SubmitCommand.CanExecute(null));
        Assert.True(viewModel.CancelCommand.CanExecute(null));
        Assert.Equal("draft answer", viewModel.Fields[0].Value);
        Assert.Equal("offline", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task CancelInProgressDisablesSubmitAndResumesAfterFailure()
    {
        var completion = new TaskCompletionSource<AskUserPrompt>(TaskCreationOptions.RunContinuationsAsynchronously);
        var service = new StubPromptService { CancelHandler = () => completion.Task };
        var viewModel = CreateViewModel(service);
        var cancel = viewModel.CancelCommand.ExecuteAsync(null);

        Assert.True(viewModel.IsSubmitting);
        Assert.False(viewModel.SubmitCommand.CanExecute(null));
        await viewModel.SubmitCommand.ExecuteAsync(null);
        Assert.Equal(0, service.SubmitCalls);

        completion.SetException(new InvalidOperationException("cancel failed"));
        await cancel;
        Assert.True(viewModel.SubmitCommand.CanExecute(null));
        Assert.True(viewModel.CancelCommand.CanExecute(null));
        Assert.Equal("cancel failed", viewModel.ErrorMessage);
    }

    [Fact]
    public async Task NonCancelablePromptDisablesCancelAndNeverCallsService()
    {
        var service = new StubPromptService();
        var viewModel = new AskUserPromptViewModel(
            Form() with { AllowsCancel = false }, service, () => Task.CompletedTask);

        Assert.False(viewModel.CancelCommand.CanExecute(null));
        await viewModel.CancelCommand.ExecuteAsync(null);
        Assert.Equal(0, service.CancelCalls);
    }

    [Theory]
    [InlineData(AskUserPromptStatus.Ok)]
    [InlineData(AskUserPromptStatus.Canceled)]
    [InlineData(AskUserPromptStatus.Timeout)]
    [InlineData(AskUserPromptStatus.Failed)]
    public async Task ResolvedPromptsCannotSubmitOrCancel(AskUserPromptStatus status)
    {
        var service = new StubPromptService();
        var viewModel = new AskUserPromptViewModel(Form() with { Status = status }, service, () => Task.CompletedTask);

        Assert.False(viewModel.SubmitCommand.CanExecute(null));
        Assert.False(viewModel.CancelCommand.CanExecute(null));
        await viewModel.SubmitCommand.ExecuteAsync(null);
        await viewModel.CancelCommand.ExecuteAsync(null);
        Assert.Equal(0, service.SubmitCalls);
        Assert.Equal(0, service.CancelCalls);
    }

    private static AskUserPromptViewModel CreateViewModel(
        StubPromptService? service = null, IReadOnlyList<AskUserField>? fields = null, AskUserChoice? choice = null) =>
        new(Form() with { Fields = fields ?? [], Choice = choice }, service ?? new StubPromptService(), () => Task.CompletedTask);

    private static AskUserPrompt Form() => new(
        "prompt-1", "conversation-a", "turn-1", null, "form", AskUserPromptStatus.Pending,
        "Confirm", "Choose", true, null, [], null, null, null);

    private static AskUserField RequiredField() => new("answer", "Answer", null, null, "", true, false, false);

    private static AskUserChoice Choice(bool multiple, int minimum, int maximum) => new(
        multiple,
        [new("first", "First", null), new("second", "Second", null), new("third", "Third", null)],
        [], minimum, maximum);

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task ResolvedResponseCannotBeRepeatedWhenPromptRefreshFails(bool cancel)
    {
        var service = new StubPromptService();
        var viewModel = new AskUserPromptViewModel(Form() with
        {
            Fields = [RequiredField() with { IsSecret = true },
                new("comment", "Comment", null, null, "keep this comment", false, false, false)],
        }, service, () => Task.FromException(new InvalidOperationException("refresh failed")));
        viewModel.Fields[0].Value = "temporary input";

        if (cancel) await viewModel.CancelCommand.ExecuteAsync(null);
        else await viewModel.SubmitCommand.ExecuteAsync(null);

        Assert.Equal(cancel ? AskUserPromptStatus.Canceled : AskUserPromptStatus.Ok, viewModel.Prompt.Status);
        Assert.Equal(string.Empty, viewModel.Fields[0].Value);
        Assert.Equal("keep this comment", viewModel.Fields[1].Value);
        Assert.False(viewModel.SubmitCommand.CanExecute(null));
        Assert.False(viewModel.CancelCommand.CanExecute(null));
        Assert.Equal("refresh failed", viewModel.ErrorMessage);
        await viewModel.SubmitCommand.ExecuteAsync(null);
        await viewModel.CancelCommand.ExecuteAsync(null);
        Assert.Equal(cancel ? 0 : 1, service.SubmitCalls);
        Assert.Equal(cancel ? 1 : 0, service.CancelCalls);
    }

    [Fact]
    public async Task FailedSubmissionPreservesSecretInputForRetry()
    {
        var service = new StubPromptService
        {
            SubmitHandler = _ => Task.FromException<AskUserPrompt>(new InvalidOperationException("offline")),
        };
        var viewModel = CreateViewModel(service, fields: [RequiredField() with { IsSecret = true }]);
        viewModel.Fields[0].Value = "temporary input";

        await viewModel.SubmitCommand.ExecuteAsync(null);

        Assert.Equal(AskUserPromptStatus.Pending, viewModel.Prompt.Status);
        Assert.Equal("temporary input", viewModel.Fields[0].Value);
        Assert.True(viewModel.SubmitCommand.CanExecute(null));
    }

    [Fact]
    public async Task RequiredSecretFieldAndChoiceAreValidatedBeforeSubmission()
    {
        var service = new StubPromptService();
        var prompt = new AskUserPrompt(
            "prompt-1",
            "conversation-a",
            "turn-1",
            null,
            "form",
            AskUserPromptStatus.Pending,
            "部署信息",
            "请确认",
            true,
            null,
            new[] { new AskUserField("token", "Token", null, null, "", true, false, true) },
            new AskUserChoice(
                false,
                new[] { new AskUserChoiceOption("prod", "生产", null) },
                Array.Empty<string>(),
                1,
                1),
            null,
            null);
        var changed = 0;
        var viewModel = new AskUserPromptViewModel(prompt, service, () =>
        {
            changed++;
            return Task.CompletedTask;
        });

        await viewModel.SubmitCommand.ExecuteAsync(null);
        Assert.Contains("Token", viewModel.ErrorMessage);
        Assert.Null(service.Submission);

        viewModel.Fields[0].Value = "secret";
        viewModel.SelectedSingleOption = viewModel.Options[0];
        await viewModel.SubmitCommand.ExecuteAsync(null);

        Assert.Equal("secret", service.Submission?.Values["token"]);
        Assert.IsType<AskUserSelection.Single>(service.Submission?.Selection);
        Assert.Equal(1, changed);
    }

    private sealed class StubPromptService : IAskUserPromptService
    {
        public Func<AskUserSubmission, Task<AskUserPrompt>>? SubmitHandler { get; init; }

        public Func<Task<AskUserPrompt>>? CancelHandler { get; init; }

        public int SubmitCalls { get; private set; }

        public int CancelCalls { get; private set; }

        public AskUserSubmission? Submission { get; private set; }

        public Task<IReadOnlyList<AskUserPrompt>> FetchPromptsAsync(
            string conversationId,
            int limit = 100,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<AskUserPrompt>>(Array.Empty<AskUserPrompt>());

        public Task<AskUserPrompt> SubmitAsync(
            string promptId,
            string conversationId,
            AskUserSubmission submission,
            CancellationToken cancellationToken = default)
        {
            SubmitCalls++;
            Submission = submission;
            return SubmitHandler?.Invoke(submission) ?? Task.FromResult(Prompt(AskUserPromptStatus.Ok));
        }

        public Task<AskUserPrompt> CancelAsync(
            string promptId,
            string conversationId,
            CancellationToken cancellationToken = default)
        {
            CancelCalls++;
            return CancelHandler?.Invoke() ?? Task.FromResult(Prompt(AskUserPromptStatus.Canceled));
        }

        private static AskUserPrompt Prompt(AskUserPromptStatus status) => new(
            "prompt-1",
            "conversation-a",
            "turn-1",
            null,
            "form",
            status,
            "",
            "",
            true,
            null,
            Array.Empty<AskUserField>(),
            null,
            null,
            null);
    }
}
