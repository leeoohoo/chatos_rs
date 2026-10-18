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

    [Fact]
    public async Task LoadsVersionedPlanAndConfirmsExactRevisionWithoutStartingWork()
    {
        var submitted = Survey("submitted", AgentRequirementSurveyStatus.Submitted, 100);
        var plan = new AgentRequirementPlan(
            "requirement-plan:submitted", 3, "owner", "project", "conversation",
            "submitted", "run-plan", "方案 V3", "# 方案\n\n保持边界。", 200, null);
        var service = new StubRequirementSurveyService
        {
            Surveys = [submitted],
            Details = new(plan,
                new(AgentRequirementPlanFollowUpState.Succeeded, "old reply"),
                new(AgentRequirementPlanFollowUpState.Succeeded, plan.Markdown)),
        };
        using var viewModel = new ProjectRequirementSurveysViewModel(
            service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync("owner", Project());
        await WaitUntilAsync(() => viewModel.SelectedSurvey?.Details is not null);

        Assert.Equal("方案版本 3", viewModel.SelectedSurvey!.PlanVersionText);
        Assert.True(viewModel.SelectedSurvey.CanConfirmPlan);
        Assert.True(await viewModel.ConfirmSelectedPlanAsync());
        Assert.Equal((ulong)3, service.ConfirmedRevision);
        Assert.True(viewModel.SelectedSurvey.IsPlanConfirmed);
        Assert.Contains("不会自动执行", viewModel.StatusMessage);
    }

    [Fact]
    public async Task ExplicitlyAdoptsAndConfirmsAnExactLegacyFinalReply()
    {
        var submitted = Survey("submitted", AgentRequirementSurveyStatus.Submitted, 100);
        var service = new StubRequirementSurveyService
        {
            Surveys = [submitted],
            Details = new(null,
                new(AgentRequirementPlanFollowUpState.Succeeded, "# 历史方案\n\n保持边界。"),
                null),
        };
        using var viewModel = new ProjectRequirementSurveysViewModel(
            service, new ImmediateUiDispatcher());

        await viewModel.OpenAsync("owner", Project());
        await WaitUntilAsync(() => viewModel.SelectedSurvey?.Details is not null);

        Assert.True(viewModel.SelectedSurvey!.HasLegacyPlanCandidate);
        Assert.True(viewModel.SelectedSurvey.CanAdoptLegacyPlan);
        Assert.Equal("# 历史方案\n\n保持边界。",
            viewModel.SelectedSurvey.LegacyPlanMarkdown);
        Assert.True(await viewModel.AdoptAndConfirmSelectedPlanAsync());
        Assert.Equal("# 历史方案\n\n保持边界。", service.AdoptedMarkdown);
        Assert.Equal((ulong)1, service.ConfirmedRevision);
        Assert.True(viewModel.SelectedSurvey.IsPlanConfirmed);
        Assert.False(viewModel.SelectedSurvey.HasLegacyPlanCandidate);
        Assert.Contains("不会自动执行", viewModel.StatusMessage);
    }

    private static WorkspaceProject Project() =>
        new("project", "Parity", null, null, null);

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        for (var attempt = 0; attempt < 100 && !condition(); attempt++)
            await Task.Delay(10);
        Assert.True(condition());
    }

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
        public ulong? ConfirmedRevision { get; private set; }
        public string? AdoptedMarkdown { get; private set; }
        public AgentRequirementSurveyDetails Details { get; set; } = new(
            null, new(AgentRequirementPlanFollowUpState.Processing), null);

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

        public Task<AgentRequirementSurveyDetails> GetDetailsAsync(
            string ownerUserId,
            string projectId,
            string surveyId,
            CancellationToken cancellationToken = default) => Task.FromResult(Details);

        public Task<AgentRequirementPlan> ConfirmPlanAsync(
            string ownerUserId,
            string projectId,
            string surveyId,
            ulong expectedRevision,
            CancellationToken cancellationToken = default)
        {
            ConfirmedRevision = expectedRevision;
            var plan = Details.Plan ?? throw new InvalidOperationException();
            var confirmed = plan with { ConfirmedAtUnixMs = 300 };
            Details = Details with { Plan = confirmed };
            Changed?.Invoke(this, new(ownerUserId, projectId));
            return Task.FromResult(confirmed);
        }

        public Task<AgentRequirementPlan> SaveSourceReplyAsPlanAsync(
            string ownerUserId,
            string projectId,
            string surveyId,
            string markdown,
            CancellationToken cancellationToken = default)
        {
            AdoptedMarkdown = markdown;
            var plan = new AgentRequirementPlan(
                $"requirement-plan:{surveyId}", 1, ownerUserId, projectId, "conversation",
                surveyId, "run-legacy", "历史方案", markdown.Trim(), 200, null);
            Details = Details with
            {
                Plan = plan,
                PlanFollowUp = new(AgentRequirementPlanFollowUpState.Succeeded, plan.Markdown),
            };
            Changed?.Invoke(this, new(ownerUserId, projectId));
            return Task.FromResult(plan);
        }
    }
}
