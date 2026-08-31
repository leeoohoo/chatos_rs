using ChatOS.NetworkGuard.Contracts;
using ChatOS.NetworkGuard.Service;
using Microsoft.Extensions.Options;

namespace ChatOS.NetworkGuard.Tests;

public sealed class NetworkGuardDriverBackendTests
{
    [Fact]
    public async Task LeaseIsBoundToSidAndProcessAndRemovedExactlyOnce()
    {
        var context = new BackendContext();
        var lease = await context.Backend.AcquireAsync(
            context.Policy(),
            BackendContext.SidA,
            41,
            BackendContext.WindowsSid);

        await Assert.ThrowsAsync<InvalidOperationException>(() => context.Backend.RenewAsync(
            lease.LeaseId,
            BackendContext.SidB,
            41,
            BackendContext.WindowsSid));
        await Assert.ThrowsAsync<InvalidOperationException>(() => context.Backend.ReleaseAsync(
            lease.LeaseId,
            BackendContext.SidA,
            42,
            BackendContext.WindowsSid));

        var renewed = await context.Backend.RenewAsync(
            lease.LeaseId,
            BackendContext.SidA,
            41,
            BackendContext.WindowsSid);
        await context.Backend.ReleaseAsync(
            renewed.LeaseId,
            BackendContext.SidA,
            41,
            BackendContext.WindowsSid);

        Assert.Equal(2, context.Native.Applied.Count);
        Assert.Single(context.Native.Removed);
        Assert.Equal(Guid.ParseExact(lease.LeaseId, "N"), context.Native.Removed[0]);
    }

    [Fact]
    public async Task ExpiredLeaseFailsClosedAndRemovesNativeFilter()
    {
        var context = new BackendContext();
        var lease = await context.Backend.AcquireAsync(
            context.Policy(),
            BackendContext.SidA,
            41,
            BackendContext.WindowsSid);
        context.Time.Advance(TimeSpan.FromMinutes(3));

        Assert.False(context.Backend.TryGetActive(Guid.ParseExact(lease.LeaseId, "N"), out _));
        await Assert.ThrowsAsync<InvalidOperationException>(() => context.Backend.RenewAsync(
            lease.LeaseId,
            BackendContext.SidA,
            41,
            BackendContext.WindowsSid));

        await WaitUntilAsync(() => context.Native.Removed.Count == 1);
    }

    [Fact]
    public async Task NativeApplyFailureRollsBackLeaseAndFilter()
    {
        var context = new BackendContext { Native = { ThrowOnApply = true } };

        await Assert.ThrowsAsync<IOException>(() => context.Backend.AcquireAsync(
            context.Policy(),
            BackendContext.SidA,
            41,
            BackendContext.WindowsSid));

        Assert.Single(context.Native.Removed);
    }

    [Fact]
    public async Task HealthRequiresBothDriverAndBrokerSelfTest()
    {
        var context = new BackendContext(brokerReady: false);

        var unavailable = await context.Backend.CheckHealthAsync();
        context.Broker.SetReady(true);
        var ready = await context.Backend.CheckHealthAsync();

        Assert.True(unavailable.DriverReady);
        Assert.False(unavailable.SelfTestPassed);
        Assert.True(ready.DriverReady);
        Assert.True(ready.SelfTestPassed);
    }

    private static async Task WaitUntilAsync(Func<bool> predicate)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(2));
        while (!predicate()) await Task.Delay(10, timeout.Token);
    }

    private sealed class BackendContext
    {
        public const string SidA = "S-1-15-2-111111111-222222222";
        public const string SidB = "S-1-15-2-333333333-444444444";
        public const string WindowsSid = "S-1-5-21-100-200-300-400";

        public BackendContext(bool brokerReady = true)
        {
            Broker.SetReady(brokerReady);
            Backend = new NetworkGuardDriverBackend(
                Native,
                Broker,
                Options.Create(new NetworkGuardServiceOptions
                {
                    LeaseDuration = TimeSpan.FromMinutes(2),
                }),
                Time);
        }

        public MutableTimeProvider Time { get; } = new(DateTimeOffset.Parse("2026-08-30T12:00:00Z"));
        public FakeNativeController Native { get; } = new();
        public NetworkGuardBrokerState Broker { get; } = new();
        public NetworkGuardDriverBackend Backend { get; }

        public ControlledNetworkPolicy Policy() => new(
            "policy-1",
            "owner-1",
            "device-1",
            "workspace-1",
            WindowsSid,
            ["example.com"],
            [80],
            Time.GetUtcNow().AddHours(1),
            "key-1");
    }

    private sealed class MutableTimeProvider(DateTimeOffset now) : TimeProvider
    {
        private DateTimeOffset _now = now;

        public override DateTimeOffset GetUtcNow() => _now;

        public void Advance(TimeSpan value) => _now += value;
    }

    private sealed class FakeNativeController : INetworkGuardNativeController
    {
        public bool ThrowOnApply { get; set; }
        public List<ActiveNetworkGuardLease> Applied { get; } = [];
        public List<Guid> Removed { get; } = [];

        public Task<NetworkGuardDriverHealth> CheckHealthAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new NetworkGuardDriverHealth(true, true, "test-driver"));

        public Task ResetAsync(CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task ApplyLeaseAsync(
            ActiveNetworkGuardLease lease,
            int httpBrokerPort,
            int httpsBrokerPort,
            CancellationToken cancellationToken = default)
        {
            if (ThrowOnApply) throw new IOException("native apply failed");
            Applied.Add(lease);
            return Task.CompletedTask;
        }

        public Task RemoveLeaseAsync(Guid leaseId, CancellationToken cancellationToken = default)
        {
            Removed.Add(leaseId);
            return Task.CompletedTask;
        }
    }
}
