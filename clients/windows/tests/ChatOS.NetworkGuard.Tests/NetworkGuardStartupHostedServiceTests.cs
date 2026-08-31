using ChatOS.NetworkGuard.Service;

namespace ChatOS.NetworkGuard.Tests;

public sealed class NetworkGuardStartupHostedServiceTests
{
    [Fact]
    public async Task StartupResetsStaleDriverLeasesBeforeBrokerCanBecomeReady()
    {
        var native = new StubNativeController();
        var broker = new NetworkGuardBrokerState();
        broker.SetReady(true);
        var service = new NetworkGuardStartupHostedService(native, broker);

        await service.StartAsync(CancellationToken.None);

        Assert.Equal(1, native.ResetCount);
        Assert.False(broker.IsReady);
    }

    [Fact]
    public async Task StartupFailsClosedWhenDriverStillReportsLeaseResidue()
    {
        var native = new StubNativeController
        {
            Health = new NetworkGuardDriverHealth(true, true, "test", ActiveLeaseCount: 1),
        };
        var broker = new NetworkGuardBrokerState();
        var service = new NetworkGuardStartupHostedService(native, broker);

        await Assert.ThrowsAsync<InvalidOperationException>(() =>
            service.StartAsync(CancellationToken.None));

        Assert.False(broker.IsReady);
    }

    private sealed class StubNativeController : INetworkGuardNativeController
    {
        public NetworkGuardDriverHealth Health { get; init; } = new(true, true, "test");

        public int ResetCount { get; private set; }

        public Task<NetworkGuardDriverHealth> CheckHealthAsync(
            CancellationToken cancellationToken = default) => Task.FromResult(Health);

        public Task ResetAsync(CancellationToken cancellationToken = default)
        {
            ResetCount++;
            return Task.CompletedTask;
        }

        public Task ApplyLeaseAsync(
            ActiveNetworkGuardLease lease,
            int httpBrokerPort,
            int httpsBrokerPort,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();

        public Task RemoveLeaseAsync(
            Guid leaseId,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }
}
