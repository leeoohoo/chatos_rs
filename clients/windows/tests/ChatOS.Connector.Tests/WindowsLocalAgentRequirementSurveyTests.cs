using System.Text.Json;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentRequirementSurveyTests
{
    [Fact]
    public async Task ListsAndResolvesSurveyThroughLocalAgentHost()
    {
        var host = new SurveyHost();
        var service = new WindowsLocalAgentRequirementSurveyService(host);
        ProjectRequirementSurveyChangedEventArgs? changed = null;
        service.Changed += (_, args) => changed = args;

        var listed = await service.ListAsync("user-1", "project-1");
        var survey = Assert.Single(listed);
        Assert.Equal(AgentRequirementQuestionKind.Text, survey.Draft.Questions[1].Kind);
        Assert.Equal(AgentRequirementQuestionKind.Boolean, survey.Draft.Questions[2].Kind);

        var resolved = await service.SubmitAsync(
            "user-1",
            "project-1",
            survey.Id,
            new AgentRequirementSubmission([
                new("platform", ["local"]),
                new("detail", [], "Keep data local"),
                new("confirm", [], BooleanValue: true),
            ]));

        Assert.Equal(AgentRequirementSurveyStatus.Submitted, resolved.Status);
        Assert.Equal("Keep data local", resolved.Submission!.Answers[1].TextValue);
        Assert.Equal(true, resolved.Submission.Answers[2].BooleanValue);
        Assert.Equal("project-1", changed?.ProjectId);
        var command = Assert.IsType<ResolveLocalRequirementSurveyCommand>(host.LastCommand);
        Assert.Equal((ulong)1, command.ExpectedVersion);
        Assert.Equal("local", command.Answers["platform"].GetString());
        Assert.Equal("Keep data local", command.Answers["detail"].GetString());
        Assert.True(command.Answers["confirm"].GetBoolean());
    }

    [Fact]
    public async Task ReadsAndConfirmsExactGeneratedPlanRevision()
    {
        var host = new SurveyHost();
        var service = new WindowsLocalAgentRequirementSurveyService(host);
        var survey = Assert.Single(await service.ListAsync("user-1", "project-1"));
        await service.SubmitAsync("user-1", "project-1", survey.Id,
            new AgentRequirementSubmission([
                new("platform", ["local"]),
                new("detail", [], "Keep data local"),
                new("confirm", [], BooleanValue: true),
            ]));

        var details = await service.GetDetailsAsync("user-1", "project-1", "survey-1");

        Assert.Equal((ulong)2, details.Plan!.Revision);
        Assert.Equal("# Deployment plan", details.Plan.Markdown);
        Assert.Equal(AgentRequirementPlanFollowUpState.Succeeded,
            details.PlanFollowUp!.State);

        var confirmed = await service.ConfirmPlanAsync(
            "user-1", "project-1", "survey-1", 2);

        Assert.NotNull(confirmed.ConfirmedAtUnixMs);
        var command = Assert.IsType<ConfirmLocalRequirementPlanCommand>(host.LastCommand);
        Assert.Equal((ulong)2, command.ExpectedRevision);
        Assert.Equal(0, host.StartedTaskCount);
    }

    [Fact]
    public async Task RefusesToConfirmPlanWhenItsSourceRunFailed()
    {
        var host = new SurveyHost { PlanRunStatus = "failed" };
        var service = new WindowsLocalAgentRequirementSurveyService(host);
        var survey = Assert.Single(await service.ListAsync("user-1", "project-1"));
        await service.SubmitAsync("user-1", "project-1", survey.Id,
            new AgentRequirementSubmission([
                new("platform", ["local"]),
                new("detail", [], "Keep data local"),
                new("confirm", [], BooleanValue: true),
            ]));

        var details = await service.GetDetailsAsync("user-1", "project-1", survey.Id);
        Assert.Equal(AgentRequirementPlanFollowUpState.Failed, details.PlanFollowUp!.State);

        var error = await Assert.ThrowsAsync<InvalidOperationException>(() =>
            service.ConfirmPlanAsync("user-1", "project-1", survey.Id, 2));
        Assert.Contains("尚未成功", error.Message);
        Assert.Equal(0, host.ConfirmCommandCount);
    }

    [Fact]
    public async Task AdoptsOnlyTheExactSuccessfulLegacyReplyBeforeConfirmation()
    {
        var host = new SurveyHost(planInitiallyMissing: true);
        var service = new WindowsLocalAgentRequirementSurveyService(host);
        var survey = Assert.Single(await service.ListAsync("user-1", "project-1"));
        await service.SubmitAsync("user-1", "project-1", survey.Id,
            new AgentRequirementSubmission([
                new("platform", ["local"]),
                new("detail", [], "Keep data local"),
                new("confirm", [], BooleanValue: true),
            ]));

        var details = await service.GetDetailsAsync("user-1", "project-1", survey.Id);
        Assert.Null(details.Plan);
        Assert.Equal(AgentRequirementPlanFollowUpState.Succeeded,
            details.SurveyFollowUp.State);
        Assert.Equal("# Deployment plan", details.SurveyFollowUp.Markdown);

        var mismatch = await Assert.ThrowsAsync<InvalidOperationException>(() =>
            service.SaveSourceReplyAsPlanAsync(
                "user-1", "project-1", survey.Id, "# Different plan"));
        Assert.Contains("准确成功", mismatch.Message);
        Assert.Equal(0, host.SaveCommandCount);

        var saved = await service.SaveSourceReplyAsPlanAsync(
            "user-1", "project-1", survey.Id, details.SurveyFollowUp.Markdown!);
        Assert.Equal((ulong)1, saved.Revision);
        Assert.Null(saved.ConfirmedAtUnixMs);
        var save = Assert.IsType<SaveLocalRequirementPlanCommand>(host.LastCommand);
        Assert.Equal((ulong)0, save.ExpectedRevision);
        Assert.Equal("run-1", save.SourceRunId);

        var confirmed = await service.ConfirmPlanAsync(
            "user-1", "project-1", survey.Id, saved.Revision);
        Assert.NotNull(confirmed.ConfirmedAtUnixMs);
        Assert.Equal(1, host.SaveCommandCount);
        Assert.Equal(1, host.ConfirmCommandCount);
        Assert.Equal(0, host.StartedTaskCount);
    }

    [Fact]
    public async Task RefusesToAdoptALegacyReplyFromAFailedRun()
    {
        var host = new SurveyHost(planInitiallyMissing: true) { SurveyRunStatus = "failed" };
        var service = new WindowsLocalAgentRequirementSurveyService(host);
        var survey = Assert.Single(await service.ListAsync("user-1", "project-1"));
        await service.SubmitAsync("user-1", "project-1", survey.Id,
            new AgentRequirementSubmission([
                new("platform", ["local"]),
                new("detail", [], "Keep data local"),
                new("confirm", [], BooleanValue: true),
            ]));

        var details = await service.GetDetailsAsync("user-1", "project-1", survey.Id);
        Assert.Equal(AgentRequirementPlanFollowUpState.Failed,
            details.SurveyFollowUp.State);
        var error = await Assert.ThrowsAsync<InvalidOperationException>(() =>
            service.SaveSourceReplyAsPlanAsync(
                "user-1", "project-1", survey.Id, "# Deployment plan"));
        Assert.Contains("准确成功", error.Message);
        Assert.Equal(0, host.SaveCommandCount);
        Assert.Equal(0, host.StartedTaskCount);
    }

    private sealed class SurveyHost : ILocalAgentHostClient
    {
        private WindowsLocalRequirementSurvey _survey = CreateSurvey();
        private WindowsLocalRequirementPlan? _plan;

        public SurveyHost(bool planInitiallyMissing = false) =>
            _plan = planInitiallyMissing ? null : CreatePlan();

        public object? LastCommand { get; private set; }
        public int StartedTaskCount { get; private set; }
        public int ConfirmCommandCount { get; private set; }
        public int SaveCommandCount { get; private set; }
        public string PlanRunStatus { get; init; } = "succeeded";
        public string SurveyRunStatus { get; init; } = "succeeded";
        public string? ActiveOwnerUserId => "user-1";

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task RestartForOwnerAsync(
            string ownerUserId,
            IReadOnlyDictionary<string, string> credentialEnvironment,
            CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task StopAsync(CancellationToken cancellationToken = default) => Task.CompletedTask;

        public Task<TResponse> SendAsync<TCommand, TResponse>(
            TCommand command,
            CancellationToken cancellationToken = default)
            where TCommand : notnull
        {
            LastCommand = command;
            object response = command switch
            {
                ListLocalRequirementSurveysCommand =>
                    new LocalRequirementSurveysResult("requirement_surveys", [_survey]),
                GetLocalRequirementSurveyCommand =>
                    new LocalRequirementSurveyResult("requirement_survey", _survey),
                ResolveLocalRequirementSurveyCommand resolve => Resolve(resolve),
                GetLocalRequirementPlanCommand get => Plan(get.Revision),
                SaveLocalRequirementPlanCommand save => Save(save),
                ConfirmLocalRequirementPlanCommand confirm => Confirm(confirm),
                GetLocalRunCommand get => Run(get.RunId),
                _ => throw new InvalidOperationException(
                    $"Unexpected Requirement Survey command: {typeof(TCommand).Name}"),
            };
            return Task.FromResult((TResponse)response);
        }

        private LocalRequirementPlanResult Plan(ulong? revision) => new(
            "requirement_plan",
            _plan is not null && (revision is null || revision == _plan.Revision) ? _plan : null);

        private LocalRequirementPlanResult Save(SaveLocalRequirementPlanCommand command)
        {
            if (_plan is not null || command.ExpectedRevision != 0 ||
                command.SourceRunId != "run-1" || command.Markdown != "# Deployment plan")
                throw new InvalidOperationException("stale");
            SaveCommandCount++;
            _plan = new(
                "requirement-plan:survey-1", 1, "user-1", "project-1", "conversation-1",
                "survey-1", "run-1", "Deployment plan", "# Deployment plan", 2_000, null);
            return new("requirement_plan", _plan);
        }

        private LocalRequirementPlanResult Confirm(
            ConfirmLocalRequirementPlanCommand command)
        {
            if (_plan is null || command.ExpectedRevision != _plan.Revision)
                throw new InvalidOperationException("stale");
            ConfirmCommandCount++;
            _plan = _plan with { ConfirmedAtUnixMs = 3_000 };
            return new("requirement_plan", _plan);
        }

        private GetLocalRunResult Run(string runId)
        {
            var taskSource = runId == "run-1";
            var input = taskSource
                ? JsonSerializer.SerializeToElement(new { source_conversation_id = "conversation-1" })
                : JsonSerializer.SerializeToElement(new { conversation_id = "conversation-1" });
            var run = new WindowsLocalAgentRun(runId, "user-1",
                taskSource ? "task" : "conversation_turn",
                taskSource ? "task-1" : "turn-1",
                taskSource ? "task_execution" : "main_chat", input,
                taskSource ? SurveyRunStatus : PlanRunStatus, 1,
                JsonSerializer.SerializeToElement(new { content = "# Deployment plan" }),
                1_000, 2_000);
            return new("run", run);
        }

        private static WindowsLocalRequirementPlan CreatePlan() => new(
            "requirement-plan:survey-1", 2, "user-1", "project-1", "conversation-1",
            "survey-1", "run-plan", "Deployment plan", "# Deployment plan", 2_000, null);

        private LocalRequirementSurveyResolvedResult Resolve(
            ResolveLocalRequirementSurveyCommand command)
        {
            _survey = _survey with
            {
                Answers = command.Answers,
                Status = "resolved",
                Version = 2,
                UpdatedAtUnixMs = 2_000,
                ResolvedAtUnixMs = 2_000,
            };
            return new("requirement_survey_resolved",
                new(_survey, JsonSerializer.SerializeToElement(new { status = "continuation_ready" })));
        }

        private static WindowsLocalRequirementSurvey CreateSurvey() => new(
            "survey-1",
            "user-1",
            "project-1",
            "conversation-1",
            "run-1",
            "task-1",
            "Deployment",
            "Confirm requirements",
            [
                new("platform", "Platform?", "single_choice", true, ["local", "cloud"]),
                new("detail", "Details?", "text", true, []),
                new("confirm", "Proceed?", "boolean", true, []),
            ],
            null,
            "open",
            1,
            1_000,
            1_000,
            null);
    }
}
