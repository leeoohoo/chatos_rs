using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Chat;

namespace ChatOS.Presentation.Tests;

public sealed class AskUserPromptViewModelTests
{
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
            Submission = submission;
            return Task.FromResult(Prompt(AskUserPromptStatus.Ok));
        }

        public Task<AskUserPrompt> CancelAsync(
            string promptId,
            string conversationId,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(Prompt(AskUserPromptStatus.Canceled));

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
