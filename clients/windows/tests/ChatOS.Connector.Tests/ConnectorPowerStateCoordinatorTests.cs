using ChatOS.Connector.Connection;
using ChatOS.Connector.Runtime;

namespace ChatOS.Connector.Tests;

public sealed class ConnectorPowerStateCoordinatorTests
{
    [Fact]
    public async Task SuspendInvalidatesConnectionAndResumeWakesWaitersOnce()
    {
        var state = new ConnectorConnectionStateMachine();
        state.SetConfigured(true);
        var lease = state.Start();
        var power = new ConnectorPowerStateCoordinator(state);
        var before = power.Snapshot;
        var suspended = power.WaitForChangeAsync(before.Revision, CancellationToken.None);

        power.Suspend();

        Assert.True(await suspended > before.Revision);
        Assert.True(power.Snapshot.IsSuspended);
        Assert.Equal(ConnectorConnectionPhase.Suspended, state.Snapshot.Phase);
        Assert.False(state.MarkConnected(lease, DateTimeOffset.UtcNow));
        var resumeWait = power.WaitForChangeAsync(power.Snapshot.Revision, CancellationToken.None);

        power.Resume();

        Assert.False(power.Snapshot.IsSuspended);
        Assert.Equal(power.Snapshot.Revision, await resumeWait);
    }

    [Fact]
    public async Task DuplicatePowerNotificationsDoNotCreateReconnectStorm()
    {
        var state = new ConnectorConnectionStateMachine();
        state.SetConfigured(true);
        var power = new ConnectorPowerStateCoordinator(state);
        power.Suspend();
        var revision = power.Snapshot.Revision;

        power.Suspend();
        power.Resume();
        var resumedRevision = power.Snapshot.Revision;
        power.Resume();

        Assert.Equal(revision + 1, resumedRevision);
        Assert.Equal(resumedRevision, power.Snapshot.Revision);
        using var cancellation = new CancellationTokenSource(TimeSpan.FromMilliseconds(50));
        await Assert.ThrowsAnyAsync<OperationCanceledException>(() =>
            power.WaitForChangeAsync(resumedRevision, cancellation.Token));
    }
}
