using ChatOS.Presentation.Threading;
using Microsoft.UI.Dispatching;

namespace ChatOS.Desktop.Threading;

public sealed class DispatcherQueueUiDispatcher : IUiDispatcher
{
    private readonly DispatcherQueue _dispatcherQueue;

    public DispatcherQueueUiDispatcher(DispatcherQueue dispatcherQueue)
    {
        _dispatcherQueue = dispatcherQueue;
    }

    public Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
    {
        if (_dispatcherQueue.HasThreadAccess)
        {
            cancellationToken.ThrowIfCancellationRequested();
            action();
            return Task.CompletedTask;
        }

        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        if (!_dispatcherQueue.TryEnqueue(() =>
            {
                if (cancellationToken.IsCancellationRequested)
                {
                    completion.TrySetCanceled(cancellationToken);
                    return;
                }

                try
                {
                    action();
                    completion.TrySetResult();
                }
                catch (Exception exception)
                {
                    completion.TrySetException(exception);
                }
            }))
        {
            completion.TrySetException(new InvalidOperationException("The ChatOS window dispatcher is unavailable."));
        }

        return completion.Task;
    }
}
