namespace ChatOS.Connector.LocalAgent;

public enum WindowsLocalAgentHostStatus
{
    Stopped,
    Starting,
    Running,
    Restarting,
    Failed,
}

public sealed record WindowsLocalAgentHostState(
    WindowsLocalAgentHostStatus Status,
    string? AccountId = null,
    uint? ProcessId = null,
    string? ClientEndpoint = null,
    int RestartCount = 0,
    string? FailureReason = null);

internal interface IWindowsLocalAgentHostSupervisor : IAsyncDisposable
{
    Task<WindowsLocalAgentHostState> GetStateAsync();

    Task StartAsync(
        string accountId,
        Func<CancellationToken, Task<WindowsLocalAgentHostLaunchConfiguration>> configurationProvider,
        CancellationToken cancellationToken = default);

    Task LogoutAsync();
}

public sealed class WindowsLocalAgentHostSupervisor : IWindowsLocalAgentHostSupervisor
{
    private readonly IWindowsLocalAgentHostProcessLauncher _launcher;
    private readonly IReadOnlyList<TimeSpan> _restartDelays;
    private readonly SemaphoreSlim _gate = new(1, 1);
    private IWindowsLocalAgentHostProcess? _process;
    private Func<CancellationToken, Task<WindowsLocalAgentHostLaunchConfiguration>>? _configurationProvider;
    private CancellationTokenSource? _lifetime;
    private string? _accountId;
    private long _generation;
    private WindowsLocalAgentHostState _state = new(WindowsLocalAgentHostStatus.Stopped);

    public WindowsLocalAgentHostSupervisor()
        : this(null, null)
    {
    }

    internal WindowsLocalAgentHostSupervisor(
        IWindowsLocalAgentHostProcessLauncher? launcher = null,
        IReadOnlyList<TimeSpan>? restartDelays = null)
    {
        _launcher = launcher ?? new WindowsLocalAgentHostProcessLauncher();
        _restartDelays = restartDelays
            ?? [
                TimeSpan.FromSeconds(1),
                TimeSpan.FromSeconds(2),
                TimeSpan.FromSeconds(5),
                TimeSpan.FromSeconds(10),
                TimeSpan.FromSeconds(30),
            ];
        if (_restartDelays.Count == 0
            || _restartDelays.Any(delay => delay < TimeSpan.Zero || delay > TimeSpan.FromMinutes(1)))
        {
            throw new ArgumentOutOfRangeException(nameof(restartDelays));
        }
    }

    public async Task<WindowsLocalAgentHostState> GetStateAsync()
    {
        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            return _state;
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task StartAsync(
        string accountId,
        Func<CancellationToken, Task<WindowsLocalAgentHostLaunchConfiguration>> configurationProvider,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(accountId);
        ArgumentNullException.ThrowIfNull(configurationProvider);
        if (!string.Equals(accountId, accountId.Trim(), StringComparison.Ordinal))
        {
            throw new ArgumentException("Account identity is invalid.", nameof(accountId));
        }

        await LogoutAsync().ConfigureAwait(false);
        CancellationTokenSource lifetime;
        long generation;
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            _accountId = accountId;
            _configurationProvider = configurationProvider;
            _lifetime = new CancellationTokenSource();
            lifetime = _lifetime;
            generation = checked(++_generation);
            _state = new(WindowsLocalAgentHostStatus.Starting, accountId);
        }
        finally
        {
            _gate.Release();
        }

        try
        {
            using var startup = CancellationTokenSource.CreateLinkedTokenSource(
                cancellationToken,
                lifetime.Token);
            await LaunchAsync(
                accountId,
                0,
                generation,
                startup.Token,
                lifetime.Token).ConfigureAwait(false);
        }
        catch (Exception error)
        {
            await FailInitialStartAsync(accountId, generation, error).ConfigureAwait(false);
            throw;
        }
    }

    public async Task LogoutAsync()
    {
        IWindowsLocalAgentHostProcess? process;
        CancellationTokenSource? lifetime;
        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            checked { _generation++; }
            _accountId = null;
            _configurationProvider = null;
            lifetime = _lifetime;
            _lifetime = null;
            process = _process;
            _process = null;
            _state = new(WindowsLocalAgentHostStatus.Stopped);
        }
        finally
        {
            _gate.Release();
        }

