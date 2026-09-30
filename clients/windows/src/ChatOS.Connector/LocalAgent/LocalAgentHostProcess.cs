using System.Diagnostics;

namespace ChatOS.Connector.LocalAgent;

internal interface ILocalAgentHostProcess : IAsyncDisposable
{
    Stream StandardInput { get; }

    Stream StandardOutput { get; }

    bool HasExited { get; }

    Task TerminateAsync();
}

internal interface ILocalAgentHostProcessLauncher
{
    Task<ILocalAgentHostProcess> LaunchAsync(
        LocalAgentHostOptions options,
        string ownerUserId,
        CancellationToken cancellationToken);
}

internal sealed class LocalAgentHostProcessLauncher : ILocalAgentHostProcessLauncher
{
    private static readonly string[] AllowedEnvironmentVariables =
    [
        "LOCALAPPDATA", "PATH", "SystemRoot", "TEMP", "TMP", "USERPROFILE",
    ];

    public Task<ILocalAgentHostProcess> LaunchAsync(
        LocalAgentHostOptions options,
        string ownerUserId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        Directory.CreateDirectory(Path.GetDirectoryName(options.DatabasePath)
            ?? throw new InvalidOperationException("Local Agent database path has no parent."));
        var start = new ProcessStartInfo
        {
            FileName = Path.GetFullPath(options.ExecutablePath),
            WorkingDirectory = Path.GetDirectoryName(Path.GetFullPath(options.ExecutablePath))!,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        start.ArgumentList.Add("--database");
        start.ArgumentList.Add(Path.GetFullPath(options.DatabasePath));
        start.ArgumentList.Add("--owner-user-id");
        start.ArgumentList.Add(ownerUserId);
        start.ArgumentList.Add("--stdio");
        start.Environment.Clear();
        foreach (var name in AllowedEnvironmentVariables)
        {
            var value = Environment.GetEnvironmentVariable(name);
            if (!string.IsNullOrEmpty(value)) start.Environment[name] = value;
        }

        var process = new Process { StartInfo = start, EnableRaisingEvents = true };
        try
        {
            if (!process.Start())
            {
                throw new InvalidOperationException("Local Agent Host could not be started.");
            }
            _ = DrainStandardErrorAsync(process.StandardError);
            return Task.FromResult<ILocalAgentHostProcess>(new SystemLocalAgentHostProcess(process));
        }
        catch
        {
            process.Dispose();
            throw;
        }
    }

    private static async Task DrainStandardErrorAsync(StreamReader reader)
    {
        var buffer = new char[4096];
        try
        {
            while (await reader.ReadAsync(buffer).ConfigureAwait(false) > 0)
            {
                // Host diagnostics are intentionally drained, not logged: they may
                // contain local paths and are not required for protocol errors.
            }
        }
        catch (ObjectDisposedException)
        {
        }
        catch (IOException)
        {
        }
    }

    private sealed class SystemLocalAgentHostProcess(Process process) : ILocalAgentHostProcess
    {
        private int _terminated;

        public Stream StandardInput => process.StandardInput.BaseStream;

        public Stream StandardOutput => process.StandardOutput.BaseStream;

        public bool HasExited
        {
            get
            {
                try { return process.HasExited; }
                catch (InvalidOperationException) { return true; }
            }
        }

        public Task TerminateAsync()
        {
            if (Interlocked.Exchange(ref _terminated, 1) != 0) return Task.CompletedTask;
            try
            {
                process.StandardInput.Close();
                if (!process.HasExited) process.Kill(entireProcessTree: true);
            }
            catch (InvalidOperationException)
            {
            }
            return Task.CompletedTask;
        }

        public async ValueTask DisposeAsync()
        {
            await TerminateAsync().ConfigureAwait(false);
            process.Dispose();
        }
    }
}
