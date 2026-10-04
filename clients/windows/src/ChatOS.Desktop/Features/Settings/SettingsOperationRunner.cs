using ChatOS.Presentation.Threading;

namespace ChatOS.Desktop.Features.Settings;

internal sealed class SettingsOperationRunner(IUiDispatcher dispatcher) : IDisposable
{
    private readonly SemaphoreSlim _gate = new(1, 1);

    public async Task RunAsync(
        Func<CancellationToken, Task> operation,
        Action begin,
        Action<string> fail,
        Action complete,
        CancellationToken cancellationToken)
    {
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await dispatcher.InvokeAsync(begin, cancellationToken).ConfigureAwait(false);
            await operation(cancellationToken).ConfigureAwait(false);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await dispatcher.InvokeAsync(() => fail(exception.Message)).ConfigureAwait(false);
        }
        finally
        {
            await dispatcher.InvokeAsync(complete).ConfigureAwait(false);
            _gate.Release();
        }
    }

    public void Dispose() => _gate.Dispose();
}