        lifetime?.Cancel();
        lifetime?.Dispose();
        if (process is not null)
        {
            await process.TerminateAsync().ConfigureAwait(false);
            await process.DisposeAsync().ConfigureAwait(false);
        }
    }

    public async ValueTask DisposeAsync()
    {
        await LogoutAsync().ConfigureAwait(false);
        _gate.Dispose();
    }

    private async Task LaunchAsync(
        string accountId,
        int restartCount,
        long generation,
        CancellationToken cancellationToken,
        CancellationToken monitorCancellationToken)
    {
        Func<CancellationToken, Task<WindowsLocalAgentHostLaunchConfiguration>> provider;
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_generation != generation || _accountId != accountId)
            {
                return;
            }

            provider = _configurationProvider
                ?? throw new InvalidOperationException("Host configuration provider is unavailable.");
        }
        finally
        {
            _gate.Release();
        }

        var configuration = await provider(cancellationToken).ConfigureAwait(false);
        var launched = await _launcher.LaunchAsync(configuration, cancellationToken).ConfigureAwait(false);
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_generation != generation || _accountId != accountId)
            {
                await launched.TerminateAsync().ConfigureAwait(false);
                await launched.DisposeAsync().ConfigureAwait(false);
                return;
            }

            _process = launched;
            _state = new(
                WindowsLocalAgentHostStatus.Running,
                accountId,
                launched.Ready.ProcessId,
                launched.Ready.ClientEndpoint,
                restartCount);
        }
        finally
        {
            _gate.Release();
        }

        _ = MonitorAsync(
            launched,
            accountId,
            restartCount,
            generation,
            monitorCancellationToken);
    }

    private async Task MonitorAsync(
        IWindowsLocalAgentHostProcess launched,
        string accountId,
        int restartCount,
        long generation,
        CancellationToken cancellationToken)
    {
        int exitCode;
        try
        {
            exitCode = await launched.WaitForExitAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
            return;
        }

        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            if (!ReferenceEquals(_process, launched)
                || _generation != generation
                || _accountId != accountId)
            {
                return;
            }

            _process = null;
        }
        finally
        {
            _gate.Release();
        }

        await launched.DisposeAsync().ConfigureAwait(false);
        var failureReason = $"Local Agent Host exited unexpectedly with code {exitCode}.";
        for (var offset = 0; offset < _restartDelays.Count; offset++)
        {
            var attempt = restartCount + offset + 1;
            await SetRestartingAsync(accountId, attempt, generation).ConfigureAwait(false);
            try
            {
                await Task.Delay(_restartDelays[offset], cancellationToken).ConfigureAwait(false);
                await LaunchAsync(
                    accountId,
                    attempt,
                    generation,
                    cancellationToken,
                    cancellationToken).ConfigureAwait(false);
                return;
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (Exception error)
            {
                failureReason = Sanitize(error);
            }
        }

        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            if (_generation == generation && _accountId == accountId)
            {
                _accountId = null;
                _configurationProvider = null;
                _state = new(
                    WindowsLocalAgentHostStatus.Failed,
                    accountId,
                    FailureReason: failureReason);
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    private async Task SetRestartingAsync(string accountId, int attempt, long generation)
    {
        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            if (_generation == generation && _accountId == accountId)
            {
                _state = new(
                    WindowsLocalAgentHostStatus.Restarting,
                    accountId,
                    RestartCount: attempt);
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    private async Task FailInitialStartAsync(string accountId, long generation, Exception error)
    {
        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            if (_generation == generation && _accountId == accountId)
            {
                _accountId = null;
                _configurationProvider = null;
                _lifetime?.Dispose();
                _lifetime = null;
                _state = new(
                    WindowsLocalAgentHostStatus.Failed,
                    accountId,
                    FailureReason: Sanitize(error));
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    private static string Sanitize(Exception error) => error switch
    {
        WindowsLocalAgentHostLaunchException launch => launch.Message,
        _ => "Local Agent Host failed to start.",
    };
}
