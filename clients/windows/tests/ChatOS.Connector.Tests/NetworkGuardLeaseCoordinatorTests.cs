using ChatOS.Connector.NetworkGuard;
using ChatOS.NetworkGuard.Contracts;

namespace ChatOS.Connector.Tests;

public sealed class NetworkGuardLeaseCoordinatorTests
{
    [Fact]
    public async Task AcquireRefusesToCreateLeaseWhenGuardIsNotReady()
    {
        var client = new StubGuardClient
        {
            Readiness = new NetworkGuardReadiness(NetworkGuardReadinessState.DriverUnavailable),
        };
        var coordinator = new NetworkGuardLeaseCoordinator(client);

        await Assert.ThrowsAsync<InvalidOperationException>(() => coordinator.AcquireAsync(
            Policy(),
            "S-1-15-2-1234",
            42,
            _ => Task.CompletedTask));
        Assert.Equal(0, client.AcquireCount);
    }

    [Fact]
    public async Task DisposeKillsProcessBeforeReleasingLease()
    {
        var order = new List<string>();
        var client = new StubGuardClient
        {
            OnRelease = () => order.Add("release"),
        };
        var coordinator = new NetworkGuardLeaseCoordinator(
            client,
            maximumRenewInterval: TimeSpan.FromMinutes(1));
        var lifetime = await coordinator.AcquireAsync(
            Policy(),
            "S-1-15-2-1234",
            42,
            _ =>
            {
                order.Add("kill");
                return Task.CompletedTask;
            });

        await lifetime.DisposeAsync();

        Assert.Equal(["kill", "release"], order);
        Assert.False(lifetime.IsLost);
    }

    [Fact]
    public async Task RenewalFailureFailsClosedAndReleasesLeaseOnce()
    {
        var failedClosed = new TaskCompletionSource(
            TaskCreationOptions.RunContinuationsAsynchronously);
        var client = new StubGuardClient
        {
            ThrowOnRenew = true,
        };
        var coordinator = new NetworkGuardLeaseCoordinator(
            client,
            maximumRenewInterval: TimeSpan.FromMilliseconds(20));
        await using var lifetime = await coordinator.AcquireAsync(
            Policy(),
            "S-1-15-2-1234",
            42,
            _ =>
            {
                failedClosed.TrySetResult();
                return Task.CompletedTask;
            });

        await failedClosed.Task.WaitAsync(TimeSpan.FromSeconds(2));

        Assert.True(lifetime.IsLost);
        Assert.Equal(1, client.RenewCount);
        Assert.Equal(1, client.ReleaseCount);
    }

    private static ControlledNetworkPolicyEnvelope Policy() => new(
        "policy-1",
        "owner-1",
        "device-1",
        "workspace-1",
        "S-1-5-21-100-200-300-400",
        ["example.com"],
        [443],
        DateTimeOffset.UtcNow.AddHours(1),
        "key-1",
        "ed25519",
        "signature");

    private sealed class StubGuardClient : IControlledNetworkGuardClient
    {
        public NetworkGuardReadiness Readiness { get; init; } =
            new(NetworkGuardReadinessState.Ready, "1.0.0", "1.0.0");
        public bool ThrowOnRenew { get; init; }
        public Action? OnRelease { get; init; }
        public int AcquireCount { get; private set; }
        public int RenewCount { get; private set; }
        public int ReleaseCount { get; private set; }

        public Task<NetworkGuardReadiness> CheckReadinessAsync(
            CancellationToken cancellationToken = default) => Task.FromResult(Readiness);

        public Task<NetworkGuardLease> AcquireLeaseAsync(
            ControlledNetworkPolicyEnvelope policy,
            string appContainerSid,
            int processId,
            CancellationToken cancellationToken = default)
        {
            AcquireCount++;
            return Task.FromResult(new NetworkGuardLease(
                "lease-1",
                DateTimeOffset.UtcNow.AddHours(1),
                policy.PolicyRevision,
                appContainerSid,
                processId));
        }

        public Task<NetworkGuardLease> RenewLeaseAsync(
            NetworkGuardLease lease,
            CancellationToken cancellationToken = default)
        {
            RenewCount++;
            return ThrowOnRenew
                ? Task.FromException<NetworkGuardLease>(new IOException("service unavailable"))
                : Task.FromResult(lease with { ExpiresAt = DateTimeOffset.UtcNow.AddHours(1) });
        }

        public Task ReleaseLeaseAsync(
            NetworkGuardLease lease,
            CancellationToken cancellationToken = default)
        {
            ReleaseCount++;
            OnRelease?.Invoke();
            return Task.CompletedTask;
        }
    }
}
