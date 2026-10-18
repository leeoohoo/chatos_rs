using System.Text;
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

internal sealed record WindowsLocalRequirementPlan(
    string PlanId,
    ulong Revision,
    string OwnerUserId,
    string ProjectResourceId,
    string SourceConversationId,
    string SourceSurveyId,
    string SourceRunId,
    string Title,
    string Markdown,
    long CreatedAtUnixMs,
    long? ConfirmedAtUnixMs);

internal sealed record GetLocalRequirementPlanCommand(
    string Type,
    string OwnerUserId,
    string SurveyId,
    ulong? Revision = null);

internal sealed record ConfirmLocalRequirementPlanCommand(
    string Type,
    string OwnerUserId,
    string SurveyId,
    ulong ExpectedRevision);

internal sealed record SaveLocalRequirementPlanCommand(
    string Type,
    string OwnerUserId,
    string SurveyId,
    string SourceRunId,
    ulong ExpectedRevision,
    string Markdown);

internal sealed record LocalRequirementPlanResult(
    string Type,
    WindowsLocalRequirementPlan? Plan);

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

    public async Task<AgentRequirementSurveyDetails> GetDetailsAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        CancellationToken cancellationToken = default)
    {
        var survey = await GetAsync(ownerUserId, surveyId, cancellationToken)
            .ConfigureAwait(false);
        ValidateScope(survey, ownerUserId, projectId);
        var surveyFollowUp = await GetFollowUpAsync(
            ownerUserId, survey, survey.SourceRunId, cancellationToken).ConfigureAwait(false);
        var result = await host.SendAsync<
            GetLocalRequirementPlanCommand,
            LocalRequirementPlanResult>(
                new("get_requirement_plan", ownerUserId, surveyId), cancellationToken)
            .ConfigureAwait(false);
        if (result.Type != "requirement_plan")
            throw new InvalidDataException("Invalid Requirement Plan result.");
        if (result.Plan is null)
            return new(null, surveyFollowUp, null);

        var plan = MapPlan(result.Plan, survey, ownerUserId, projectId);
        var planFollowUp = await GetPlanFollowUpAsync(
            ownerUserId, plan, cancellationToken).ConfigureAwait(false);
        return new(plan, surveyFollowUp, planFollowUp);
    }

    public async Task<AgentRequirementPlan> ConfirmPlanAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        ulong expectedRevision,
        CancellationToken cancellationToken = default)
    {
        if (expectedRevision == 0)
            throw new ArgumentOutOfRangeException(nameof(expectedRevision));
        var survey = await GetAsync(ownerUserId, surveyId, cancellationToken)
            .ConfigureAwait(false);
        ValidateScope(survey, ownerUserId, projectId);
        var beforeResult = await host.SendAsync<
            GetLocalRequirementPlanCommand,
            LocalRequirementPlanResult>(
                new("get_requirement_plan", ownerUserId, surveyId, expectedRevision),
                cancellationToken).ConfigureAwait(false);
        if (beforeResult is not { Type: "requirement_plan", Plan: not null })
            throw new InvalidOperationException("方案版本已变化，请刷新后重新查看当前方案。");
        var before = MapPlan(beforeResult.Plan, survey, ownerUserId, projectId);
        if (before.Revision != expectedRevision || before.ConfirmedAtUnixMs is not null)
            throw new InvalidOperationException("方案版本已变化，请刷新后重新查看当前方案。");
        var followUp = await GetPlanFollowUpAsync(
            ownerUserId, before, cancellationToken).ConfigureAwait(false);
        if (followUp.State is not (AgentRequirementPlanFollowUpState.Succeeded or
            AgentRequirementPlanFollowUpState.SucceededWithoutReply))
            throw new InvalidOperationException("方案生成尚未成功，当前版本不能确认。");

        var result = await host.SendAsync<
            ConfirmLocalRequirementPlanCommand,
            LocalRequirementPlanResult>(new(
                "confirm_requirement_plan", ownerUserId, surveyId, expectedRevision),
                cancellationToken).ConfigureAwait(false);
        if (result is not { Type: "requirement_plan", Plan: not null })
            throw new InvalidDataException("Invalid Requirement Plan confirmation result.");
        var confirmed = MapPlan(result.Plan, survey, ownerUserId, projectId);
        if (confirmed.PlanId != before.PlanId || confirmed.Revision != before.Revision ||
            confirmed.Markdown != before.Markdown || confirmed.Title != before.Title ||
            confirmed.SourceRunId != before.SourceRunId ||
            confirmed.SourceConversationId != before.SourceConversationId ||
            confirmed.CreatedAtUnixMs != before.CreatedAtUnixMs ||
            confirmed.ConfirmedAtUnixMs is null)
            throw new InvalidDataException("Requirement Plan changed while it was confirmed.");
        Changed?.Invoke(this, new(ownerUserId, projectId));
        return confirmed;
    }

    public async Task<AgentRequirementPlan> SaveSourceReplyAsPlanAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        string markdown,
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(markdown) ||
            Encoding.UTF8.GetByteCount(markdown) > 64 * 1024)
            throw new ArgumentException("历史方案正文为空或超过 64 KiB。", nameof(markdown));

        var survey = await GetAsync(ownerUserId, surveyId, cancellationToken)
            .ConfigureAwait(false);
        ValidateScope(survey, ownerUserId, projectId);
        if (survey.Status != "resolved")
            throw new InvalidOperationException("调研尚未提交，不能保存历史方案。");

        // Reading a legacy reply is deliberately side-effect free. Only this
        // explicit Human UI action may adopt the exact successful source reply.
        var followUp = await GetFollowUpAsync(
            ownerUserId, survey, survey.SourceRunId, cancellationToken).ConfigureAwait(false);
        if (followUp.State != AgentRequirementPlanFollowUpState.Succeeded ||
            !string.Equals(followUp.Markdown, markdown, StringComparison.Ordinal))
            throw new InvalidOperationException("只有准确成功 Run 的最终回复才能保存为方案。");

        var result = await host.SendAsync<
            SaveLocalRequirementPlanCommand,
            LocalRequirementPlanResult>(new(
                "save_requirement_plan",
                ownerUserId,
                surveyId,
                survey.SourceRunId,
                0,
                markdown), cancellationToken).ConfigureAwait(false);
        if (result is not { Type: "requirement_plan", Plan: not null })
            throw new InvalidDataException("Invalid Requirement Plan save result.");

        var saved = MapPlan(result.Plan, survey, ownerUserId, projectId);
        if (saved.Revision != 1 ||
            saved.SourceRunId != survey.SourceRunId ||
            saved.SourceConversationId != survey.SourceConversationId ||
            saved.Markdown != markdown.Trim() ||
            saved.ConfirmedAtUnixMs is not null)
            throw new InvalidDataException("Saved Requirement Plan does not match the source reply.");
        Changed?.Invoke(this, new(ownerUserId, projectId));
        return saved;
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

    private async Task<AgentRequirementPlanFollowUp> GetFollowUpAsync(
        string ownerUserId,
        WindowsLocalRequirementSurvey survey,
        string runId,
        CancellationToken cancellationToken)
    {
        if (survey.Status == "open")
            return new(AgentRequirementPlanFollowUpState.AwaitingAnswers);
        var result = await host.SendAsync<GetLocalRunCommand, GetLocalRunResult>(
            new("get_run", ownerUserId, runId), cancellationToken).ConfigureAwait(false);
        if (result.Type != "run") throw new InvalidDataException("Invalid Run result.");
        var run = result.Run;
        if (run.OwnerUserId != ownerUserId || run.RunId != runId ||
            !RunMatchesSurvey(run, survey))
            throw new InvalidDataException("Requirement Plan source Run is invalid.");
        return MapFollowUp(run);
    }

    private async Task<AgentRequirementPlanFollowUp> GetPlanFollowUpAsync(
        string ownerUserId,
        AgentRequirementPlan plan,
        CancellationToken cancellationToken)
    {
        var result = await host.SendAsync<GetLocalRunCommand, GetLocalRunResult>(
            new("get_run", ownerUserId, plan.SourceRunId), cancellationToken)
            .ConfigureAwait(false);
        if (result.Type != "run") throw new InvalidDataException("Invalid Run result.");
        var run = result.Run;
        if (run.OwnerUserId != ownerUserId || run.RunId != plan.SourceRunId ||
            run.Input.ValueKind != JsonValueKind.Object)
            throw new InvalidDataException("Requirement Plan source Run is invalid.");
        var validMain = run.ProfileKey == "main_chat" &&
            run.OwnerEntityType == "conversation_turn" &&
            HasInputString(run.Input, "conversation_id", plan.SourceConversationId);
        var validTask = run.ProfileKey == "task_execution" &&
            run.OwnerEntityType == "task" &&
            HasInputString(run.Input, "source_conversation_id", plan.SourceConversationId);
        if (!validMain && !validTask)
            throw new InvalidDataException("Requirement Plan source Run is invalid.");
        return MapFollowUp(run);
    }

    private static bool HasInputString(JsonElement input, string key, string expected) =>
        input.TryGetProperty(key, out var value) && value.ValueKind == JsonValueKind.String &&
        value.GetString() == expected;

    private static bool RunMatchesSurvey(
        WindowsLocalAgentRun run,
        WindowsLocalRequirementSurvey survey)
    {
        if (run.Input.ValueKind != JsonValueKind.Object) return false;
        var key = survey.SourceTaskId is null ? "conversation_id" : "source_conversation_id";
        if (!run.Input.TryGetProperty(key, out var conversation) ||
            conversation.ValueKind != JsonValueKind.String ||
            conversation.GetString() != survey.SourceConversationId)
            return false;
        return survey.SourceTaskId is null
            ? run.ProfileKey == "main_chat" && run.OwnerEntityType == "conversation_turn"
            : run.ProfileKey == "task_execution" && run.OwnerEntityType == "task" &&
                run.OwnerEntityId == survey.SourceTaskId;
    }

    private static AgentRequirementPlanFollowUp MapFollowUp(WindowsLocalAgentRun run)
    {
        if (run.Status == "succeeded")
        {
            var markdown = TerminalReply(run.TerminalOutcome);
            return new(string.IsNullOrWhiteSpace(markdown)
                ? AgentRequirementPlanFollowUpState.SucceededWithoutReply
                : AgentRequirementPlanFollowUpState.Succeeded, markdown);
        }
        return new(run.Status switch
        {
            "failed" => AgentRequirementPlanFollowUpState.Failed,
            "cancelled" => AgentRequirementPlanFollowUpState.Cancelled,
            "waiting_user" => AgentRequirementPlanFollowUpState.WaitingForInput,
            "paused" or "needs_review" => AgentRequirementPlanFollowUpState.NeedsReview,
            _ => AgentRequirementPlanFollowUpState.Processing,
        });
    }

    private static string? TerminalReply(JsonElement? outcome)
    {
        if (outcome is not { ValueKind: JsonValueKind.Object } value) return null;
        foreach (var key in new[] { "content", "answer", "text", "output" })
            if (value.TryGetProperty(key, out var property) &&
                property.ValueKind == JsonValueKind.String &&
                !string.IsNullOrWhiteSpace(property.GetString()))
                return property.GetString();
        return null;
    }

    private static void ValidateScope(
        WindowsLocalRequirementSurvey survey,
        string ownerUserId,
        string projectId)
    {
        if (survey.OwnerUserId != ownerUserId || survey.ProjectResourceId != projectId)
            throw new InvalidDataException("Requirement Survey scope is invalid.");
    }

    private static AgentRequirementPlan MapPlan(
        WindowsLocalRequirementPlan plan,
        WindowsLocalRequirementSurvey survey,
        string ownerUserId,
        string projectId)
    {
        if (plan.OwnerUserId != ownerUserId || plan.ProjectResourceId != projectId ||
            plan.SourceSurveyId != survey.SurveyId ||
            plan.PlanId != $"requirement-plan:{survey.SurveyId}" || plan.Revision == 0 ||
            string.IsNullOrWhiteSpace(plan.Markdown))
            throw new InvalidDataException("Requirement Plan identity is invalid.");
        return new(plan.PlanId, plan.Revision, plan.OwnerUserId, plan.ProjectResourceId,
            plan.SourceConversationId, plan.SourceSurveyId, plan.SourceRunId,
            plan.Title, plan.Markdown, plan.CreatedAtUnixMs, plan.ConfirmedAtUnixMs);
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
