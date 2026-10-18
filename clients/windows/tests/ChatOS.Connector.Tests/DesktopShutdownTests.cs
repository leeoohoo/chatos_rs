using ChatOS.Desktop.AppShell;

namespace ChatOS.Connector.Tests;

public sealed class DesktopShutdownTests
{
    [Fact]
    public async Task ShutdownWaitsForEachCleanupBeforeStartingTheNext()
    {
        var started = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var order = new List<string>();
        var shutdown = DesktopShutdown.RunAsync(
            [
                async token =>
                {
                    order.Add("terminal");
                    started.SetResult();
                    await completion.Task;
                    token.ThrowIfCancellationRequested();
                },
                _ => { order.Add("remote"); return Task.CompletedTask; },
                _ => { order.Add("host"); return Task.CompletedTask; },
            ], TimeSpan.FromSeconds(5), exception => throw new Exception("Unexpected cleanup failure", exception));
        await started.Task.WaitAsync(TimeSpan.FromSeconds(2));
        Assert.Equal(["terminal"], order);
        completion.SetResult();
        await shutdown;
        Assert.Equal(["terminal", "remote", "host"], order);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public async Task FailedCleanupDoesNotPreventRemainingServicesFromStopping(bool synchronous)
    {
        var stopped = false;
        var errors = new List<Exception>();
        await DesktopShutdown.RunAsync(
            [
                _ => synchronous ? throw new IOException("terminal failure") : Task.FromException(new IOException("terminal failure")),
                _ => { stopped = true; return Task.CompletedTask; },
            ], TimeSpan.FromSeconds(5), errors.Add);
        Assert.True(stopped);
        Assert.IsType<IOException>(Assert.Single(errors));
    }

    [Fact]
    public async Task UnresponsiveCleanupTimesOutAndStillStopsTheHost()
    {
        var pending = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        var errors = new List<Exception>();
        var blockedToken = CancellationToken.None;
        var hostStopped = false;
        await DesktopShutdown.RunAsync(
            [
                token => { blockedToken = token; return pending.Task; },
                token => { Assert.False(token.IsCancellationRequested); hostStopped = true; return Task.CompletedTask; },
            ], TimeSpan.FromMilliseconds(100), errors.Add).WaitAsync(TimeSpan.FromSeconds(5));
        Assert.True(blockedToken.IsCancellationRequested);
        Assert.True(hostStopped);
        Assert.IsAssignableFrom<OperationCanceledException>(Assert.Single(errors));
        pending.SetResult();
    }

    [Fact]
    public async Task CleanupThatBlocksBeforeReturningATaskStillTimesOut()
    {
        using var release = new ManualResetEventSlim();
        var errors = new List<Exception>();
        var nextStepRan = false;

        await DesktopShutdown.RunAsync(
            [
                _ =>
                {
                    release.Wait();
                    return Task.CompletedTask;
                },
                _ =>
                {
                    nextStepRan = true;
                    return Task.CompletedTask;
                },
            ], TimeSpan.FromMilliseconds(100), errors.Add).WaitAsync(TimeSpan.FromSeconds(5));

        Assert.True(nextStepRan);
        Assert.IsAssignableFrom<OperationCanceledException>(Assert.Single(errors));
        release.Set();
    }
}
