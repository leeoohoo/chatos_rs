using ChatOS.Core.Abstractions;
using Microsoft.Extensions.Hosting;

namespace ChatOS.Connector.AgentTeams;

internal sealed class AgentTeamBackgroundService(
    IAgentTeamStore store,
    AgentTeamScheduler scheduler) : BackgroundService
{
    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        using var timer = new PeriodicTimer(TimeSpan.FromSeconds(30));
        while (!stoppingToken.IsCancellationRequested)
        {
            try
            {
                await store.EnqueueDueHeartbeatsAsync(
                    DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(), stoppingToken).ConfigureAwait(false);
                var owners = await store.ListOwnersWithPendingDeliveriesAsync(stoppingToken)
                    .ConfigureAwait(false);
                foreach (var owner in owners)
                {
                    await scheduler.DrainAsync(owner, stoppingToken).ConfigureAwait(false);
                }
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                break;
            }
            catch
            {
                // Queue state remains durable; the next bounded tick retries it.
            }

            if (!await timer.WaitForNextTickAsync(stoppingToken).ConfigureAwait(false))
            {
                break;
            }
        }
    }
}
