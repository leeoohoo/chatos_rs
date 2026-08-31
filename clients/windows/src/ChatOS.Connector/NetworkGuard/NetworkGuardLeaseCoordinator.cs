using ChatOS.NetworkGuard.Contracts;

namespace ChatOS.Connector.NetworkGuard;

public sealed class NetworkGuardLeaseCoordinator(
    IControlledNetworkGuardClient client,
    TimeProvider? timeProvider = null,
    TimeSpan? maximumRenewInterval = null)
{
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;
    private readonly TimeSpan _maximumRenewInterval = maximumRenewInterval ?? TimeSpan.FromSeconds(30);

    public async Task<NetworkGuardLeaseLifetime> AcquireAsync(
        ControlledNetworkPolicyEnvelope policy,
        string appContainerSid,
        int processId,
        Func<CancellationToken, Task> failClosed,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(policy);
        ArgumentNullException.ThrowIfNull(failClosed);
        var readiness = await client.CheckReadinessAsync(cancellationToken).ConfigureAwait(false);
        if (!readiness.IsReady)
        {
            throw new InvalidOperationException(
                $"NetworkGuard is not ready ({readiness.State}); controlled networking remains disabled.");
        }

        var lease = await client.AcquireLeaseAsync(
            policy,
            appContainerSid,
            processId,
            cancellationToken).ConfigureAwait(false);
        return new NetworkGuardLeaseLifetime(
            client,
            lease,
            failClosed,
            _timeProvider,
            _maximumRenewInterval);
    }
}

public sealed class NetworkGuardLeaseLifetime : IAsyncDisposable
{
    private readonly IControlledNetworkGuardClient _client;
    private readonly Func<CancellationToken, Task> _failClosed;
    private readonly TimeProvider _timeProvider;
    private readonly TimeSpan _maximumRenewInterval;
    private readonly CancellationTokenSource _lifetime = new();
    private readonly Task _renewalTask;
    private NetworkGuardLease _lease;
    private int _failedClosed;
    private int _releaseAttempted;
    private int _disposed;
    private int _lost;

    internal NetworkGuardLeaseLifetime(
        IControlledNetworkGuardClient client,
        NetworkGuardLease lease,
        Func<CancellationToken, Task> failClosed,
        TimeProvider timeProvider,
        TimeSpan maximumRenewInterval)
    {
        if (maximumRenewInterval <= TimeSpan.Zero)
        {
            throw new ArgumentOutOfRangeException(nameof(maximumRenewInterval));
        }
        _client = client;
        _lease = lease;
        _failClosed = failClosed;
        _timeProvider = timeProvider;
        _maximumRenewInterval = maximumRenewInterval;
        _renewalTask = RenewUntilStoppedAsync();
    }

    public NetworkGuardLease CurrentLease => Volatile.Read(ref _lease);

    public bool IsLost => Volatile.Read(ref _lost) != 0;

    public async ValueTask DisposeAsync()
    {
        if (Interlocked.Exchange(ref _disposed, 1) != 0) return;
        _lifetime.Cancel();
        try
        {
            await _renewalTask.ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (_lifetime.IsCancellationRequested)
        {
        }

        await FailClosedSafelyAsync().ConfigureAwait(false);
        try
        {
            await ReleaseOnceAsync().ConfigureAwait(false);
        }
        catch
        {
            Interlocked.Exchange(ref _lost, 1);
        }
        finally
        {
            _lifetime.Dispose();
        }
    }

    private async Task RenewUntilStoppedAsync()
    {
        try
        {
            while (!_lifetime.IsCancellationRequested)
            {
                var lease = CurrentLease;
                var remaining = lease.ExpiresAt - _timeProvider.GetUtcNow();
                if (remaining <= TimeSpan.Zero)
                {
                    throw new TimeoutException("NetworkGuard lease expired before renewal.");
                }
                var delay = TimeSpan.FromTicks(Math.Max(
                    TimeSpan.FromMilliseconds(100).Ticks,
                    Math.Min(_maximumRenewInterval.Ticks, remaining.Ticks / 2)));
                await Task.Delay(delay, _timeProvider, _lifetime.Token).ConfigureAwait(false);
                var renewed = await _client.RenewLeaseAsync(lease, _lifetime.Token)
                    .ConfigureAwait(false);
                Volatile.Write(ref _lease, renewed);
            }
        }
        catch (OperationCanceledException) when (_lifetime.IsCancellationRequested)
        {
        }
        catch
        {
            Interlocked.Exchange(ref _lost, 1);
            await FailClosedSafelyAsync().ConfigureAwait(false);
            try
            {
                await ReleaseOnceAsync().ConfigureAwait(false);
            }
            catch
            {
            }
        }
    }

    private async Task FailClosedOnceAsync()
    {
        if (Interlocked.Exchange(ref _failedClosed, 1) == 0)
        {
            await _failClosed(CancellationToken.None).ConfigureAwait(false);
        }
    }

    private async Task FailClosedSafelyAsync()
    {
        try
        {
            await FailClosedOnceAsync().ConfigureAwait(false);
        }
        catch
        {
            Interlocked.Exchange(ref _lost, 1);
        }
    }

    private async Task ReleaseOnceAsync()
    {
        if (Interlocked.Exchange(ref _releaseAttempted, 1) == 0)
        {
            await _client.ReleaseLeaseAsync(CurrentLease, CancellationToken.None)
                .ConfigureAwait(false);
        }
    }
}
