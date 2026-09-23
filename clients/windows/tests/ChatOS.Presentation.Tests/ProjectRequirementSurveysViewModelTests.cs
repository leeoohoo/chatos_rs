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
        var service = new StubAgentTeamService
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
        Assert.Equal(1, viewModel.AwaitingResolutionCount);
        Assert.Equal(1, viewModel.ResolvedCount);
        Assert.True(viewModel.SelectedSurvey!.CanSubmit);
        Assert.Contains("Human", viewModel.SelectedSurvey.PermissionText);
    }

    [Fact]
    public async Task SubmitUsesProjectScopeAndLocksHumanAnswers()
    {
        var pending = Survey("pending", AgentRequirementSurveyStatus.Pending, 100);
        var service = new StubAgentTeamService { Surveys = [pending] };
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
        Assert.Equal(1, viewModel.AwaitingResolutionCount);
        Assert.False(viewModel.SelectedSurvey!.CanSubmit);
        Assert.Equal("范围确认完毕", viewModel.SelectedSurvey.SubmissionNotes);
        Assert.Contains("只读", viewModel.SelectedSurvey.PermissionText);
    }

    [Fact]
    public async Task RefreshFailureProducesRecoverableErrorState()
    {
        var service = new StubAgentTeamService { ListError = new IOException("database busy") };
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

    private sealed class StubAgentTeamService : IAgentTeamService
    {
        public event EventHandler<AgentTeamChangedEventArgs>? Changed;

        public IReadOnlyList<AgentRequirementSurvey> Surveys { get; set; } = [];
        public Exception? ListError { get; set; }
        public string? SubmittedProjectId { get; private set; }
        public string? SubmittedSurveyId { get; private set; }

        public Task<IReadOnlyList<AgentRequirementSurvey>> ListProjectRequirementSurveysAsync(
            string ownerUserId,
            string projectId,
            CancellationToken cancellationToken = default) =>
            ListError is null
                ? Task.FromResult(Surveys)
                : Task.FromException<IReadOnlyList<AgentRequirementSurvey>>(ListError);

        public Task<AgentRequirementSurvey> SubmitProjectRequirementSurveyAsync(
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
            };
            Surveys = Surveys.Select(value => value.Id == surveyId ? updated : value).ToArray();
            Changed?.Invoke(this, new AgentTeamChangedEventArgs(
                ownerUserId, projectId, null, "requirement_survey_submitted"));
            return Task.FromResult(updated);
        }

        public Task<IReadOnlyList<AgentProfile>> ListAgentsAsync(string ownerUserId,
            bool includeArchived = false, CancellationToken cancellationToken = default) => Unsupported<IReadOnlyList<AgentProfile>>();
        public Task<AgentProfile> SaveAgentAsync(string ownerUserId, string? agentId,
            AgentProfileDraft draft, CancellationToken cancellationToken = default) => Unsupported<AgentProfile>();
        public Task ArchiveAgentAsync(string ownerUserId, string agentId,
            CancellationToken cancellationToken = default) => Unsupported();
        public Task<IReadOnlyList<AgentRoom>> ListRoomsAsync(string ownerUserId, string projectId,
            CancellationToken cancellationToken = default) => Unsupported<IReadOnlyList<AgentRoom>>();
        public Task<AgentRoom> CreateTeamAsync(string ownerUserId, string projectId,
            AgentRoomDraft draft, string projectManagerAgentId,
            CancellationToken cancellationToken = default) => Unsupported<AgentRoom>();
        public Task<AgentRoom> OpenDirectAsync(string ownerUserId, string agentId,
            CancellationToken cancellationToken = default) => Unsupported<AgentRoom>();
        public Task<AgentRoomMember> AddMemberAsync(string ownerUserId, string roomId,
            string agentId, AgentRoomMemberDraft draft,
            CancellationToken cancellationToken = default) => Unsupported<AgentRoomMember>();
        public Task RemoveMemberAsync(string ownerUserId, string roomId, string agentId,
            CancellationToken cancellationToken = default) => Unsupported();
        public Task<AgentRoom> ConfigureTeamAsync(string ownerUserId, string roomId,
            AgentRoomDraft draft, string? defaultAgentId, string projectManagerAgentId,
            CancellationToken cancellationToken = default) => Unsupported<AgentRoom>();
        public Task<AgentTeamSnapshot> LoadSnapshotAsync(string ownerUserId, string roomId,
            CancellationToken cancellationToken = default) => Unsupported<AgentTeamSnapshot>();
        public Task<AgentPostResult> PostHumanMessageAsync(string ownerUserId, string roomId,
            string content, IReadOnlyList<string>? mentionedAgentIds = null,
            IReadOnlyList<AgentMessageAttachment>? attachments = null,
            CancellationToken cancellationToken = default) => Unsupported<AgentPostResult>();
        public Task<AgentMessageAttachment?> GetMessageAttachmentAsync(string ownerUserId,
            string roomId, string attachmentId, CancellationToken cancellationToken = default) =>
            Unsupported<AgentMessageAttachment?>();
        public Task<AgentTodo> CreateTodoAsync(string ownerUserId, string managerAgentId,
            AgentTodoDraft draft, CancellationToken cancellationToken = default) => Unsupported<AgentTodo>();
        public Task<AgentTodo> UpdateTodoAsync(string ownerUserId, string actingAgentId,
            string todoId, long expectedRevision, AgentTodoStatus status, string result,
            string? assignedAgentId = null, CancellationToken cancellationToken = default) => Unsupported<AgentTodo>();
        public Task<IReadOnlyList<AgentTodo>> ReorderTodosAsync(string ownerUserId,
            string managerAgentId, string roomId, IReadOnlyList<string> todoIds,
            CancellationToken cancellationToken = default) => Unsupported<IReadOnlyList<AgentTodo>>();
        public Task<IReadOnlyList<AgentTodoProgress>> ListTodoProgressAsync(string ownerUserId,
            string todoId, CancellationToken cancellationToken = default) => Unsupported<IReadOnlyList<AgentTodoProgress>>();
        public Task<AgentTeamAsset> SaveAssetAsync(string ownerUserId, string roomId,
            string? assetId, string? editorAgentId, AgentTeamAssetCategory category, string title,
            string markdown, int? expectedRevision, CancellationToken cancellationToken = default) => Unsupported<AgentTeamAsset>();
        public Task ArchiveAssetAsync(string ownerUserId, string roomId, string assetId,
            string? editorAgentId, int expectedRevision,
            CancellationToken cancellationToken = default) => Unsupported();
        public Task<AgentRequirementSurvey> SubmitRequirementSurveyAsync(string ownerUserId,
            string roomId, string surveyId, AgentRequirementSubmission submission,
            CancellationToken cancellationToken = default) => Unsupported<AgentRequirementSurvey>();
        public Task<AgentStaffingProposal> ResolveStaffingProposalAsync(string ownerUserId,
            string roomId, string proposalId, bool approve,
            CancellationToken cancellationToken = default) => Unsupported<AgentStaffingProposal>();
        public Task DrainAsync(string ownerUserId,
            CancellationToken cancellationToken = default) => Unsupported();

        private static Task Unsupported() => Task.FromException(new NotSupportedException());
        private static Task<T> Unsupported<T>() => Task.FromException<T>(new NotSupportedException());
    }
}
