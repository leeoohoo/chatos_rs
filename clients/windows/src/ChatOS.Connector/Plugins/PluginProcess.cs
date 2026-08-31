using System.Diagnostics;
using ChatOS.Connector.Terminal;
using Microsoft.Win32.SafeHandles;

namespace ChatOS.Connector.Plugins;

internal interface IPluginProcess : IAsyncDisposable
{
    Stream StandardInput { get; }

    Stream StandardOutput { get; }

    Stream StandardError { get; }

    bool HasExited { get; }

    Task<int> WaitForExitAsync(CancellationToken cancellationToken = default);

    Task TerminateAsync();
}

internal interface IPluginProcessLauncher
{
    Task<IPluginProcess> LaunchAsync(
        PreparedPluginLaunch launch,
        CancellationToken cancellationToken = default);
}

internal sealed class WindowsPluginProcessLauncher : IPluginProcessLauncher
{
    public Task<IPluginProcess> LaunchAsync(
        PreparedPluginLaunch launch,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var start = new ProcessStartInfo
        {
            FileName = launch.ExecutablePath,
            WorkingDirectory = launch.InstallationPath,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        foreach (var argument in launch.Arguments)
        {
            start.ArgumentList.Add(argument);
        }

        foreach (var pair in launch.Environment)
        {
            start.Environment[pair.Key] = pair.Value;
        }

        var process = new Process
        {
            StartInfo = start,
            EnableRaisingEvents = true,
        };
        SafeKernelObjectHandle? job = null;
        try
        {
            if (!process.Start())
            {
                throw new PluginRuntimeException("Plugin MCP process could not be started.");
            }

            if (OperatingSystem.IsWindows())
            {
                job = NativeConPty.CreateKillOnCloseJob();
                NativeConPty.ThrowIfFalse(NativeConPty.AssignProcessToJobObject(job, process.SafeHandle));
            }

            return Task.FromResult<IPluginProcess>(new SystemPluginProcess(process, job));
        }
        catch
        {
            job?.Dispose();
            try
            {
                if (!process.HasExited)
                {
                    process.Kill(entireProcessTree: true);
                }
            }
            catch
            {
            }

            process.Dispose();
            throw;
        }
    }

    private sealed class SystemPluginProcess(
        Process process,
        SafeKernelObjectHandle? job) : IPluginProcess
    {
        private int _terminated;

        public Stream StandardInput => process.StandardInput.BaseStream;

        public Stream StandardOutput => process.StandardOutput.BaseStream;

        public Stream StandardError => process.StandardError.BaseStream;

        public bool HasExited
        {
            get
            {
                try
                {
                    return process.HasExited;
                }
                catch (InvalidOperationException)
                {
                    return true;
                }
            }
        }

        public async Task<int> WaitForExitAsync(CancellationToken cancellationToken = default)
        {
            await process.WaitForExitAsync(cancellationToken).ConfigureAwait(false);
            return process.ExitCode;
        }

        public Task TerminateAsync()
        {
            if (Interlocked.Exchange(ref _terminated, 1) != 0)
            {
                return Task.CompletedTask;
            }

            if (job is not null)
            {
                NativeConPty.TerminateJob(job, 1);
                job.Dispose();
            }
            else
            {
                try
                {
                    if (!process.HasExited)
                    {
                        process.Kill(entireProcessTree: true);
                    }
                }
                catch (InvalidOperationException)
                {
                }
            }

            return Task.CompletedTask;
        }

        public async ValueTask DisposeAsync()
        {
            await TerminateAsync().ConfigureAwait(false);
            process.Dispose();
            job?.Dispose();
        }
    }
}
