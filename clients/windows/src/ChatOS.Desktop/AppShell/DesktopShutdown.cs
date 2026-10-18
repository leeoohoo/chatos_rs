namespace ChatOS.Desktop.AppShell;

public static class DesktopShutdown
{
    public static async Task RunAsync(
        IEnumerable<Func<CancellationToken, Task>> cleanupSteps,
        TimeSpan stepTimeout,
        Action<Exception> reportError)
    {
        foreach (var cleanup in cleanupSteps)
        {
            using var deadline = new CancellationTokenSource(stepTimeout);
            try
            {
                // A cleanup delegate can block before it returns its Task (for example,
                // while detaching a native window hook). Invoke every step on its own
                // worker so the deadline still advances to the remaining cleanup and
                // the process-exit fallback.
                await Task.Run(() => cleanup(deadline.Token), CancellationToken.None)
                    .WaitAsync(deadline.Token)
                    .ConfigureAwait(false);
            }
            catch (Exception exception)
            {
                reportError(exception);
            }
        }
    }
}
