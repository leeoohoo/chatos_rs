using ChatOS.Core.Abstractions;
using Microsoft.Extensions.Hosting;

namespace ChatOS.Connector.AgentTeams;

internal sealed class AgentTeamBackgroundService(
    IAgentTeamStore store,
    AgentTeamScheduler scheduler) : BackgroundService
{
    protected override Task ExecuteAsync(CancellationToken stoppingToken) => Task.WhenAll(
        RunLoopAsync(TimeSpan.FromSeconds(2), DrainCommunicationsAsync, stoppingToken),
        RunLoopAsync(TimeSpan.FromSeconds(30), DrainExecutorsAsync, stoppingToken));

    private async Task DrainCommunicationsAsync(CancellationToken cancellationToken)
    {
        var owners = await store.ListOwnersWithPendingDeliveriesAsync(cancellationToken)
            .ConfigureAwait(false);
        foreach (var owner in owners)
        {
            await scheduler.DrainCommunicationAsync(owner, cancellationToken).ConfigureAwait(false);
        }
    }

    private async Task DrainExecutorsAsync(CancellationToken cancellationToken)
    {
        await store.EnqueueDueHeartbeatsAsync(
            DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(), cancellationToken).ConfigureAwait(false);
        var owners = await store.ListOwnersWithPendingDeliveriesAsync(cancellationToken)
            .ConfigureAwait(false);
        foreach (var owner in owners)
        {
            await scheduler.DrainExecutorAsync(owner, cancellationToken).ConfigureAwait(false);
        }
    }

    private static async Task RunLoopAsync(
        TimeSpan interval,
        Func<CancellationToken, Task> tick,
        CancellationToken stoppingToken)
    {
        using var timer = new PeriodicTimer(interval);
        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                await tick(stoppingToken).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
            catch
            {
                // Queue state remains durable; the next bounded tick retries it.
            }

            try
            {
                if (!await timer.WaitForNextTickAsync(stoppingToken).ConfigureAwait(false)) break;
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
        }
    }
}
