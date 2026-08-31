namespace ChatOS.Presentation.Threading;

public interface IUiDispatcher
{
    Task InvokeAsync(Action action, CancellationToken cancellationToken = default);
}

public sealed class SynchronizationContextUiDispatcher : IUiDispatcher
{
    private readonly SynchronizationContext _context;

    public SynchronizationContextUiDispatcher(SynchronizationContext context)
    {
        _context = context;
    }

    public Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
    {
        if (SynchronizationContext.Current == _context)
        {
            cancellationToken.ThrowIfCancellationRequested();
            action();
            return Task.CompletedTask;
        }

        var completion = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        _context.Post(_ =>
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
        }, null);
        return completion.Task;
    }
}

public sealed class ImmediateUiDispatcher : IUiDispatcher
{
    public Task InvokeAsync(Action action, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        action();
        return Task.CompletedTask;
    }
}
