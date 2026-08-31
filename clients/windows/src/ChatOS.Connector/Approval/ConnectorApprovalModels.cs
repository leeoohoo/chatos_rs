namespace ChatOS.Connector.Approval;

public enum ConnectorApprovalMode
{
    RequestApproval,
    AutoApproval,
    FullControl,
}

public enum ConnectorApprovalAction
{
    Accept,
    AcceptForSession,
    Decline,
}

public enum ConnectorApprovalRiskLevel
{
    Low,
    Medium,
    High,
}

public enum ConnectorApprovalReviewer
{
    User,
    Ai,
    Policy,
    Session,
    System,
}

public sealed record ConnectorApprovalRisk(
    ConnectorApprovalRiskLevel Level,
    string? Reason = null);

public sealed record CommandApprovalRequest(
    string RequestId,
    string OwnerUserId,
    string DeviceId,
    string WorkspaceId,
    string Command,
    IReadOnlyList<string> Arguments,
    string WorkingDirectory,
    string Source,
    string ScopeKey)
{
    public string StableIdentity => string.Join('\0',
        OwnerUserId,
        DeviceId,
        WorkspaceId,
        RequestId);

    public string DisplayCommand => CommandDisplay.Format(Command, Arguments);
}

public sealed record ConnectorPendingApproval(
    string Id,
    string StableIdentity,
    string RequestId,
    string WorkspaceId,
    string Command,
    string WorkingDirectory,
    string Source,
    ConnectorApprovalRisk Risk,
    string? Reason,
    ConnectorApprovalMode Mode,
    DateTimeOffset CreatedAt,
    IReadOnlyList<ConnectorApprovalAction> AvailableActions);

public sealed record ConnectorApprovalOutcome(
    bool Approved,
    ConnectorApprovalMode Mode,
    ConnectorApprovalReviewer Reviewer,
    string Reason,
    bool RememberedForSession = false);

public sealed class ConnectorApprovalDecisionEventArgs(
    string approvalId,
    CommandApprovalRequest request,
    ConnectorApprovalRisk risk,
    ConnectorApprovalOutcome outcome,
    DateTimeOffset occurredAt) : EventArgs
{
    public string ApprovalId { get; } = approvalId;
    public CommandApprovalRequest Request { get; } = request;
    public ConnectorApprovalRisk Risk { get; } = risk;
    public ConnectorApprovalOutcome Outcome { get; } = outcome;
    public DateTimeOffset OccurredAt { get; } = occurredAt;
}

public sealed record ConnectorApprovalHistoryEntry(
    string Id,
    string ApprovalId,
    string RequestId,
    string WorkspaceId,
    string Command,
    string WorkingDirectory,
    string Source,
    ConnectorApprovalMode Mode,
    bool Approved,
    ConnectorApprovalReviewer Reviewer,
    ConnectorApprovalRiskLevel Risk,
    string? RiskReason,
    string Reason,
    DateTimeOffset CreatedAt);

internal static class CommandDisplay
{
    public static string Format(string command, IReadOnlyList<string> arguments) =>
        string.Join(' ', new[] { command }.Concat(arguments).Select(QuoteIfNeeded));

    private static string QuoteIfNeeded(string value) =>
        value.Any(char.IsWhiteSpace) || value.Contains('"')
            ? $"\"{value.Replace("\"", "\\\"")}\""
            : value;
}
