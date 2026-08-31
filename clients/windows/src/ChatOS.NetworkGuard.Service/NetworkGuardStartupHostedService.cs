namespace ChatOS.NetworkGuard.Service;

internal sealed class NetworkGuardStartupHostedService(
    INetworkGuardNativeController nativeController,
    NetworkGuardBrokerState brokerState) : IHostedService
{
    public async Task StartAsync(CancellationToken cancellationToken)
    {
        brokerState.SetReady(false);
        await nativeController.ResetAsync(cancellationToken).ConfigureAwait(false);
        var health = await nativeController.CheckHealthAsync(cancellationToken).ConfigureAwait(false);
        if (!health.DriverReady || !health.SelfTestPassed || health.ActiveLeaseCount != 0)
        {
            throw new InvalidOperationException(
                "NetworkGuard driver startup reconciliation failed; broker endpoints remain disabled.");
        }
    }

    public Task StopAsync(CancellationToken cancellationToken)
    {
        brokerState.SetReady(false);
        return Task.CompletedTask;
    }
}
