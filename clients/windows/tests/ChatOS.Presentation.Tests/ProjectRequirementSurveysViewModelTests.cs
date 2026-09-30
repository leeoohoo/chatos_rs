using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.AgentTeams;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Tests;

public sealed class ProjectRequirementSurveysViewModelTests
{
    [Fact]
    public async Task OpenOrdersActionableSurveysBeforeResolvedHistory()
    {
        var service = new StubRequirementSurveyService
        {
            Surveys =
            [
                Survey("resolved", AgentRequirementSurveyStatus.Submitted, 300, resolved: true),
                Survey("awaiting", AgentRequirementSurveyStatus.Submitted, 400),
                Survey("pending", AgentRequirementSurveyStatus.Pending, 100),
            ],
        };
        using var viewModel = new ProjectRequirementSurveysViewModel(
            service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync("owner", Project());

        Assert.Equal(["pending", "awaiting", "resolved"],
            viewModel.Surveys.Select(value => value.Id));
        Assert.Equal(1, viewModel.PendingCount);
        Assert.Equal(0, viewModel.AwaitingResolutionCount);
        Assert.Equal(1, viewModel.ResolvedCount);
        Assert.True(viewModel.SelectedSurvey!.CanSubmit);
        Assert.Contains("Human", viewModel.SelectedSurvey.PermissionText);
    }

    [Fact]
    public async Task SubmitUsesProjectScopeAndLocksHumanAnswers()
    {
        var pending = Survey("pending", AgentRequirementSurveyStatus.Pending, 100);
        var service = new StubRequirementSurveyService { Surveys = [pending] };
        using var viewModel = new ProjectRequirementSurveysViewModel(
            service, new ImmediateUiDispatcher());
        await viewModel.OpenAsync("owner", Project());
        var submission = new AgentRequirementSubmission(
            [new AgentRequirementAnswer("q1", ["yes"])], "范围确认完毕");

        var submitted = await viewModel.SubmitAsync(pending, submission);

        Assert.True(submitted);
        Assert.Equal("project", service.SubmittedProjectId);
        Assert.Equal("pending", service.SubmittedSurveyId);
        Assert.Equal(0, viewModel.PendingCount);
        Assert.Equal(1, viewModel.ResolvedCount);
        Assert.False(viewModel.SelectedSurvey!.CanSubmit);
        Assert.Equal("范围确认完毕", viewModel.SelectedSurvey.SubmissionNotes);
        Assert.Contains("只读", viewModel.SelectedSurvey.PermissionText);
    }

    [Fact]
    public async Task RefreshFailureProducesRecoverableErrorState()
    {
        var service = new StubRequirementSurveyService { ListError = new IOException("database busy") };
        using var viewModel = new ProjectRequirementSurveysViewModel(
            service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync("owner", Project());

        Assert.Equal("database busy", viewModel.ErrorMessage);
        Assert.False(viewModel.IsBusy);
        Assert.False(viewModel.HasSurveys);
        Assert.True(viewModel.CanRefresh);
    }

    private static WorkspaceProject Project() =>
        new("project", "Parity", null, null, null);

    private static AgentRequirementSurvey Survey(
        string id,
        AgentRequirementSurveyStatus status,
        long createdAt,
        bool resolved = false)
    {
        var draft = new AgentRequirementSurveyDraft("范围确认", "确认交付范围",
        [
            new AgentRequirementQuestion("q1", "是否进入首版？",
                AgentRequirementQuestionKind.SingleChoice,
                [new AgentRequirementOption("yes", "是"), new AgentRequirementOption("no", "否")]),
        ]);
        var submission = status == AgentRequirementSurveyStatus.Pending
            ? null
            : new AgentRequirementSubmission([new AgentRequirementAnswer("q1", ["yes"])]);
        var resolution = resolved
            ? new AgentRequirementResolution("按首版推进", "先完成首版。",
                [new AgentRequirementExecutionStep("step1", "实现", "完成代码")])
            : null;
        return new AgentRequirementSurvey(id, "owner", "project", "agent", "delivery", id,
            draft, status, submission, resolution, createdAt,
            status == AgentRequirementSurveyStatus.Submitted ? createdAt + 1 : null,
            resolved ? createdAt + 2 : null);
    }

    private sealed class StubRequirementSurveyService : IProjectRequirementSurveyService
    {
        public event EventHandler<ProjectRequirementSurveyChangedEventArgs>? Changed;

        public IReadOnlyList<AgentRequirementSurvey> Surveys { get; set; } = [];
        public Exception? ListError { get; set; }
        public string? SubmittedProjectId { get; private set; }
        public string? SubmittedSurveyId { get; private set; }

        public Task<IReadOnlyList<AgentRequirementSurvey>> ListAsync(
            string ownerUserId,
            string projectId,
            CancellationToken cancellationToken = default) =>
            ListError is null
                ? Task.FromResult(Surveys)
                : Task.FromException<IReadOnlyList<AgentRequirementSurvey>>(ListError);

        public Task<AgentRequirementSurvey> SubmitAsync(
            string ownerUserId,
            string projectId,
            string surveyId,
            AgentRequirementSubmission submission,
            CancellationToken cancellationToken = default)
        {
            SubmittedProjectId = projectId;
            SubmittedSurveyId = surveyId;
            var current = Surveys.Single(value => value.Id == surveyId);
            var updated = current with
            {
                Status = AgentRequirementSurveyStatus.Submitted,
                Submission = submission,
                SubmittedAtUnixMs = current.CreatedAtUnixMs + 1,
                ResolvedAtUnixMs = current.CreatedAtUnixMs + 1,
            };
            Surveys = Surveys.Select(value => value.Id == surveyId ? updated : value).ToArray();
            Changed?.Invoke(this, new ProjectRequirementSurveyChangedEventArgs(
                ownerUserId, projectId));
            return Task.FromResult(updated);
        }
    }
}
