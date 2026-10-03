using System.Diagnostics;

namespace ChatOS.Connector.LocalAgent;

internal interface ILocalAgentHostProcess : IAsyncDisposable
{
    event EventHandler? Exited;

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
        IReadOnlyDictionary<string, string> credentialEnvironment,
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
        IReadOnlyDictionary<string, string> credentialEnvironment,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        Directory.CreateDirectory(Path.GetDirectoryName(options.DatabasePath)
            ?? throw new InvalidOperationException("Local Agent database path has no parent."));
        var start = CreateStartInfo(options, ownerUserId, credentialEnvironment);

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

    internal static ProcessStartInfo CreateStartInfo(
        LocalAgentHostOptions options,
        string ownerUserId,
        IReadOnlyDictionary<string, string> credentialEnvironment)
    {
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
        start.ArgumentList.Add("--memory-base-url");
        start.ArgumentList.Add(options.MemoryBaseUri.AbsoluteUri);
        start.ArgumentList.Add("--memory-source-id");
        start.ArgumentList.Add(options.MemorySourceId);
        start.ArgumentList.Add("--memory-timeout-ms");
        start.ArgumentList.Add(((long)options.MemoryTimeout.TotalMilliseconds).ToString(
            System.Globalization.CultureInfo.InvariantCulture));
        foreach (var name in new[]
        {
            "local_attachment_read", "project_list", "project_read", "project_search",
            "capability_search", "capability_describe", "capability_skill_activate",
            "capability_skill_read_resource",
        })
        {
            start.ArgumentList.Add("--read-only-tool");
            start.ArgumentList.Add(name);
        }
        start.ArgumentList.Add("--approval-exempt-tool");
        start.ArgumentList.Add("capability_invoke");
        start.ArgumentList.Add("--stdio");
        start.Environment.Clear();
        foreach (var name in AllowedEnvironmentVariables)
        {
            var value = Environment.GetEnvironmentVariable(name);
            if (!string.IsNullOrEmpty(value)) start.Environment[name] = value;
        }
        foreach (var (name, value) in credentialEnvironment)
        {
            start.Environment[name] = value;
        }
        return start;
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

    private sealed class SystemLocalAgentHostProcess : ILocalAgentHostProcess
    {
        private readonly Process _process;
        private int _terminated;

        public event EventHandler? Exited;

        public SystemLocalAgentHostProcess(Process process)
        {
            _process = process;
            _process.Exited += OnExited;
        }

        public Stream StandardInput => _process.StandardInput.BaseStream;

        public Stream StandardOutput => _process.StandardOutput.BaseStream;

        public bool HasExited
        {
            get
            {
                try { return _process.HasExited; }
                catch (InvalidOperationException) { return true; }
            }
        }

        public Task TerminateAsync()
        {
            if (Interlocked.Exchange(ref _terminated, 1) != 0) return Task.CompletedTask;
            try
            {
                _process.StandardInput.Close();
                if (!_process.HasExited) _process.Kill(entireProcessTree: true);
            }
            catch (InvalidOperationException)
            {
            }
            return Task.CompletedTask;
        }

        public async ValueTask DisposeAsync()
        {
            await TerminateAsync().ConfigureAwait(false);
            _process.Exited -= OnExited;
            _process.Dispose();
        }

        private void OnExited(object? sender, EventArgs args) => Exited?.Invoke(this, EventArgs.Empty);
    }
}
