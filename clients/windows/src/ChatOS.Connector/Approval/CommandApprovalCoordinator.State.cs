using System.Collections.Concurrent;

namespace ChatOS.Connector.Approval;

public sealed partial class CommandApprovalCoordinator
{
    private sealed class PendingState(
        CommandApprovalRequest request,
        ConnectorPendingApproval pending,
        long sessionGeneration,
        TaskCompletionSource<ConnectorApprovalOutcome>? completion = null)
    {
        public CommandApprovalRequest Request { get; } = request;

        public ConnectorPendingApproval Pending { get; } = pending;

        public long SessionGeneration { get; } = sessionGeneration;

        public TaskCompletionSource<ConnectorApprovalOutcome> Completion { get; } = completion ??
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        private int _resolving;

        public bool TryBeginResolution() =>
            Interlocked.CompareExchange(ref _resolving, 1, 0) == 0;

        public void CancelResolution() => Volatile.Write(ref _resolving, 0);
    }

    private sealed class AiReviewState(
        string approvalId,
        CommandApprovalRequest request,
        ConnectorApprovalRisk risk,
        ConnectorApprovalMode mode,
        long sessionGeneration)
    {
        public string ApprovalId { get; } = approvalId;
        public CommandApprovalRequest Request { get; } = request;
        public ConnectorApprovalRisk Risk { get; } = risk;
        public ConnectorApprovalMode Mode { get; } = mode;
        public long SessionGeneration { get; } = sessionGeneration;
        public CancellationTokenSource Lifetime { get; } = new();
        public TaskCompletionSource<ConnectorApprovalOutcome> Completion { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
    }
}
