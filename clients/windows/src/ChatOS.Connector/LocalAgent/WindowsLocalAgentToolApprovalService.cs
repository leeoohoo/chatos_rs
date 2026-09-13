using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

/// <summary>
/// Tool approvals are read from complete Local Agent Run Details and decided
/// through the exact Run/invocation tuple. This is separate from Connector
/// command approval, which protects manually executed local shell operations.
/// </summary>
public sealed class WindowsLocalAgentToolApprovalService(
    IWindowsLocalAgentProjectionStore store,
    IWindowsLocalAgentAccountSession accountSession,
    WindowsLocalAgentRunProjectionRefresher refresher) : ILocalAgentToolApprovalService
{
    public async Task<IReadOnlyList<LocalAgentToolApprovalRequest>> FetchPendingAsync(
        string conversationId,
        CancellationToken cancellationToken = default)
    {
        RequireIdentity(conversationId, nameof(conversationId));
        var projection = await RequireProjectionAsync(cancellationToken).ConfigureAwait(false);
        var routes = PendingRoutes(projection, conversationId).ToArray();
        if (routes.Select(route => route.Request.InvocationId)
                .Distinct(StringComparer.Ordinal).Count() != routes.Length)
        {
            throw new InvalidDataException(
                "The Local Agent projection contains duplicate tool invocation identities.");
        }
        return routes
            .OrderBy(route => route.Run.UpdatedAt)
            .ThenBy(route => route.Request.InvocationId, StringComparer.Ordinal)
            .Select(route => route.Request)
            .ToArray();
    }

    public async Task DecideAsync(
        string invocationId,
        string conversationId,
        LocalAgentToolApprovalDecision decision,
        string? reason,
        CancellationToken cancellationToken = default)
    {
        RequireIdentity(invocationId, nameof(invocationId));
        RequireIdentity(conversationId, nameof(conversationId));
        if (decision is not (LocalAgentToolApprovalDecision.Approve
            or LocalAgentToolApprovalDecision.Reject))
        {
            throw new ArgumentOutOfRangeException(nameof(decision));
        }
        var projection = await RequireProjectionAsync(cancellationToken).ConfigureAwait(false);
        var matches = PendingRoutes(projection, conversationId)
            .Where(route => string.Equals(
                route.Request.InvocationId, invocationId, StringComparison.Ordinal))
            .ToArray();
        if (matches.Length != 1)
            throw new InvalidOperationException(
                "The Local Agent tool invocation is no longer awaiting approval.");
        var normalizedReason = string.IsNullOrWhiteSpace(reason) ? null : reason.Trim();
        if (normalizedReason is not null && normalizedReason.Any(char.IsControl))
            throw new ArgumentException("The Local Agent approval reason is invalid.", nameof(reason));
        var route = matches[0];
        var client = await accountSession.GetClientAsync(projection.AccountId, cancellationToken)
            .ConfigureAwait(false);
        _ = await client.AcceptAsync(
            LocalAgentCommand.DecideToolApproval(
                route.Run.RunId, route.Request.InvocationId, decision, normalizedReason),
            cancellationToken).ConfigureAwait(false);
        _ = await refresher.RefreshAsync(
            projection.AccountId, route.Run.RunId, cancellationToken).ConfigureAwait(false);
    }

    private async Task<WindowsLocalAgentProjectionSnapshot> RequireProjectionAsync(
        CancellationToken cancellationToken) =>
        await store.GetAsync(cancellationToken).ConfigureAwait(false)
        ?? throw new InvalidOperationException("The Local Agent account projection is not available.");

    private static IEnumerable<ApprovalRoute> PendingRoutes(
        WindowsLocalAgentProjectionSnapshot projection,
        string conversationId)
    {
        foreach (var route in WindowsLocalAgentInteractionProjection.Routes(projection))
        {
            if (!string.Equals(route.Source.ThreadId, conversationId, StringComparison.Ordinal))
                continue;
            foreach (var request in WindowsLocalAgentInteractionProjection.ToolApprovals(route))
            {
                yield return new ApprovalRoute(route.Run, request);
            }
        }
    }

    private static void RequireIdentity(string value, string name)
    {
        if (string.IsNullOrWhiteSpace(value) || value != value.Trim() || value.Any(char.IsControl))
            throw new ArgumentException("The Local Agent approval identity is invalid.", name);
    }

    private sealed record ApprovalRoute(
        LocalAgentRunSnapshot Run,
        LocalAgentToolApprovalRequest Request);
}
