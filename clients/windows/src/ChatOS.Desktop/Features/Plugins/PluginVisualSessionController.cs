namespace ChatOS.Desktop.Features.Plugins;

public sealed class PluginVisualSessionController : IDisposable
{
    private static readonly TimeSpan RefreshInterval = TimeSpan.FromMilliseconds(650);
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

        await _viewModel.RefreshAsync(cancellationToken).ConfigureAwait(false);
        lock (_lifetimeSync)
        {
            if (_lifetimeCancellation is { } lifetime)
            {
                _monitorTask = MonitorAsync(lifetime.Token);
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

    private async Task MonitorAsync(CancellationToken cancellationToken)
    {
        using var timer = new PeriodicTimer(RefreshInterval);
        try
        {
            while (await timer.WaitForNextTickAsync(cancellationToken).ConfigureAwait(false))
            {
                await _viewModel.RefreshAsync(cancellationToken).ConfigureAwait(false);
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
    }
}
