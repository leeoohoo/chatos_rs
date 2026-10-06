using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;
using ChatOS.Connector.Sandbox;
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
        PluginProcessLaunchRequest launch,
        CancellationToken cancellationToken = default);
}

internal sealed class WindowsPluginProcessLauncher : IPluginProcessLauncher
{
    public async Task<IPluginProcess> LaunchAsync(
        PluginProcessLaunchRequest launch,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (OperatingSystem.IsWindows())
        {
            return await LaunchAppContainerAsync(launch, cancellationToken).ConfigureAwait(false);
        }
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

        start.Environment.Clear();
        AddRequiredWindowsEnvironment(start.Environment);
        foreach (var pair in launch.Environment)
        {
            if (pair.Key.Equals("PATH", StringComparison.OrdinalIgnoreCase))
            {
                continue;
            }
            start.Environment[pair.Key] = pair.Value;
        }

        if (launch.Environment.TryGetValue("CHATOS_PLUGIN_DATA_DIR", out var pluginDataDirectory))
        {
            start.Environment["USERPROFILE"] = pluginDataDirectory;
            start.Environment["HOME"] = pluginDataDirectory;
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

            return new SystemPluginProcess(process, job);
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

    private static async Task<IPluginProcess> LaunchAppContainerAsync(
        PluginProcessLaunchRequest launch,
        CancellationToken cancellationToken)
    {
        var permissions = launch.PermissionSnapshot;
        var workspaceProfile = WorkspacePermissionProfile(permissions);
        var policy = InstallationPolicy(launch);
        WindowsAppContainerLaunchContext? sandbox = null;
        SafeFileHandle? inputRead = null;
        SafeFileHandle? inputWrite = null;
        SafeFileHandle? outputRead = null;
        SafeFileHandle? outputWrite = null;
        SafeFileHandle? errorRead = null;
        SafeFileHandle? errorWrite = null;
        IntPtr attributeList = IntPtr.Zero;
        IntPtr inheritedHandles = IntPtr.Zero;
        SafeKernelObjectHandle? job = null;
        try
        {
            sandbox = await WindowsAppContainerSandbox.PrepareAsync(
                launch.InstallationPath,
                policy,
                $"plugin:{launch.Record.PluginId}:{launch.Record.ReleaseId}:{launch.ComponentKey}",
                cancellationToken,
                launch.Environment,
                minimalEnvironment: true).ConfigureAwait(false);
            foreach (var path in WritableRuntimePaths(launch))
            {
                await sandbox.GrantPathAccessAsync(
                    path,
                    ConnectorSandboxPermissionProfile.WorkspaceWrite,
                    cancellationToken).ConfigureAwait(false);
            }
            if (!string.IsNullOrWhiteSpace(launch.WorkspaceRoot) &&
                (permissions.Contains("workspace.read") || permissions.Contains("workspace.write")))
            {
                await sandbox.GrantPathAccessAsync(
                    launch.WorkspaceRoot,
                    workspaceProfile,
                    cancellationToken).ConfigureAwait(false);
            }

            NativeTerminalProcess.CreatePipe(out inputWrite, out inputRead, parentReads: false);
            NativeTerminalProcess.CreatePipe(out outputRead, out outputWrite, parentReads: true);
            NativeTerminalProcess.CreatePipe(out errorRead, out errorWrite, parentReads: true);
            nuint attributeBytes = 0;
            _ = NativeConPty.InitializeProcThreadAttributeList(IntPtr.Zero, 2, 0, ref attributeBytes);
            attributeList = Marshal.AllocHGlobal(checked((nint)attributeBytes));
            NativeConPty.ThrowIfFalse(NativeConPty.InitializeProcThreadAttributeList(
                attributeList, 2, 0, ref attributeBytes));
            inheritedHandles = Marshal.AllocHGlobal(IntPtr.Size * 3);
            Marshal.WriteIntPtr(inheritedHandles, 0, inputRead.DangerousGetHandle());
            Marshal.WriteIntPtr(inheritedHandles, IntPtr.Size, outputWrite.DangerousGetHandle());
            Marshal.WriteIntPtr(inheritedHandles, IntPtr.Size * 2, errorWrite.DangerousGetHandle());
            NativeConPty.ThrowIfFalse(NativeConPty.UpdateProcThreadAttribute(
                attributeList, 0, NativeConPty.ProcThreadAttributeHandleList, inheritedHandles,
                checked((nuint)(IntPtr.Size * 3)), IntPtr.Zero, IntPtr.Zero));
            NativeConPty.ThrowIfFalse(NativeConPty.UpdateProcThreadAttribute(
                attributeList, 0, WindowsAppContainerSandbox.ProcThreadAttributeSecurityCapabilities,
                sandbox.SecurityCapabilities, checked((nuint)Marshal.SizeOf<SecurityCapabilities>()),
                IntPtr.Zero, IntPtr.Zero));
            var startup = new StartupInfoEx
            {
                StartupInfo = new StartupInfo
                {
                    Size = (uint)Marshal.SizeOf<StartupInfoEx>(),
                    Flags = NativeTerminalProcess.StartfUseStdHandles,
                    StandardInput = inputRead.DangerousGetHandle(),
                    StandardOutput = outputWrite.DangerousGetHandle(),
                    StandardError = errorWrite.DangerousGetHandle(),
                },
                AttributeList = attributeList,
            };
            var commandLine = new StringBuilder(WindowsTerminalCommandExecutor.BuildCommandLine(
                launch.ExecutablePath,
                launch.Arguments));
            var flags = NativeConPty.ExtendedStartupInfoPresent |
                NativeConPty.CreateSuspended |
                NativeConPty.CreateUnicodeEnvironment |
                NativeTerminalProcess.CreateNoWindow;
            if (!NativeConPty.CreateProcess(
                    launch.ExecutablePath,
                    commandLine,
                    IntPtr.Zero,
                    IntPtr.Zero,
                    inheritHandles: true,
                    flags,
                    sandbox.EnvironmentBlock,
                    launch.InstallationPath,
                    ref startup,
                    out var processInformation))
            {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            using var nativeProcess = new SafeKernelObjectHandle(processInformation.Process, ownsHandle: true);
            using var nativeThread = new SafeKernelObjectHandle(processInformation.Thread, ownsHandle: true);
            job = NativeConPty.CreateKillOnCloseJob();
            NativeConPty.ThrowIfFalse(NativeConPty.AssignProcessToJobObject(job, nativeProcess));
            var process = Process.GetProcessById(checked((int)processInformation.ProcessId));
            if (NativeConPty.ResumeThread(nativeThread) == uint.MaxValue)
            {
                process.Dispose();
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            inputRead.Dispose(); inputRead = null;
            outputWrite.Dispose(); outputWrite = null;
            errorWrite.Dispose(); errorWrite = null;
            var result = new AppContainerPluginProcess(
                process,
                job,
                sandbox,
                new FileStream(inputWrite, FileAccess.Write, 16 * 1024, isAsync: true),
                new FileStream(outputRead, FileAccess.Read, 16 * 1024, isAsync: true),
                new FileStream(errorRead, FileAccess.Read, 16 * 1024, isAsync: true));
            job = null;
            sandbox = null;
            inputWrite = null;
            outputRead = null;
            errorRead = null;
            return result;
        }
        finally
        {
            inputRead?.Dispose(); inputWrite?.Dispose(); outputRead?.Dispose();
            outputWrite?.Dispose(); errorRead?.Dispose(); errorWrite?.Dispose();
            job?.Dispose();
            if (attributeList != IntPtr.Zero)
            {
                NativeConPty.DeleteProcThreadAttributeList(attributeList);
                Marshal.FreeHGlobal(attributeList);
            }
            if (inheritedHandles != IntPtr.Zero) Marshal.FreeHGlobal(inheritedHandles);
            if (sandbox is not null) await sandbox.DisposeAsync().ConfigureAwait(false);
        }
    }

    internal static SandboxExecutionPolicy InstallationPolicy(PluginProcessLaunchRequest launch) =>
        new(
            UseAppContainer: true,
            PermissionProfile: ConnectorSandboxPermissionProfile.ReadOnly,
            NetworkAccess: launch.NetworkAccess);

    internal static ConnectorSandboxPermissionProfile WorkspacePermissionProfile(
        IReadOnlySet<string> permissions) =>
        permissions.Contains("workspace.write")
            ? ConnectorSandboxPermissionProfile.WorkspaceWrite
            : ConnectorSandboxPermissionProfile.ReadOnly;

    private static IEnumerable<string> WritableRuntimePaths(PluginProcessLaunchRequest launch)
    {
        var keys = new[]
        {
            "CHATOS_PLUGIN_DATA_DIR", "CHATOS_PLUGIN_CACHE_DIR", "CHATOS_PLUGIN_ARTIFACT_DIR",
            "CHATOS_PLUGIN_FILE_GRANT_DIR", "CHATOS_PLUGIN_VISUAL_SESSION_DIR",
        };
        return keys.Select(key => launch.Environment.TryGetValue(key, out var path) ? path : null)
            .Where(path => !string.IsNullOrWhiteSpace(path))!
            .Select(path => Path.GetFullPath(path!))
            .Distinct(StringComparer.OrdinalIgnoreCase);
    }

    internal static void AddRequiredWindowsEnvironment(IDictionary<string, string?> environment)
    {
        var windowsDirectory = Environment.GetFolderPath(Environment.SpecialFolder.Windows);
        var systemDirectory = Environment.SystemDirectory;
        environment["SystemRoot"] = windowsDirectory;
        environment["WINDIR"] = windowsDirectory;
        environment["PATH"] = string.Join(Path.PathSeparator, new[]
        {
            systemDirectory,
            windowsDirectory,
            Path.Combine(systemDirectory, "Wbem"),
        }.Where(path => !string.IsNullOrWhiteSpace(path)));
        environment["TEMP"] = Path.GetTempPath();
        environment["TMP"] = Path.GetTempPath();
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

    private sealed class AppContainerPluginProcess(
        Process process,
        SafeKernelObjectHandle job,
        WindowsAppContainerLaunchContext sandbox,
        Stream input,
        Stream output,
        Stream error) : IPluginProcess
    {
        private int _terminated;
        public Stream StandardInput => input;
        public Stream StandardOutput => output;
        public Stream StandardError => error;
        public bool HasExited { get { try { return process.HasExited; } catch { return true; } } }

        public async Task<int> WaitForExitAsync(CancellationToken cancellationToken = default)
        {
            await process.WaitForExitAsync(cancellationToken).ConfigureAwait(false);
            return process.ExitCode;
        }

        public Task TerminateAsync()
        {
            if (Interlocked.Exchange(ref _terminated, 1) == 0)
            {
                NativeConPty.TerminateJob(job, 1);
            }
            return Task.CompletedTask;
        }

        public async ValueTask DisposeAsync()
        {
            await TerminateAsync().ConfigureAwait(false);
            await input.DisposeAsync().ConfigureAwait(false);
            await output.DisposeAsync().ConfigureAwait(false);
            await error.DisposeAsync().ConfigureAwait(false);
            process.Dispose();
            job.Dispose();
            await sandbox.DisposeAsync().ConfigureAwait(false);
        }
    }
}
