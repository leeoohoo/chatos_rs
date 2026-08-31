using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;
using ChatOS.Connector.NetworkGuard;
using ChatOS.Connector.Workspaces;
using ChatOS.Connector.Sandbox;

namespace ChatOS.Connector.Terminal;

public sealed class WindowsTerminalCommandExecutor(
    SandboxExecutionPolicyProvider? sandboxPolicyProvider = null,
    NetworkGuardLeaseCoordinator? networkGuard = null) : ITerminalCommandExecutor
{
    internal const int DefaultTimeoutMilliseconds = 120_000;
    internal const int MinimumTimeoutMilliseconds = 1_000;
    internal const int MaximumTimeoutMilliseconds = 15 * 60_000;
    private const int MaximumPreviewBytes = 512 * 1_024;

    public async Task<TerminalCommandResult> ExecuteAsync(
        TerminalCommandRequest request,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(request.Command);
        ArgumentException.ThrowIfNullOrWhiteSpace(request.WorkingDirectory);
        var timeoutMilliseconds = NormalizeTimeout(request.TimeoutMilliseconds);
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException("Native terminal execution requires Windows.");
        }

        var sandboxPolicy = sandboxPolicyProvider is null
            ? SandboxExecutionPolicy.FromSettings(new ConnectorSandboxSettings(
                false,
                ConnectorSandboxPermissionProfile.FullAccess,
                ConnectorSandboxNetworkAccess.Host))
            : await sandboxPolicyProvider.ResolveAsync(cancellationToken).ConfigureAwait(false);
        return await ExecuteWindowsAsync(
            request with { TimeoutMilliseconds = timeoutMilliseconds },
            sandboxPolicy,
            cancellationToken).ConfigureAwait(false);
    }

    internal static int NormalizeTimeout(int value) => value <= 0
        ? DefaultTimeoutMilliseconds
        : Math.Clamp(value, MinimumTimeoutMilliseconds, MaximumTimeoutMilliseconds);

    private async Task<TerminalCommandResult> ExecuteWindowsAsync(
        TerminalCommandRequest request,
        SandboxExecutionPolicy sandboxPolicy,
        CancellationToken cancellationToken)
    {
        SafeFileHandle? standardOutputRead = null;
        SafeFileHandle? standardOutputWrite = null;
        SafeFileHandle? standardErrorRead = null;
        SafeFileHandle? standardErrorWrite = null;
        SafeFileHandle? standardInputRead = null;
        SafeFileHandle? standardInputWrite = null;
        IntPtr attributeList = IntPtr.Zero;
        IntPtr inheritedHandles = IntPtr.Zero;
        WindowsAppContainerLaunchContext? sandbox = null;
        NetworkGuardLeaseLifetime? networkLease = null;
        try
        {
            var controlled = sandboxPolicy.NetworkAccess is ConnectorSandboxNetworkAccess.Controlled;
            if (controlled)
            {
                if (networkGuard is null || request.NetworkPolicy is null)
                {
                    throw new InvalidOperationException(
                        "Controlled networking requires a signed policy and an available NetworkGuard client.");
                }
                if (!string.Equals(
                        request.NetworkPolicy.WorkspaceId,
                        request.WorkspaceId,
                        StringComparison.Ordinal))
                {
                    throw new InvalidOperationException(
                        "Controlled network policy does not belong to this workspace.");
                }
            }
            if (sandboxPolicy.UseAppContainer)
            {
                var isolationKey = controlled
                    ? $"{request.NetworkPolicy!.PolicyRevision}:{Guid.NewGuid():N}"
                    : null;
                sandbox = await WindowsAppContainerSandbox.PrepareAsync(
                    request.WorkspaceRoot,
                    sandboxPolicy,
                    isolationKey,
                    cancellationToken).ConfigureAwait(false);
            }
            NativeTerminalProcess.CreatePipe(
                out standardOutputRead,
                out standardOutputWrite,
                parentReads: true);
            NativeTerminalProcess.CreatePipe(
                out standardErrorRead,
                out standardErrorWrite,
                parentReads: true);
            NativeTerminalProcess.CreatePipe(
                out standardInputWrite,
                out standardInputRead,
                parentReads: false);

            nuint attributeBytes = 0;
            _ = NativeConPty.InitializeProcThreadAttributeList(
                IntPtr.Zero,
                sandbox is null ? 1 : 2,
                0,
                ref attributeBytes);
            attributeList = Marshal.AllocHGlobal(checked((nint)attributeBytes));
            NativeConPty.ThrowIfFalse(NativeConPty.InitializeProcThreadAttributeList(
                attributeList,
                sandbox is null ? 1 : 2,
                0,
                ref attributeBytes));
            inheritedHandles = Marshal.AllocHGlobal(IntPtr.Size * 3);
            Marshal.WriteIntPtr(inheritedHandles, 0, standardInputRead.DangerousGetHandle());
            Marshal.WriteIntPtr(inheritedHandles, IntPtr.Size, standardOutputWrite.DangerousGetHandle());
            Marshal.WriteIntPtr(inheritedHandles, IntPtr.Size * 2, standardErrorWrite.DangerousGetHandle());
            NativeConPty.ThrowIfFalse(NativeConPty.UpdateProcThreadAttribute(
                attributeList,
                0,
                NativeConPty.ProcThreadAttributeHandleList,
                inheritedHandles,
                checked((nuint)(IntPtr.Size * 3)),
                IntPtr.Zero,
                IntPtr.Zero));
            if (sandbox is not null)
            {
                NativeConPty.ThrowIfFalse(NativeConPty.UpdateProcThreadAttribute(
                    attributeList,
                    0,
                    WindowsAppContainerSandbox.ProcThreadAttributeSecurityCapabilities,
                    sandbox.SecurityCapabilities,
                    checked((nuint)Marshal.SizeOf<SecurityCapabilities>()),
                    IntPtr.Zero,
                    IntPtr.Zero));
            }

            var startup = new StartupInfoEx
            {
                StartupInfo = new StartupInfo
                {
                    Size = (uint)Marshal.SizeOf<StartupInfoEx>(),
                    Flags = NativeTerminalProcess.StartfUseStdHandles,
                    StandardInput = standardInputRead.DangerousGetHandle(),
                    StandardOutput = standardOutputWrite.DangerousGetHandle(),
                    StandardError = standardErrorWrite.DangerousGetHandle(),
                },
                AttributeList = attributeList,
            };
            var launch = ResolveLaunch(request);
            var commandLine = new StringBuilder(BuildCommandLine(launch.Executable, launch.Arguments));
            if (!NativeConPty.CreateProcess(
                    launch.Executable,
                    commandLine,
                    IntPtr.Zero,
                    IntPtr.Zero,
                    inheritHandles: true,
                    NativeConPty.ExtendedStartupInfoPresent |
                        NativeConPty.CreateSuspended |
                        NativeConPty.CreateUnicodeEnvironment |
                        NativeTerminalProcess.CreateNoWindow,
                    sandbox?.EnvironmentBlock ?? IntPtr.Zero,
                    request.WorkingDirectory,
                    ref startup,
                    out var processInformation))
            {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }

            using var process = new SafeKernelObjectHandle(processInformation.Process, ownsHandle: true);
            using var thread = new SafeKernelObjectHandle(processInformation.Thread, ownsHandle: true);
            using var job = NativeConPty.CreateKillOnCloseJob();
            var assignedToJob = false;
            try
            {
                NativeConPty.ThrowIfFalse(NativeConPty.AssignProcessToJobObject(job, process));
                assignedToJob = true;

                if (sandboxPolicy.NetworkAccess is ConnectorSandboxNetworkAccess.Controlled)
                {
                    networkLease = await networkGuard!.AcquireAsync(
                        request.NetworkPolicy!,
                        sandbox!.AppContainerSid,
                        checked((int)processInformation.ProcessId),
                        _ =>
                        {
                            NativeConPty.TerminateJob(job, 1);
                            return Task.CompletedTask;
                        },
                        cancellationToken).ConfigureAwait(false);
                }

                standardOutputWrite.Dispose();
                standardOutputWrite = null;
                standardErrorWrite.Dispose();
                standardErrorWrite = null;
                standardInputRead.Dispose();
                standardInputRead = null;
                // Closing the parent writer makes stdin immediately observe EOF.
                standardInputWrite.Dispose();
                standardInputWrite = null;

                var stdoutCapture = new BoundedOutputCapture(MaximumPreviewBytes);
                var stderrCapture = new BoundedOutputCapture(MaximumPreviewBytes);
                await using var stdoutStream = new FileStream(
                    standardOutputRead,
                    FileAccess.Read,
                    bufferSize: 16 * 1_024,
                    isAsync: true);
                standardOutputRead = null;
                await using var stderrStream = new FileStream(
                    standardErrorRead,
                    FileAccess.Read,
                    bufferSize: 16 * 1_024,
                    isAsync: true);
                standardErrorRead = null;

                if (NativeConPty.ResumeThread(thread) == uint.MaxValue)
                {
                    throw new Win32Exception(Marshal.GetLastWin32Error());
                }

                var stdoutTask = stdoutCapture.DrainAsync(stdoutStream);
                var stderrTask = stderrCapture.DrainAsync(stderrStream);
                var exitTask = Task.Run(() => NativeConPty.WaitForExit(process), CancellationToken.None);
                var timeoutTask = Task.Delay(request.TimeoutMilliseconds, CancellationToken.None);
                var cancellationTask = Task.Delay(Timeout.InfiniteTimeSpan, cancellationToken);
                var completed = await Task.WhenAny(exitTask, timeoutTask, cancellationTask)
                    .ConfigureAwait(false);

                var timedOut = completed == timeoutTask;
                if (completed == cancellationTask)
                {
                    NativeConPty.TerminateJob(job, 1);
                    await exitTask.ConfigureAwait(false);
                    if (networkLease is not null)
                    {
                        await networkLease.DisposeAsync().ConfigureAwait(false);
                        networkLease = null;
                    }
                    job.Dispose();
                    await Task.WhenAll(stdoutTask, stderrTask).ConfigureAwait(false);
                    cancellationToken.ThrowIfCancellationRequested();
                }

                if (timedOut)
                {
                    NativeConPty.TerminateJob(job, 1460);
                }

                var exitCode = await exitTask.ConfigureAwait(false);
                if (networkLease is not null)
                {
                    await networkLease.DisposeAsync().ConfigureAwait(false);
                    networkLease = null;
                }
                // Closing a kill-on-close job also removes children left behind by the root process,
                // allowing inherited stdout/stderr handles to reach EOF deterministically.
                job.Dispose();
                await Task.WhenAll(stdoutTask, stderrTask).ConfigureAwait(false);
                return new TerminalCommandResult(
                    request.Command,
                    request.Arguments,
                    request.WorkingDirectory,
                    request.WorkspaceId,
                    !timedOut && exitCode == 0,
                    exitCode,
                    timedOut,
                    request.TimeoutMilliseconds,
                    stdoutCapture.Text,
                    stderrCapture.Text,
                    stdoutCapture.TotalBytes,
                    stderrCapture.TotalBytes,
                    stdoutCapture.Truncated,
                    stderrCapture.Truncated,
                    timedOut ? $"Command timed out after {request.TimeoutMilliseconds} ms." : null,
                    sandboxPolicy.PermissionProfile.ToString(),
                    sandboxPolicy.NetworkAccess.ToString());
            }
            catch
            {
                if (assignedToJob)
                {
                    NativeConPty.TerminateJob(job, 1);
                }
                else
                {
                    NativeTerminalProcess.TerminateProcess(process, 1);
                }
                if (networkLease is not null)
                {
                    await networkLease.DisposeAsync().ConfigureAwait(false);
                    networkLease = null;
                }
                throw;
            }
        }
        catch (OperationCanceledException)
        {
            throw;
        }
        catch (Exception exception)
        {
            return new TerminalCommandResult(
                request.Command,
                request.Arguments,
                request.WorkingDirectory,
                request.WorkspaceId,
                Success: false,
                ExitCode: null,
                TimedOut: false,
                request.TimeoutMilliseconds,
                StandardOutput: string.Empty,
                StandardError: string.Empty,
                StandardOutputBytes: 0,
                StandardErrorBytes: 0,
                StandardOutputTruncated: false,
                StandardErrorTruncated: false,
                Error: exception.Message,
                SandboxProfile: sandboxPolicy.PermissionProfile.ToString(),
                SandboxNetwork: sandboxPolicy.NetworkAccess.ToString());
        }
        finally
        {
            if (networkLease is not null)
            {
                await networkLease.DisposeAsync().ConfigureAwait(false);
            }
            standardOutputRead?.Dispose();
            standardOutputWrite?.Dispose();
            standardErrorRead?.Dispose();
            standardErrorWrite?.Dispose();
            standardInputRead?.Dispose();
            standardInputWrite?.Dispose();
            if (attributeList != IntPtr.Zero)
            {
                NativeConPty.DeleteProcThreadAttributeList(attributeList);
                Marshal.FreeHGlobal(attributeList);
            }

            if (inheritedHandles != IntPtr.Zero)
            {
                Marshal.FreeHGlobal(inheritedHandles);
            }
            if (sandbox is not null)
            {
                await sandbox.DisposeAsync().ConfigureAwait(false);
            }
        }
    }

    internal static string BuildCommandLine(string command, IReadOnlyList<string> arguments) =>
        string.Join(' ', new[] { command }.Concat(arguments).Select(QuoteWindowsArgument));

    private static LaunchCommand ResolveLaunch(TerminalCommandRequest request)
    {
        var executable = ResolveExecutable(request);
        var extension = Path.GetExtension(executable);
        if (extension.Equals(".cmd", StringComparison.OrdinalIgnoreCase) ||
            extension.Equals(".bat", StringComparison.OrdinalIgnoreCase))
        {
            var commandInterpreter = Environment.GetEnvironmentVariable("ComSpec");
            if (string.IsNullOrWhiteSpace(commandInterpreter) || !File.Exists(commandInterpreter))
            {
                commandInterpreter = Path.Combine(Environment.SystemDirectory, "cmd.exe");
            }

            return new LaunchCommand(
                Path.GetFullPath(commandInterpreter),
                ["/d", "/s", "/c", BuildCommandLine(executable, request.Arguments)]);
        }

        return new LaunchCommand(executable, request.Arguments);
    }

    private static string ResolveExecutable(TerminalCommandRequest request)
    {
        var command = request.Command.Trim();
        if (Path.IsPathRooted(command))
        {
            var absolute = Path.GetFullPath(command);
            return File.Exists(absolute)
                ? absolute
                : throw new FileNotFoundException("Terminal executable was not found.", absolute);
        }

        if (command.Contains(Path.DirectorySeparatorChar) ||
            command.Contains(Path.AltDirectorySeparatorChar))
        {
            var candidate = Path.GetFullPath(command, request.WorkingDirectory);
            var relative = Path.GetRelativePath(request.WorkspaceRoot, candidate);
            var guarded = new WorkspacePathGuard(request.WorkspaceRoot).ResolveExisting(relative);
            return File.Exists(guarded)
                ? guarded
                : throw new FileNotFoundException("Terminal executable was not found.", guarded);
        }

        var extensions = Path.HasExtension(command)
            ? new[] { string.Empty }
            : (Environment.GetEnvironmentVariable("PATHEXT") ?? ".COM;.EXE;.BAT;.CMD")
                .Split(';', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        foreach (var rawDirectory in (Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
                     .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            var directory = rawDirectory.Trim('"');
            if (directory.Length == 0 || !Path.IsPathRooted(directory))
            {
                continue;
            }

            foreach (var extension in extensions)
            {
                var candidate = Path.Combine(directory, command + extension);
                if (File.Exists(candidate))
                {
                    return Path.GetFullPath(candidate);
                }
            }
        }

        throw new FileNotFoundException(
            $"Terminal executable '{command}' was not found in the trusted PATH search.");
    }

    private static string QuoteWindowsArgument(string value)
    {
        if (value.Length > 0 && !value.Any(character => char.IsWhiteSpace(character) || character == '"'))
        {
            return value;
        }

        var result = new StringBuilder(value.Length + 2).Append('"');
        var backslashes = 0;
        foreach (var character in value)
        {
            if (character == '\\')
            {
                backslashes++;
                continue;
            }

            if (character == '"')
            {
                result.Append('\\', backslashes * 2 + 1).Append('"');
                backslashes = 0;
                continue;
            }

            result.Append('\\', backslashes).Append(character);
            backslashes = 0;
        }

        result.Append('\\', backslashes * 2).Append('"');
        return result.ToString();
    }

    private sealed class BoundedOutputCapture(int maximumBytes)
    {
        private readonly MemoryStream _preview = new(maximumBytes);

        public long TotalBytes { get; private set; }

        public bool Truncated => TotalBytes > _preview.Length;

        public string Text => Encoding.UTF8.GetString(_preview.GetBuffer(), 0, checked((int)_preview.Length));

        public async Task DrainAsync(Stream stream)
        {
            var buffer = new byte[64 * 1_024];
            while (true)
            {
                var count = await stream.ReadAsync(buffer).ConfigureAwait(false);
                if (count == 0)
                {
                    return;
                }

                TotalBytes += count;
                var remaining = maximumBytes - checked((int)_preview.Length);
                if (remaining > 0)
                {
                    _preview.Write(buffer, 0, Math.Min(remaining, count));
                }
            }
        }
    }

    private sealed record LaunchCommand(
        string Executable,
        IReadOnlyList<string> Arguments);
}

internal static class NativeTerminalProcess
{
    internal const uint StartfUseStdHandles = 0x0000_0100;
    internal const uint CreateNoWindow = 0x0800_0000;
    private const uint HandleFlagInherit = 0x0000_0001;

    internal static void CreatePipe(
        out SafeFileHandle parentEnd,
        out SafeFileHandle childEnd,
        bool parentReads)
    {
        var security = new SecurityAttributes
        {
            Length = Marshal.SizeOf<SecurityAttributes>(),
            InheritHandle = true,
        };
        NativeConPty.ThrowIfFalse(CreatePipeNative(out var read, out var write, ref security, 0));
        parentEnd = parentReads ? read : write;
        childEnd = parentReads ? write : read;
        NativeConPty.ThrowIfFalse(SetHandleInformation(parentEnd, HandleFlagInherit, 0));
    }

    [DllImport("kernel32.dll", EntryPoint = "CreatePipe", SetLastError = true)]
    private static extern bool CreatePipeNative(
        out SafeFileHandle readPipe,
        out SafeFileHandle writePipe,
        ref SecurityAttributes pipeAttributes,
        uint size);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetHandleInformation(
        SafeFileHandle handle,
        uint mask,
        uint flags);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool TerminateProcessNative(
        SafeKernelObjectHandle process,
        uint exitCode);

    internal static void TerminateProcess(SafeKernelObjectHandle process, uint exitCode)
    {
        if (!process.IsClosed && !process.IsInvalid)
        {
            _ = TerminateProcessNative(process, exitCode);
        }
    }

}
