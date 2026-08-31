using ChatOS.Connector.Connection;

namespace ChatOS.Connector.Tests;

public sealed class ConnectorConnectionStateMachineTests
{
    [Fact]
    public void ReconnectPolicyUsesBoundedExponentialDelays()
    {
        var policy = new ConnectorReconnectPolicy();

        Assert.Equal(TimeSpan.Zero, policy.DelayAfterFailure(0));
        Assert.Equal(TimeSpan.FromSeconds(1), policy.DelayAfterFailure(1));
        Assert.Equal(TimeSpan.FromSeconds(2), policy.DelayAfterFailure(2));
        Assert.Equal(TimeSpan.FromSeconds(16), policy.DelayAfterFailure(5));
        Assert.Equal(TimeSpan.FromSeconds(30), policy.DelayAfterFailure(6));
        Assert.Equal(TimeSpan.FromSeconds(30), policy.DelayAfterFailure(100));
    }

    [Fact]
    public void StaleConnectionCannotOverwriteNewGeneration()
    {
        var machine = new ConnectorConnectionStateMachine();
        machine.SetConfigured(true);
        var oldLease = machine.Start();
        machine.Stop();
        var currentLease = machine.Start();
        var timestamp = new DateTimeOffset(2026, 8, 30, 8, 0, 0, TimeSpan.Zero);

        Assert.False(machine.MarkConnected(oldLease, timestamp));
        Assert.True(machine.MarkConnected(currentLease, timestamp));
        Assert.Equal(ConnectorConnectionPhase.Connected, machine.Snapshot.Phase);
        Assert.Equal(timestamp, machine.Snapshot.ConnectedAt);
    }

    [Fact]
    public void FailureReconnectAndPongFollowOneAuthoritativeGeneration()
    {
        var machine = new ConnectorConnectionStateMachine();
        machine.SetConfigured(true);
        var firstLease = machine.Start();
        Assert.True(machine.MarkFailed(firstLease, "offline"));
        Assert.Equal(1, machine.Snapshot.ConsecutiveFailures);

        var reconnectLease = machine.BeginReconnect();
        var connectedAt = DateTimeOffset.UtcNow;
        Assert.True(machine.MarkConnected(reconnectLease, connectedAt));
        Assert.Equal(0, machine.Snapshot.ConsecutiveFailures);

        var pongAt = connectedAt.AddSeconds(15);
        Assert.True(machine.MarkPong(reconnectLease, pongAt));
        Assert.Equal(pongAt, machine.Snapshot.LastPongAt);
    }

    [Fact]
    public void SuspendInvalidatesLiveConnectionAndResumeCreatesNewLease()
    {
        var machine = new ConnectorConnectionStateMachine();
        machine.SetConfigured(true);
        var oldLease = machine.Start();
        machine.Suspend();

        Assert.False(machine.MarkConnected(oldLease, DateTimeOffset.UtcNow));
        Assert.Equal(ConnectorConnectionPhase.Suspended, machine.Snapshot.Phase);

        var resumedLease = machine.Resume();
        Assert.NotEqual(oldLease, resumedLease);
        Assert.True(machine.MarkConnected(resumedLease, DateTimeOffset.UtcNow));
    }
}
