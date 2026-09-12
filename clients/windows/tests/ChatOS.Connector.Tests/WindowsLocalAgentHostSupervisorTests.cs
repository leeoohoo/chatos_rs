using ChatOS.Connector.LocalAgent;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentHostSupervisorTests
{
    [Fact]
    public async Task CrashedHostRestartsWithFreshLaunchMaterial()
    {
        var launcher = new FakeLauncher();
        await using var supervisor = new WindowsLocalAgentHostSupervisor(
            launcher,
            [TimeSpan.Zero]);
        var providerCalls = 0;
        await supervisor.StartAsync("user-1", _ =>
        {
            providerCalls++;
            return Task.FromResult(Configuration(providerCalls));
        });

        launcher.Processes[0].Exit(7);
        await WaitUntilAsync(async () =>
            (await supervisor.GetStateAsync()).RestartCount == 1);

        Assert.Equal(2, providerCalls);
        Assert.Equal(2, launcher.Processes.Count);
        Assert.Equal(WindowsLocalAgentHostStatus.Running, (await supervisor.GetStateAsync()).Status);
    }

    [Fact]
    public async Task LogoutTerminatesWithoutRestarting()
    {
        var launcher = new FakeLauncher();
        await using var supervisor = new WindowsLocalAgentHostSupervisor(
            launcher,
            [TimeSpan.Zero]);
        var providerCalls = 0;
        await supervisor.StartAsync("user-1", _ =>
        {
            providerCalls++;
            return Task.FromResult(Configuration(providerCalls));
        });

        await supervisor.LogoutAsync();
        await Task.Delay(50);

        Assert.Equal(1, providerCalls);
        Assert.True(launcher.Processes[0].Terminated);
        Assert.Equal(WindowsLocalAgentHostStatus.Stopped, (await supervisor.GetStateAsync()).Status);
    }

    [Fact]
    public void LaunchMaterialIsZeroizedWhenDisposed()
    {
        var source = new byte[] { 1, 2, 3, 4 };
        var material = new WindowsLocalAgentHostLaunchMaterial(source);
        source[0] = 9;

        Assert.Equal(1, material.RequestJson.Span[0]);
        material.Dispose();

        Assert.Throws<ObjectDisposedException>(() => _ = material.RequestJson);
    }

    private static WindowsLocalAgentHostLaunchConfiguration Configuration(int revision) => new()
    {
        ExecutablePath = @"C:\Program Files\ChatOS\local-agent-host.exe",
        ExpectedExecutableSha256 = $"sha256:{new string('0', 64)}",
        LaunchId = $"launch-{revision}",
        ExpectedClientEndpoint = $@"\\.\pipe\chatos-local-agent-{revision:D16}",
        LaunchMaterial = new WindowsLocalAgentHostLaunchMaterial([1, 2, 3]),
    };

    private static async Task WaitUntilAsync(Func<Task<bool>> predicate)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(3));
        while (!await predicate())
        {
            await Task.Delay(10, timeout.Token);
        }
    }

    private sealed class FakeLauncher : IWindowsLocalAgentHostProcessLauncher
    {
        public List<FakeProcess> Processes { get; } = [];

        public Task<IWindowsLocalAgentHostProcess> LaunchAsync(
            WindowsLocalAgentHostLaunchConfiguration configuration,
            CancellationToken cancellationToken = default)
        {
            cancellationToken.ThrowIfCancellationRequested();
            configuration.LaunchMaterial.Dispose();
            var process = new FakeProcess(new WindowsLocalAgentHostReady(
                1,
                configuration.LaunchId,
                (uint)(Processes.Count + 10),
                configuration.ExpectedClientEndpoint));
            Processes.Add(process);
            return Task.FromResult<IWindowsLocalAgentHostProcess>(process);
        }
    }

    private sealed class FakeProcess(WindowsLocalAgentHostReady ready) : IWindowsLocalAgentHostProcess
    {
        private readonly TaskCompletionSource<int> _exit =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public WindowsLocalAgentHostReady Ready { get; } = ready;

        public bool Terminated { get; private set; }

        public void Exit(int code) => _exit.TrySetResult(code);

        public Task<int> WaitForExitAsync(CancellationToken cancellationToken = default) =>
            _exit.Task.WaitAsync(cancellationToken);

        public Task TerminateAsync()
        {
            Terminated = true;
            _exit.TrySetResult(1);
            return Task.CompletedTask;
        }

        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }
}
