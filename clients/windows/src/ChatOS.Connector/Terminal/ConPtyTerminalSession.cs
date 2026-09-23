using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
using Microsoft.Win32.SafeHandles;
using ChatOS.Connector.NetworkGuard;
using ChatOS.Connector.Sandbox;

namespace ChatOS.Connector.Terminal;

public sealed class ConPtyTerminalSessionFactory : ITerminalSessionFactory
{
    private readonly ConnectorOutboundEventHub _events;
    private readonly SandboxExecutionPolicyProvider _sandboxPolicy;
    private readonly NetworkGuardLeaseCoordinator _networkGuard;

    public ConPtyTerminalSessionFactory(
        ConnectorOutboundEventHub events,
        SandboxExecutionPolicyProvider sandboxPolicy,
        NetworkGuardLeaseCoordinator networkGuard)
    {
        _events = events;
        _sandboxPolicy = sandboxPolicy;
        _networkGuard = networkGuard;
    }

    public async Task<ITerminalSession> CreateAsync(
        TerminalSessionIdentity identity,
        TerminalSize size,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (!OperatingSystem.IsWindowsVersionAtLeast(10, 0, 17763))
        {
            throw new PlatformNotSupportedException("ConPTY requires Windows 10 version 1809 or newer.");
        }

        var policy = await _sandboxPolicy.ResolveAsync(cancellationToken).ConfigureAwait(false);
        var controlled = policy.NetworkAccess is ConnectorSandboxNetworkAccess.Controlled;
        if (controlled && identity.NetworkPolicy is null)
        {
            throw new InvalidOperationException(
                "Controlled terminal sessions require a signed network policy.");
        }
        if (identity.NetworkPolicy is not null &&
            !string.Equals(identity.NetworkPolicy.WorkspaceId, identity.WorkspaceId, StringComparison.Ordinal))
        {
            throw new InvalidOperationException(
                "Controlled network policy does not belong to this terminal workspace.");
        }
        using var sandbox = policy.UseAppContainer
            ? await WindowsAppContainerSandbox.PrepareAsync(
                identity.WorkspaceRoot,
                policy,
                controlled
                    ? $"{identity.NetworkPolicy!.PolicyRevision}:{identity.SessionId}:{Guid.NewGuid():N}"
                    : null,
                cancellationToken).ConfigureAwait(false)
            : null;
        var shell = WindowsShellResolver.Resolve();
        var session = await ConPtyTerminalSession.StartAsync(
            identity,
            size,
            shell.Executable,
            shell.Arguments,
            sandbox,
            controlled ? _networkGuard : null,
            cancellationToken).ConfigureAwait(false);
        session.EventReceived += (_, value) => _events.Publish(value);
        return session;
    }
}

internal sealed class ConPtyTerminalSession : ITerminalSession
{
    private readonly TerminalOutputBuffer _outputBuffer = new();
    private readonly SemaphoreSlim _writeGate = new(1, 1);
    private readonly CancellationTokenSource _lifetime = new();
    private readonly SafeFileHandle _input;
    private readonly FileStream _output;
    private readonly SafePseudoConsoleHandle _pseudoConsole;
    private readonly SafeKernelObjectHandle _job;
    private readonly SafeKernelObjectHandle _process;
    private IAsyncDisposable? _sandboxProfileLease;
    private readonly Task _outputTask;
    private readonly Task _waitTask;
    private NetworkGuardLeaseLifetime? _networkLease;
    private int _exitCode = int.MinValue;
    private string? _outputFailure;
    private int _exited;
    private int _disposed;

    private ConPtyTerminalSession(
        TerminalSessionIdentity identity,
        NativeConPtyProcess native)
    {
        Identity = identity;
        _input = native.Input;
        _output = native.Output;
        _pseudoConsole = native.PseudoConsole;
        _job = native.Job;
        _process = native.Process;
        _networkLease = native.NetworkLease;
        _sandboxProfileLease = native.SandboxProfileLease;
        _outputTask = Task.Factory.StartNew(
            () => ReadOutput(_lifetime.Token),
            CancellationToken.None,
            TaskCreationOptions.LongRunning,
            TaskScheduler.Default);
        _waitTask = WaitForExitAsync();
    }

