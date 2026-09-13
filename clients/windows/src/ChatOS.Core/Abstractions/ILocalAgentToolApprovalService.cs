using ChatOS.Core.Domain;

namespace ChatOS.Core.Abstractions;

/// <summary>
/// Resolves Local Agent tool approvals from the account-scoped Host projection.
/// Decisions apply to one exact invocation and are never reusable grants.
/// </summary>
public interface ILocalAgentToolApprovalService
{
    Task<IReadOnlyList<LocalAgentToolApprovalRequest>> FetchPendingAsync(
        string conversationId,
        CancellationToken cancellationToken = default);

    Task DecideAsync(
        string invocationId,
        string conversationId,
        LocalAgentToolApprovalDecision decision,
        string? reason,
        CancellationToken cancellationToken = default);
}
