namespace ChatOS.Core.Domain;

public enum AgentRequirementSurveyStatus
{
    Pending,
    Submitted,
}

public enum AgentRequirementQuestionKind
{
    SingleChoice,
    MultipleChoice,
}

public sealed record AgentRequirementOption(string Id, string Label)
{
    public void Validate()
    {
        AgentTeamValidation.Identifier(Id, nameof(Id));
        AgentTeamValidation.Text(Label, nameof(Label), 500);
    }
}

public sealed record AgentRequirementQuestion(
    string Id,
    string Prompt,
    AgentRequirementQuestionKind Kind,
    IReadOnlyList<AgentRequirementOption> Options,
    bool IsRequired = true)
{
    public void Validate()
    {
        AgentTeamValidation.Identifier(Id, nameof(Id));
        AgentTeamValidation.Text(Prompt, nameof(Prompt), 1_000);
        if (Options.Count is < 2 or > 12 ||
            Options.Select(value => value.Id).Distinct(StringComparer.Ordinal).Count() != Options.Count)
            throw AgentTeamValidation.Invalid(nameof(Options));
        foreach (var option in Options) option.Validate();
    }
}

public sealed record AgentRequirementSurveyDraft(
    string Title,
    string Purpose,
    IReadOnlyList<AgentRequirementQuestion> Questions)
{
    public void Validate()
    {
        AgentTeamValidation.Text(Title, nameof(Title), 240);
        AgentTeamValidation.Text(Purpose, nameof(Purpose), 4_000);
        if (Questions.Count is < 1 or > 12 ||
            Questions.Select(value => value.Id).Distinct(StringComparer.Ordinal).Count() != Questions.Count)
            throw AgentTeamValidation.Invalid(nameof(Questions));
        foreach (var question in Questions) question.Validate();
    }
}

public sealed record AgentRequirementAnswer(
    string QuestionId,
    IReadOnlyList<string> SelectedOptionIds);

public sealed record AgentRequirementSubmission(
    IReadOnlyList<AgentRequirementAnswer> Answers,
    string Notes = "");

public sealed record AgentRequirementExecutionStep(
    string Id,
    string Title,
    string Detail,
    string Owner = "",
    string Deliverable = "",
    string AcceptanceCriteria = "")
{
    public void Validate()
    {
        AgentTeamValidation.Identifier(Id, nameof(Id));
        AgentTeamValidation.Text(Title, nameof(Title), 500);
        AgentTeamValidation.Text(Detail, nameof(Detail), 8_000);
        AgentTeamValidation.OptionalText(Owner, nameof(Owner), 500);
        AgentTeamValidation.OptionalText(Deliverable, nameof(Deliverable), 4_000);
        AgentTeamValidation.OptionalText(AcceptanceCriteria, nameof(AcceptanceCriteria), 4_000);
    }
}

public sealed record AgentRequirementResolution(
    string Summary,
    string SolutionMarkdown,
    IReadOnlyList<AgentRequirementExecutionStep> ExecutionSteps,
    string RisksAndOpenQuestions = "",
    string RelatedMaterials = "")
{
    public void Validate()
    {
        AgentTeamValidation.Text(Summary, nameof(Summary), 4_000);
        AgentTeamValidation.Text(SolutionMarkdown, nameof(SolutionMarkdown), 128_000);
        if (ExecutionSteps.Count is < 1 or > 50 ||
            ExecutionSteps.Select(value => value.Id).Distinct(StringComparer.Ordinal).Count() != ExecutionSteps.Count)
            throw AgentTeamValidation.Invalid(nameof(ExecutionSteps));
        foreach (var step in ExecutionSteps) step.Validate();
        AgentTeamValidation.OptionalText(RisksAndOpenQuestions, nameof(RisksAndOpenQuestions), 32_000);
        AgentTeamValidation.OptionalText(RelatedMaterials, nameof(RelatedMaterials), 32_000);
    }
}

public sealed record AgentRequirementSurvey(
    string Id,
    string OwnerUserId,
    string ProjectId,
    string CreatorAgentId,
    string SourceDeliveryId,
    string RequestKey,
    AgentRequirementSurveyDraft Draft,
    AgentRequirementSurveyStatus Status,
    AgentRequirementSubmission? Submission,
    AgentRequirementResolution? Resolution,
    long CreatedAtUnixMs,
    long? SubmittedAtUnixMs,
    long? ResolvedAtUnixMs)
{
    public void Validate()
    {
        AgentTeamValidation.Identifier(Id, nameof(Id));
        AgentTeamValidation.Identifier(OwnerUserId, nameof(OwnerUserId));
        AgentTeamValidation.Identifier(ProjectId, nameof(ProjectId));
        AgentTeamValidation.Identifier(CreatorAgentId, nameof(CreatorAgentId));
        AgentTeamValidation.Identifier(SourceDeliveryId, nameof(SourceDeliveryId));
        AgentTeamValidation.Identifier(RequestKey, nameof(RequestKey));
        Draft.Validate();
        if (CreatedAtUnixMs < 0) throw AgentTeamValidation.Invalid(nameof(CreatedAtUnixMs));
        if (Status == AgentRequirementSurveyStatus.Pending)
        {
            if (Submission is not null || SubmittedAtUnixMs is not null ||
                Resolution is not null || ResolvedAtUnixMs is not null)
                throw AgentTeamValidation.Invalid(nameof(Status));
            return;
        }

        if (Submission is null || SubmittedAtUnixMs < CreatedAtUnixMs)
            throw AgentTeamValidation.Invalid(nameof(Submission));
        ValidateSubmission(Submission, Draft.Questions);
        if (Resolution is not null)
        {
            if (ResolvedAtUnixMs < SubmittedAtUnixMs) throw AgentTeamValidation.Invalid(nameof(Resolution));
            Resolution.Validate();
        }
        else if (ResolvedAtUnixMs is not null)
        {
            throw AgentTeamValidation.Invalid(nameof(ResolvedAtUnixMs));
        }
    }

    public static void ValidateSubmission(
        AgentRequirementSubmission submission,
        IReadOnlyList<AgentRequirementQuestion> questions)
    {
        AgentTeamValidation.OptionalText(submission.Notes, nameof(submission.Notes), 16_000);
        if (submission.Answers.Select(value => value.QuestionId)
            .Distinct(StringComparer.Ordinal).Count() != submission.Answers.Count)
            throw AgentTeamValidation.Invalid(nameof(submission.Answers));
        var answers = submission.Answers.ToDictionary(value => value.QuestionId, StringComparer.Ordinal);
        if (answers.Keys.Except(questions.Select(value => value.Id), StringComparer.Ordinal).Any())
            throw AgentTeamValidation.Invalid(nameof(submission.Answers));
        foreach (var question in questions)
        {
            var selected = answers.GetValueOrDefault(question.Id)?.SelectedOptionIds ?? [];
            if (selected.Distinct(StringComparer.Ordinal).Count() != selected.Count ||
                selected.Except(question.Options.Select(value => value.Id), StringComparer.Ordinal).Any() ||
                question.IsRequired && selected.Count == 0 ||
                question.Kind == AgentRequirementQuestionKind.SingleChoice && selected.Count > 1)
                throw AgentTeamValidation.Invalid(nameof(submission.Answers));
        }
    }
}
