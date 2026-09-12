namespace ChatOS.Connector.LocalAgent;

public interface IWindowsLocalAgentClientRuntime
{
    Task ActivateAsync(string accountId, CancellationToken cancellationToken = default);
    Task UpdateAccessTokenAsync(string accountId, CancellationToken cancellationToken = default);
    Task LogoutAsync();
}

/// The production transaction boundary for one Windows account: Host startup,
/// authoritative recovery, then background event consumption.
public sealed class WindowsLocalAgentClientRuntime : IWindowsLocalAgentClientRuntime
{
    private readonly IWindowsLocalAgentAccountSession _accountSession;
    private readonly IWindowsLocalAgentStartupRecovery _recovery;
    private readonly IWindowsLocalAgentEventHub _eventHub;
    private readonly IWindowsLocalAgentProjectionStore _store;
    private readonly TimeSpan _recoveryRetryDelay;
    private readonly int _maximumSameEndpointFailures;
    private readonly SemaphoreSlim _gate = new(1, 1);

    public WindowsLocalAgentClientRuntime(
        IWindowsLocalAgentAccountSession accountSession,
        IWindowsLocalAgentStartupRecovery recovery,
        IWindowsLocalAgentEventHub eventHub,
        IWindowsLocalAgentProjectionStore store)
        : this(accountSession, recovery, eventHub, store, TimeSpan.FromMilliseconds(250), 20)
    {
    }

    internal WindowsLocalAgentClientRuntime(
        IWindowsLocalAgentAccountSession accountSession,
        IWindowsLocalAgentStartupRecovery recovery,
        IWindowsLocalAgentEventHub eventHub,
        IWindowsLocalAgentProjectionStore store,
        TimeSpan recoveryRetryDelay,
        int maximumSameEndpointFailures)
    {
        if (recoveryRetryDelay < TimeSpan.Zero || maximumSameEndpointFailures <= 0)
        {
            throw new ArgumentOutOfRangeException(nameof(recoveryRetryDelay));
        }
        _accountSession = accountSession;
        _recovery = recovery;
        _eventHub = eventHub;
        _store = store;
        _recoveryRetryDelay = recoveryRetryDelay;
        _maximumSameEndpointFailures = maximumSameEndpointFailures;
    }

    public async Task ActivateAsync(string accountId, CancellationToken cancellationToken = default)
    {
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await ResetBeforeTransitionAsync().ConfigureAwait(false);
            try
            {
                await _accountSession.ActivateAsync(accountId, cancellationToken)
                    .ConfigureAwait(false);
                await RestoreAndStartAsync(accountId, cancellationToken).ConfigureAwait(false);
            }
            catch
            {
                await ResetBeforeTransitionAsync().ConfigureAwait(false);
                await _accountSession.LogoutAsync().ConfigureAwait(false);
                throw;
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task UpdateAccessTokenAsync(
        string accountId,
        CancellationToken cancellationToken = default)
    {
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await _eventHub.StopAsync().ConfigureAwait(false);
            try
            {
                await _accountSession.UpdateAccessTokenAsync(accountId, cancellationToken)
                    .ConfigureAwait(false);
                await RestoreAndStartAsync(accountId, cancellationToken).ConfigureAwait(false);
            }
            catch
            {
                await _store.ResetAsync().ConfigureAwait(false);
                await _accountSession.LogoutAsync().ConfigureAwait(false);
                throw;
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task LogoutAsync()
    {
        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            await ResetBeforeTransitionAsync().ConfigureAwait(false);
            await _accountSession.LogoutAsync().ConfigureAwait(false);
        }
        finally
        {
            _gate.Release();
        }
    }

    private async Task RestoreAndStartAsync(string accountId, CancellationToken cancellationToken)
    {
        await RestoreFromOneHostLifetimeAsync(accountId, cancellationToken).ConfigureAwait(false);
        await _eventHub.StartAsync(accountId, cancellationToken).ConfigureAwait(false);
    }

    private async Task RestoreFromOneHostLifetimeAsync(
        string accountId,
        CancellationToken cancellationToken)
    {
        string? failingEndpoint = null;
        var failuresAtEndpoint = 0;
        while (true)
        {
            cancellationToken.ThrowIfCancellationRequested();
            var state = await _accountSession.GetStateAsync().ConfigureAwait(false);
            if (state.Status is WindowsLocalAgentHostStatus.Starting
                or WindowsLocalAgentHostStatus.Restarting)
            {
                failingEndpoint = null;
                failuresAtEndpoint = 0;
                await Task.Delay(_recoveryRetryDelay, cancellationToken).ConfigureAwait(false);
                continue;
            }
            if (state.Status == WindowsLocalAgentHostStatus.Stopped)
            {
                throw new InvalidOperationException(
                    "Local Agent Host stopped before startup recovery completed.");
            }
            if (state.Status == WindowsLocalAgentHostStatus.Failed)
            {
                throw new InvalidOperationException(
                    state.FailureReason ?? "Local Agent Host failed during startup recovery.");
            }
            if (state.Status != WindowsLocalAgentHostStatus.Running
                || string.IsNullOrWhiteSpace(state.ClientEndpoint))
            {
                throw new InvalidDataException("Local Agent Host state is invalid.");
            }

            try
            {
                var client = await _accountSession.GetClientAsync(accountId, cancellationToken)
                    .ConfigureAwait(false);
                await _recovery.RestoreAsync(accountId, client, cancellationToken)
                    .ConfigureAwait(false);
                return;
            }
            catch (Exception error) when (
                WindowsLocalAgentHostFailurePolicy.IsTransientEndpointLoss(error))
            {
                if (failingEndpoint == state.ClientEndpoint)
                {
                    failuresAtEndpoint++;
                }
                else
                {
                    failingEndpoint = state.ClientEndpoint;
                    failuresAtEndpoint = 1;
                }
                if (failuresAtEndpoint >= _maximumSameEndpointFailures)
                {
                    throw new IOException(
                        $"Local Agent endpoint '{state.ClientEndpoint}' remained unavailable.",
                        error);
                }
                await Task.Delay(_recoveryRetryDelay, cancellationToken).ConfigureAwait(false);
            }
        }
    }

    private async Task ResetBeforeTransitionAsync()
    {
        await _eventHub.StopAsync().ConfigureAwait(false);
        await _store.ResetAsync().ConfigureAwait(false);
    }
}