    public TerminalSessionIdentity Identity { get; }

    public bool HasExited => Volatile.Read(ref _exited) != 0;

    internal int? ExitCode => Volatile.Read(ref _exitCode) is var value && value != int.MinValue
        ? value
        : null;

    internal string? OutputFailure => Volatile.Read(ref _outputFailure);

    public bool IsBusy => false;

    public event EventHandler<TerminalEvent>? EventReceived;

    public static ConPtyTerminalSession Start(
        TerminalSessionIdentity identity,
        TerminalSize size,
        string executable,
        IReadOnlyList<string> arguments,
        WindowsAppContainerLaunchContext? sandbox) =>
        new(identity, NativeConPtyProcess.Start(
            executable,
            arguments,
            identity.WorkingDirectory,
            size,
            sandbox));

    public static async Task<ConPtyTerminalSession> StartAsync(
        TerminalSessionIdentity identity,
        TerminalSize size,
        string executable,
        IReadOnlyList<string> arguments,
        WindowsAppContainerLaunchContext? sandbox,
        NetworkGuardLeaseCoordinator? networkGuard,
        CancellationToken cancellationToken)
    {
        var native = await NativeConPtyProcess.StartAsync(
            executable,
            arguments,
            identity.WorkingDirectory,
            size,
            sandbox,
            networkGuard is null
                ? null
                : async (processId, job) => await networkGuard.AcquireAsync(
                    identity.NetworkPolicy!,
                    sandbox!.AppContainerSid,
                    checked((int)processId),
                    _ =>
                    {
                        NativeConPty.TerminateJob(job, 1);
                        return Task.CompletedTask;
                    },
                    cancellationToken).ConfigureAwait(false),
            cancellationToken).ConfigureAwait(false);
        return new ConPtyTerminalSession(identity, native);
    }

