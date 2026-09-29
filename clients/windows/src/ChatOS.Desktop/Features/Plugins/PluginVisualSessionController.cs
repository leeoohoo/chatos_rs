namespace ChatOS.Desktop.Features.Plugins;

public sealed class PluginVisualSessionController : IDisposable
{
    private const int DiscoveryRefreshAttempts = 8;
    private static readonly TimeSpan ActiveRefreshInterval = TimeSpan.FromMilliseconds(650);
    private static readonly TimeSpan IdleRefreshInterval = TimeSpan.FromSeconds(5);
    private readonly PluginVisualSessionsViewModel _viewModel;
    private readonly PluginVisualSessionWindow _window;
    private readonly object _lifetimeSync = new();
    private CancellationTokenSource? _lifetimeCancellation;
    private Task? _monitorTask;

    public PluginVisualSessionController(
        PluginVisualSessionsViewModel viewModel,
        PluginVisualSessionWindow window)
    {
        _viewModel = viewModel;
        _window = window;
    }

    public async Task SetAuthenticatedAsync(
        bool authenticated,
        CancellationToken cancellationToken = default)
    {
        if (!authenticated)
        {
            Stop();
            return;
        }

        lock (_lifetimeSync)
        {
            if (_lifetimeCancellation is not null) return;
            _lifetimeCancellation = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        }

        var observedRevision = _viewModel.VisualRevision;
        await _viewModel.RefreshAsync(cancellationToken).ConfigureAwait(false);
        lock (_lifetimeSync)
        {
            if (_lifetimeCancellation is { } lifetime)
            {
                _monitorTask = MonitorAsync(observedRevision, lifetime.Token);
            }
        }
    }

    public void Stop()
    {
        CancellationTokenSource? cancellation;
        lock (_lifetimeSync)
        {
            cancellation = _lifetimeCancellation;
            _lifetimeCancellation = null;
            _monitorTask = null;
        }

        cancellation?.Cancel();
        cancellation?.Dispose();
        _viewModel.Stop();
        _window.Hide();
    }

    public void Dispose() => Stop();

    private async Task MonitorAsync(
        long observedRevision,
        CancellationToken cancellationToken)
    {
        var discoveryRefreshesRemaining = 0;
        try
        {
            while (true)
            {
                if (_viewModel.HasSessions || discoveryRefreshesRemaining > 0)
                {
                    if (!_viewModel.HasSessions) discoveryRefreshesRemaining--;
                    await Task.Delay(ActiveRefreshInterval, cancellationToken).ConfigureAwait(false);
                }
                else
                {
                    var changed = await WaitForChangeOrIdleRefreshAsync(
                        observedRevision,
                        cancellationToken).ConfigureAwait(false);
                    if (changed) discoveryRefreshesRemaining = DiscoveryRefreshAttempts;
                }

                var refreshRevision = _viewModel.VisualRevision;
                await _viewModel.RefreshAsync(cancellationToken).ConfigureAwait(false);
                observedRevision = refreshRevision;
                if (_viewModel.HasSessions) discoveryRefreshesRemaining = 0;
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
    }

    private async Task<bool> WaitForChangeOrIdleRefreshAsync(
        long observedRevision,
        CancellationToken cancellationToken)
    {
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(IdleRefreshInterval);
        try
        {
            await _viewModel.WaitForVisualChangeAsync(observedRevision, timeout.Token)
                .ConfigureAwait(false);
            return true;
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            return false;
        }
    }
}
