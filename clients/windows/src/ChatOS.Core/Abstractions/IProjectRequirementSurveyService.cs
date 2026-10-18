using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

public sealed class ProjectRequirementSurveyChangedEventArgs(
    string ownerUserId,
    string projectId) : EventArgs
{
    public string OwnerUserId { get; } = ownerUserId;
    public string ProjectId { get; } = projectId;
}

public interface IProjectRequirementSurveyService
{
    event EventHandler<ProjectRequirementSurveyChangedEventArgs>? Changed;

    Task<IReadOnlyList<AgentRequirementSurvey>> ListAsync(
        string ownerUserId,
        string projectId,
        CancellationToken cancellationToken = default);

    Task<AgentRequirementSurvey> SubmitAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        AgentRequirementSubmission submission,
        CancellationToken cancellationToken = default);

    Task<AgentRequirementSurveyDetails> GetDetailsAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        CancellationToken cancellationToken = default);

    Task<AgentRequirementPlan> ConfirmPlanAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        ulong expectedRevision,
        CancellationToken cancellationToken = default);

    Task<AgentRequirementPlan> SaveSourceReplyAsPlanAsync(
        string ownerUserId,
        string projectId,
        string surveyId,
        string markdown,
        CancellationToken cancellationToken = default);
}
