using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

internal sealed record WindowsLocalRequirementSurveyQuestion(
    string QuestionId,
    string Prompt,
    string ResponseKind,
    bool Required,
    IReadOnlyList<string> Options);

internal sealed record WindowsLocalRequirementSurvey(
    string SurveyId,
    string OwnerUserId,
    string ProjectResourceId,
    string SourceConversationId,
    string SourceRunId,
    string? SourceTaskId,
    string Title,
    string? Description,
    IReadOnlyList<WindowsLocalRequirementSurveyQuestion> Questions,
    IReadOnlyDictionary<string, JsonElement>? Answers,
    string Status,
    ulong Version,
    long CreatedAtUnixMs,
    long UpdatedAtUnixMs,
    long? ResolvedAtUnixMs);

internal sealed record ListLocalRequirementSurveysCommand(
    string Type,
    string OwnerUserId,
    string? ProjectResourceId,
    string? Status,
    uint Limit);

internal sealed record GetLocalRequirementSurveyCommand(
    string Type,
    string OwnerUserId,
    string SurveyId);

internal sealed record ResolveLocalRequirementSurveyCommand(
    string Type,
    string OwnerUserId,
    string SurveyId,
    ulong ExpectedVersion,
    IReadOnlyDictionary<string, JsonElement> Answers);

internal sealed record LocalRequirementSurveysResult(
    string Type,
    IReadOnlyList<WindowsLocalRequirementSurvey> Surveys);

internal sealed record LocalRequirementSurveyResult(
    string Type,
    WindowsLocalRequirementSurvey Survey);

internal sealed record LocalRequirementSurveyResolution(
    WindowsLocalRequirementSurvey Survey,
    JsonElement ResumedRun);

internal sealed record LocalRequirementSurveyResolvedResult(
    string Type,
    LocalRequirementSurveyResolution Resolution);