    public async Task WriteAsync(string data, CancellationToken cancellationToken = default)
    {
        ObjectDisposedException.ThrowIf(Volatile.Read(ref _disposed) != 0, this);
        TerminalTransportChunker.ValidateInput(data);
        if (HasExited)
        {
            throw new InvalidOperationException("Terminal session has exited.");
        }

        var bytes = Encoding.UTF8.GetBytes(data);
        await _writeGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            cancellationToken.ThrowIfCancellationRequested();
            NativeConPty.Write(_input, bytes);
        }
        finally
        {
            _writeGate.Release();
        }
    }

    public Task ResizeAsync(TerminalSize size, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        ObjectDisposedException.ThrowIf(Volatile.Read(ref _disposed) != 0, this);
        if (HasExited)
        {
            return Task.CompletedTask;
        }

        NativeConPty.Resize(_pseudoConsole, size);
        return Task.CompletedTask;
    }

    public string Snapshot(int maximumLines = 500) => _outputBuffer.Snapshot(maximumLines);

    public TerminalSnapshot SnapshotState(int maximumLines = 500) =>
        _outputBuffer.SnapshotState(maximumLines);

    public async Task StopAsync(CancellationToken cancellationToken = default)
    {
        if (HasExited)
        {
            return;
        }

        NativeConPty.TerminateJob(_job, 1);
        try
        {
            await _waitTask.WaitAsync(TimeSpan.FromSeconds(3), cancellationToken).ConfigureAwait(false);
        }
        catch (TimeoutException)
        {
        }
    }

    public async ValueTask DisposeAsync()
    {
        if (Interlocked.Exchange(ref _disposed, 1) != 0)
        {
            return;
        }

        try
        {
            await StopAsync(CancellationToken.None).ConfigureAwait(false);
        }
        catch
        {
        }

        _input.Dispose();
        // Closing ConPTY first releases its duplicated output writer, allowing the
        // synchronous reader thread to drain the final frame and observe EOF. Do not
        // cancel that reader until ClosePseudoConsole has finished or ConHost can block
        // while flushing its remaining output.
        var closePseudoConsoleTask = Task.Factory.StartNew(
            _pseudoConsole.Dispose,
            CancellationToken.None,
            TaskCreationOptions.LongRunning,
            TaskScheduler.Default);
        try
        {
            await closePseudoConsoleTask.WaitAsync(TimeSpan.FromSeconds(3)).ConfigureAwait(false);
        }
        catch (TimeoutException)
        {
            // A broken output reader forces ConHost to abandon any final frame and lets
            // ClosePseudoConsole return instead of hanging application shutdown.
        }
        _lifetime.Cancel();
        _output.Dispose();
        await ReleaseNetworkLeaseAsync().ConfigureAwait(false);
        _job.Dispose();
        _process.Dispose();
        await ReleaseSandboxProfileAsync().ConfigureAwait(false);
        try
        {
            await Task.WhenAll(_outputTask, _waitTask, closePseudoConsoleTask)
                .WaitAsync(TimeSpan.FromSeconds(1))
                .ConfigureAwait(false);
        }
        catch
        {
        }

        _lifetime.Dispose();
        _writeGate.Dispose();
    }

    private void ReadOutput(CancellationToken cancellationToken)
    {
        var bytes = new byte[16 * 1024];
        var decoder = Encoding.UTF8.GetDecoder();
        var characters = new char[Encoding.UTF8.GetMaxCharCount(bytes.Length)];
        try
        {
            while (!cancellationToken.IsCancellationRequested)
            {
                var read = _output.Read(bytes, 0, bytes.Length);
                if (read == 0)
                {
                    break;
                }

                var count = decoder.GetChars(bytes, 0, read, characters, 0, flush: false);
                var text = new string(characters, 0, count);
                foreach (var chunk in TerminalTransportChunker.SplitOutput(text))
                {
                    var sequence = _outputBuffer.Append(chunk);
                    Publish(new TerminalEvent(
                        TerminalEventKind.Output,
                        Identity.SessionId,
                        Data: chunk,
                        Sequence: sequence));
                }
            }

            var remaining = decoder.GetChars(
                Array.Empty<byte>(),
                0,
                0,
                characters,
                0,
                flush: true);
            foreach (var chunk in TerminalTransportChunker.SplitOutput(
                new string(characters, 0, remaining)))
            {
                var sequence = _outputBuffer.Append(chunk);
                Publish(new TerminalEvent(
                    TerminalEventKind.Output,
                    Identity.SessionId,
                    Data: chunk,
                    Sequence: sequence));
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (ObjectDisposedException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            Volatile.Write(ref _outputFailure, exception.Message);
            Publish(new TerminalEvent(
                TerminalEventKind.Error,
                Identity.SessionId,
                Data: exception.Message));
        }
    }

    private async Task WaitForExitAsync()
    {
        var exitCode = await Task.Factory.StartNew(
            () => NativeConPty.WaitForExit(_process),
            CancellationToken.None,
            TaskCreationOptions.LongRunning,
            TaskScheduler.Default).ConfigureAwait(false);
        _input.Dispose();
        var closePseudoConsoleTask = Task.Factory.StartNew(
            _pseudoConsole.Dispose,
            CancellationToken.None,
            TaskCreationOptions.LongRunning,
            TaskScheduler.Default);
        try
        {
            await closePseudoConsoleTask.WaitAsync(TimeSpan.FromSeconds(3)).ConfigureAwait(false);
            await _outputTask.WaitAsync(TimeSpan.FromSeconds(1)).ConfigureAwait(false);
        }
        catch (TimeoutException)
        {
            // The process has exited, so no more useful output can be produced. A broken
            // pipe during the final ConHost frame must not delay exit notification.
        }
        Volatile.Write(ref _exitCode, exitCode);
        Interlocked.Exchange(ref _exited, 1);
        await ReleaseNetworkLeaseAsync().ConfigureAwait(false);
        await ReleaseSandboxProfileAsync().ConfigureAwait(false);
        Publish(new TerminalEvent(
            TerminalEventKind.Exit,
            Identity.SessionId,
            ExitCode: exitCode,
            Busy: false));
    }

    private async Task ReleaseNetworkLeaseAsync()
    {
        var lease = Interlocked.Exchange(ref _networkLease, null);
        if (lease is not null)
        {
            await lease.DisposeAsync().ConfigureAwait(false);
        }
    }

    private async Task ReleaseSandboxProfileAsync()
    {
        var lease = Interlocked.Exchange(ref _sandboxProfileLease, null);
        if (lease is not null)
        {
            await lease.DisposeAsync().ConfigureAwait(false);
        }
    }

    private void Publish(TerminalEvent value)
    {
        var handlers = EventReceived;
        if (handlers is null)
        {
            return;
        }

        foreach (EventHandler<TerminalEvent> handler in handlers.GetInvocationList())
        {
            try
            {
                handler(this, value);
            }
            catch
            {
                // A UI/event subscriber cannot terminate the native terminal reader.
            }
        }
    }
}

internal sealed record WindowsShell(string Executable, IReadOnlyList<string> Arguments);

internal static class WindowsShellResolver
{
    public static WindowsShell Resolve()
    {
        var configured = Environment.GetEnvironmentVariable("CHATOS_WINDOWS_SHELL");
        if (!string.IsNullOrWhiteSpace(configured))
        {
            return new WindowsShell(configured.Trim(), []);
        }

        var pwsh = FindOnPath("pwsh.exe");
        if (pwsh is not null)
        {
            return new WindowsShell(pwsh, ["-NoLogo", "-NoExit"]);
        }

        var windows = Environment.GetFolderPath(Environment.SpecialFolder.Windows);
        var powershell = Path.Combine(
            windows,
            "System32",
            "WindowsPowerShell",
            "v1.0",
            "powershell.exe");
        if (File.Exists(powershell))
        {
            return new WindowsShell(powershell, ["-NoLogo", "-NoExit"]);
        }

        return new WindowsShell(
            Environment.GetEnvironmentVariable("COMSPEC") ?? "cmd.exe",
            []);
    }

    private static string? FindOnPath(string executable)
    {
        foreach (var directory in (Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
            .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries))
        {
            try
            {
                var candidate = Path.Combine(directory.Trim(), executable);
                if (File.Exists(candidate))
                {
                    return candidate;
                }
            }
            catch (ArgumentException)
            {
            }
        }

        return null;
    }
}

internal sealed record NativeConPtyProcess(
    SafeFileHandle Input,
    FileStream Output,
    SafePseudoConsoleHandle PseudoConsole,
    SafeKernelObjectHandle Job,
    SafeKernelObjectHandle Process,
    NetworkGuardLeaseLifetime? NetworkLease,
    IAsyncDisposable? SandboxProfileLease)
{
    public static NativeConPtyProcess Start(
        string executable,
        IReadOnlyList<string> arguments,
        string workingDirectory,
        TerminalSize size,
        WindowsAppContainerLaunchContext? sandbox) =>
        StartAsync(
            executable,
            arguments,
            workingDirectory,
            size,
            sandbox,
            beforeResume: null,
            CancellationToken.None).GetAwaiter().GetResult();

    public static async Task<NativeConPtyProcess> StartAsync(
        string executable,
        IReadOnlyList<string> arguments,
        string workingDirectory,
        TerminalSize size,
        WindowsAppContainerLaunchContext? sandbox,
        Func<uint, SafeKernelObjectHandle, Task<NetworkGuardLeaseLifetime?>>? beforeResume,
        CancellationToken cancellationToken)
    {
        SafeFileHandle? pseudoInput = null;
        SafeFileHandle? inputWriter = null;
        SafeFileHandle? outputReader = null;
        SafeFileHandle? pseudoOutput = null;
        SafePseudoConsoleHandle? pseudoConsole = null;
        SafeKernelObjectHandle? job = null;
        SafeKernelObjectHandle? process = null;
        SafeKernelObjectHandle? thread = null;
        NetworkGuardLeaseLifetime? networkLease = null;
        IntPtr attributeList = IntPtr.Zero;
        try
        {
            NativeConPty.CreatePipePair(out inputWriter, out pseudoInput, parentReads: false);
            NativeConPty.CreatePipePair(out outputReader, out pseudoOutput, parentReads: true);
            pseudoConsole = NativeConPty.CreatePseudoConsole(size, pseudoInput, pseudoOutput);

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
            NativeConPty.ThrowIfFalse(NativeConPty.UpdateProcThreadAttribute(
                attributeList,
                0,
                NativeConPty.ProcThreadAttributePseudoConsole,
                pseudoConsole.DangerousGetHandle(),
                (nuint)IntPtr.Size,
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
                StartupInfo = new StartupInfo { Size = (uint)Marshal.SizeOf<StartupInfoEx>() },
                AttributeList = attributeList,
            };
            var commandLine = new StringBuilder(CommandLine(executable, arguments));
            var creationFlags = NativeConPty.ExtendedStartupInfoPresent |
                NativeConPty.CreateSuspended;
            if (sandbox is not null)
            {
                creationFlags |= NativeConPty.CreateUnicodeEnvironment;
            }
            NativeConPty.ThrowIfFalse(NativeConPty.CreateProcess(
                executable,
                commandLine,
                IntPtr.Zero,
                IntPtr.Zero,
                false,
                creationFlags,
                sandbox?.EnvironmentBlock ?? IntPtr.Zero,
                workingDirectory,
                ref startup,
                out var processInformation));
            process = new SafeKernelObjectHandle(processInformation.Process, ownsHandle: true);
            thread = new SafeKernelObjectHandle(processInformation.Thread, ownsHandle: true);
            job = NativeConPty.CreateKillOnCloseJob();
            NativeConPty.ThrowIfFalse(NativeConPty.AssignProcessToJobObject(
                job,
                process));
            cancellationToken.ThrowIfCancellationRequested();
            if (beforeResume is not null)
            {
                networkLease = await beforeResume(processInformation.ProcessId, job)
                    .ConfigureAwait(false);
            }
            if (NativeConPty.ResumeThread(thread) == uint.MaxValue)
            {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }

            pseudoInput.Dispose();
            pseudoInput = null;
            pseudoOutput.Dispose();
            pseudoOutput = null;
            thread.Dispose();
            thread = null;
            var input = inputWriter;
            inputWriter = null;
            var output = new FileStream(outputReader, FileAccess.Read, 16 * 1024, isAsync: false);
            outputReader = null;
            return new NativeConPtyProcess(
                input,
                output,
                pseudoConsole,
                job,
                process,
                networkLease,
                sandbox?.DetachProfileLease());
        }
        catch
        {
            if (networkLease is not null)
            {
                await networkLease.DisposeAsync().ConfigureAwait(false);
            }
            if (job is not null && process is not null)
            {
                // beforeResume failures happen while the initial process is still
                // suspended and therefore cannot have spawned descendants. Terminating
                // that process directly avoids TerminateJobObject blocking on ConPTY.
                NativeTerminalProcess.TerminateProcess(process, 1);
                var processToWait = process;
                var waitForExitTask = Task.Factory.StartNew(
                    () => NativeConPty.WaitForExit(processToWait),
                    CancellationToken.None,
                    TaskCreationOptions.LongRunning,
                    TaskScheduler.Default);
                try
                {
                    await waitForExitTask.WaitAsync(TimeSpan.FromSeconds(3)).ConfigureAwait(false);
                }
                catch (TimeoutException)
                {
                    // Handle disposal below remains the final kill-on-close backstop.
                }
            }
            thread?.Dispose();
            process?.Dispose();
            job?.Dispose();
            pseudoInput?.Dispose();
            inputWriter?.Dispose();
            outputReader?.Dispose();
            pseudoOutput?.Dispose();
            if (pseudoConsole is not null)
            {
                var closeTask = Task.Factory.StartNew(
                    pseudoConsole.Dispose,
                    CancellationToken.None,
                    TaskCreationOptions.LongRunning,
                    TaskScheduler.Default);
                try
                {
                    await closeTask.WaitAsync(TimeSpan.FromSeconds(3)).ConfigureAwait(false);
                }
                catch (TimeoutException)
                {
                    // The process and every pipe are already closed. Do not let a stuck
                    // ConHost cleanup mask the original launch failure indefinitely.
                }
            }
            throw;
        }
        finally
        {
            if (attributeList != IntPtr.Zero)
            {
                NativeConPty.DeleteProcThreadAttributeList(attributeList);
                Marshal.FreeHGlobal(attributeList);
            }
        }
    }

    private static string CommandLine(string executable, IReadOnlyList<string> arguments) =>
        string.Join(' ', new[] { Quote(executable) }.Concat(arguments.Select(Quote)));

    private static string Quote(string value)
    {
        if (value.Length > 0 && !value.Any(character => char.IsWhiteSpace(character) || character == '"'))
        {
            return value;
        }

        var result = new StringBuilder("\"");
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
}
