namespace ChatOS.Connector.Approval;

public enum CommandApprovalAiDecisionKind
{
    Approve,
    Deny,
    AskUser,
}

public sealed record CommandApprovalAiReview(
    CommandApprovalAiDecisionKind Decision,
    string Reason,
    bool RememberForSession = false);

public interface ICommandApprovalAiReviewer
{
    Task<CommandApprovalAiReview> ReviewAsync(
        CommandApprovalRequest request,
        ConnectorApprovalRisk risk,
        CancellationToken cancellationToken = default);
}
