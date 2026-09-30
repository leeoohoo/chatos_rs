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

    private sealed class SurveyHost : ILocalAgentHostClient
    {
        private WindowsLocalRequirementSurvey _survey = CreateSurvey();

        public object? LastCommand { get; private set; }
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
                _ => throw new InvalidOperationException(
                    $"Unexpected Requirement Survey command: {typeof(TCommand).Name}"),
            };
            return Task.FromResult((TResponse)response);
        }

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