public sealed class WindowsLocalAgentRequirementSurveyService(ILocalAgentHostClient host)
    : IProjectRequirementSurveyService
{
    public event EventHandler<ProjectRequirementSurveyChangedEventArgs>? Changed;

    public async Task<IReadOnlyList<AgentRequirementSurvey>> ListAsync(
        string ownerUserId,
        string projectId,
        CancellationToken cancellationToken = default)
    {
        var result = await host.SendAsync<
            ListLocalRequirementSurveysCommand,
            LocalRequirementSurveysResult>(
                new("list_requirement_surveys", ownerUserId, projectId, null, 200),
                cancellationToken).ConfigureAwait(false);
        if (result.Type != "requirement_surveys")
            throw new InvalidDataException("Invalid Requirement Survey list result.");
        return result.Surveys.Select(Map).ToArray();
    }

    public async Task<AgentRequirementSurvey> SubmitAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        AgentRequirementSubmission submission,
        CancellationToken cancellationToken = default)
    {
        var current = await GetAsync(ownerUserId, surveyId, cancellationToken)
            .ConfigureAwait(false);
        if (current.ProjectResourceId != projectId || current.Status != "open")
            throw new InvalidOperationException("这张调研已不能提交，请刷新后重试。");
        var domain = Map(current);
        AgentRequirementSurvey.ValidateSubmission(submission, domain.Draft.Questions);
        var answers = EncodeAnswers(current.Questions, submission);
        var result = await host.SendAsync<
            ResolveLocalRequirementSurveyCommand,
            LocalRequirementSurveyResolvedResult>(new(
                "resolve_requirement_survey",
                ownerUserId,
                surveyId,
                current.Version,
                answers), cancellationToken).ConfigureAwait(false);
        if (result.Type != "requirement_survey_resolved")
            throw new InvalidDataException("Invalid Requirement Survey resolution result.");
        var resolved = Map(result.Resolution.Survey);
        Changed?.Invoke(this, new(ownerUserId, projectId));
        return resolved;
    }

    private async Task<WindowsLocalRequirementSurvey> GetAsync(
        string ownerUserId,
        string surveyId,
        CancellationToken cancellationToken)
    {
        var result = await host.SendAsync<
            GetLocalRequirementSurveyCommand,
            LocalRequirementSurveyResult>(
                new("get_requirement_survey", ownerUserId, surveyId),
                cancellationToken).ConfigureAwait(false);
        return result.Type == "requirement_survey" ? result.Survey
            : throw new InvalidDataException("Invalid Requirement Survey result.");
    }

    private static IReadOnlyDictionary<string, JsonElement> EncodeAnswers(
        IReadOnlyList<WindowsLocalRequirementSurveyQuestion> questions,
        AgentRequirementSubmission submission)
    {
        var byId = submission.Answers.ToDictionary(value => value.QuestionId,
            StringComparer.Ordinal);
        var answers = new Dictionary<string, JsonElement>(StringComparer.Ordinal);
        foreach (var question in questions)
        {
            if (!byId.TryGetValue(question.QuestionId, out var answer)) continue;
            answers[question.QuestionId] = question.ResponseKind switch
            {
                "text" => JsonSerializer.SerializeToElement(answer.TextValue),
                "boolean" => JsonSerializer.SerializeToElement(answer.BooleanValue),
                "single_choice" => JsonSerializer.SerializeToElement(
                    answer.SelectedOptionIds.Single()),
                "multiple_choice" => JsonSerializer.SerializeToElement(
                    answer.SelectedOptionIds),
                _ => throw new InvalidDataException("Unknown Requirement Survey question kind."),
            };
        }
        return answers;
    }

    private static AgentRequirementSurvey Map(WindowsLocalRequirementSurvey survey)
    {
        var status = survey.Status switch
        {
            "open" => AgentRequirementSurveyStatus.Pending,
            "resolved" => AgentRequirementSurveyStatus.Submitted,
            _ => throw new InvalidDataException("Unknown Requirement Survey status."),
        };
        var questions = survey.Questions.Select(question => new AgentRequirementQuestion(
            question.QuestionId,
            question.Prompt,
            ParseKind(question.ResponseKind),
            question.Options.Select(option => new AgentRequirementOption(option, option)).ToArray(),
            question.Required)).ToArray();
        var submission = survey.Answers is null ? null : new AgentRequirementSubmission(
            survey.Questions.Select(question => DecodeAnswer(question, survey.Answers))
                .Where(answer => answer is not null)
                .Cast<AgentRequirementAnswer>()
                .ToArray());
        return new AgentRequirementSurvey(
            survey.SurveyId,
            survey.OwnerUserId,
            survey.ProjectResourceId,
            survey.SourceTaskId ?? "local-task",
            survey.SourceRunId,
            survey.SurveyId,
            new AgentRequirementSurveyDraft(
                survey.Title,
                string.IsNullOrWhiteSpace(survey.Description)
                    ? "本地任务需要补充项目要求。"
                    : survey.Description,
                questions),
            status,
            submission,
            null,
            survey.CreatedAtUnixMs,
            survey.ResolvedAtUnixMs,
            survey.ResolvedAtUnixMs);
    }

    private static AgentRequirementAnswer? DecodeAnswer(
        WindowsLocalRequirementSurveyQuestion question,
        IReadOnlyDictionary<string, JsonElement> answers)
    {
        if (!answers.TryGetValue(question.QuestionId, out var answer)) return null;
        return question.ResponseKind switch
        {
            "text" => new(question.QuestionId, [], answer.GetString() ?? string.Empty),
            "boolean" => new(question.QuestionId, [], BooleanValue: answer.GetBoolean()),
            "single_choice" => new(question.QuestionId, [answer.GetString() ?? string.Empty]),
            "multiple_choice" => new(question.QuestionId,
                answer.EnumerateArray().Select(value => value.GetString() ?? string.Empty).ToArray()),
            _ => throw new InvalidDataException("Unknown Requirement Survey question kind."),
        };
    }

    private static AgentRequirementQuestionKind ParseKind(string kind) => kind switch
    {
        "text" => AgentRequirementQuestionKind.Text,
        "single_choice" => AgentRequirementQuestionKind.SingleChoice,
        "multiple_choice" => AgentRequirementQuestionKind.MultipleChoice,
        "boolean" => AgentRequirementQuestionKind.Boolean,
        _ => throw new InvalidDataException("Unknown Requirement Survey question kind."),
    };
}
