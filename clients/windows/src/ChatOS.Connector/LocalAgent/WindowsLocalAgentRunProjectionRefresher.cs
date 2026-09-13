using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

/// Refreshes one changed Run from the current Named Pipe endpoint and commits
/// its complete authoritative detail atomically. Ask User, approvals and run
/// controls share this boundary instead of implementing their own projections.
public sealed class WindowsLocalAgentRunProjectionRefresher(
    IWindowsLocalAgentProjectionStore store,
    IWindowsLocalAgentAccountSession accountSession)
{
    public async Task<WindowsLocalAgentRecoveredRun> RefreshAsync(
        string accountId,
        string runId,
        CancellationToken cancellationToken = default)
    {
        var projection = await store.GetAsync(cancellationToken).ConfigureAwait(false)
            ?? throw new InvalidOperationException("The Local Agent account projection is not available.");
        if (!string.Equals(projection.AccountId, accountId, StringComparison.Ordinal)
            || !projection.Runs.TryGetValue(runId, out var previous))
            throw new InvalidOperationException("The Local Agent run is not owned by the active account.");

        var client = await accountSession.GetClientAsync(accountId, cancellationToken)
            .ConfigureAwait(false);
        var detail = await WindowsLocalAgentStartupRecovery.CompleteDetailAsync(
            client, runId, cancellationToken).ConfigureAwait(false);
        WindowsLocalAgentStartupRecovery.ValidateRunDetail(previous.Run, detail.Run);
        LocalAgentMainChatRunBinding? binding = null;
        if (detail.Run.ProfileKey == "main_chat")
        {
            binding = await client.GetMainChatRunBindingAsync(runId, cancellationToken)
                .ConfigureAwait(false);
            WindowsLocalAgentStartupRecovery.ValidateBinding(detail.Run, binding);
        }
        var recovered = new WindowsLocalAgentRecoveredRun(
            detail.Run, detail, binding, detail.SnapshotEventSequence);
        await store.ReplaceAuthoritativeRunAsync(accountId, recovered, cancellationToken)
            .ConfigureAwait(false);
        return recovered;
    }
}
