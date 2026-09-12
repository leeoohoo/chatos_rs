using System.Buffers.Binary;
using System.Diagnostics;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.Terminal;
using Microsoft.Win32.SafeHandles;

namespace ChatOS.Connector.LocalAgent;

public enum WindowsLocalAgentHostLaunchFailure
{
    InvalidConfiguration,
    UntrustedExecutable,
    ProcessLaunchFailed,
    LaunchFrameWriteFailed,
    ReadyTimeout,
    InvalidReadyFrame,
    ProtocolMismatch,
    LaunchMismatch,
    ProcessMismatch,
    EndpointMismatch,
}

public sealed class WindowsLocalAgentHostLaunchException(
    WindowsLocalAgentHostLaunchFailure failure,
    string message,
    Exception? innerException = null) : Exception(message, innerException)
{
    public WindowsLocalAgentHostLaunchFailure Failure { get; } = failure;
}

public sealed class WindowsLocalAgentHostLaunchMaterial : IDisposable
{
    private byte[]? _requestJson;

    public WindowsLocalAgentHostLaunchMaterial(ReadOnlySpan<byte> requestJson)
    {
        if (requestJson.IsEmpty || requestJson.Length > LocalAgentHostLaunchProtocol.MaximumFrameBytes)
        {
            throw LaunchError(
                WindowsLocalAgentHostLaunchFailure.InvalidConfiguration,
                "Local Agent Host launch request size is invalid.");
        }

        _requestJson = requestJson.ToArray();
    }

    internal ReadOnlyMemory<byte> RequestJson => _requestJson
        ?? throw new ObjectDisposedException(nameof(WindowsLocalAgentHostLaunchMaterial));

    public void Dispose()
    {
        var bytes = Interlocked.Exchange(ref _requestJson, null);
        if (bytes is not null)
        {
            CryptographicOperations.ZeroMemory(bytes);
        }
    }

    private static WindowsLocalAgentHostLaunchException LaunchError(
        WindowsLocalAgentHostLaunchFailure failure,
        string message) => new(failure, message);
}

public sealed record WindowsLocalAgentHostLaunchConfiguration
{
    public required string ExecutablePath { get; init; }

    public required string ExpectedExecutableSha256 { get; init; }

    public required string LaunchId { get; init; }

    public required string ExpectedClientEndpoint { get; init; }

    public required WindowsLocalAgentHostLaunchMaterial LaunchMaterial { get; init; }

    public TimeSpan ReadyTimeout { get; init; } = TimeSpan.FromSeconds(30);
}

public sealed record WindowsLocalAgentHostReady(
    [property: JsonPropertyName("protocol_version")] uint ProtocolVersion,
    [property: JsonPropertyName("launch_id")] string LaunchId,
    [property: JsonPropertyName("process_id")] uint ProcessId,
    [property: JsonPropertyName("client_endpoint")] string ClientEndpoint);

internal static class LocalAgentHostLaunchProtocol
{
    public const uint Version = 3;
    public const int MaximumFrameBytes = 1024 * 1024;
}

internal interface IWindowsLocalAgentHostProcess : IAsyncDisposable
{
    WindowsLocalAgentHostReady Ready { get; }

    Task<int> WaitForExitAsync(CancellationToken cancellationToken = default);

    Task TerminateAsync();
}

internal interface IWindowsLocalAgentHostProcessLauncher
{
    Task<IWindowsLocalAgentHostProcess> LaunchAsync(
        WindowsLocalAgentHostLaunchConfiguration configuration,
        CancellationToken cancellationToken = default);
}

internal sealed class WindowsLocalAgentHostProcessLauncher : IWindowsLocalAgentHostProcessLauncher
{
    public async Task<IWindowsLocalAgentHostProcess> LaunchAsync(
        WindowsLocalAgentHostLaunchConfiguration configuration,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(configuration);
        ArgumentNullException.ThrowIfNull(configuration.LaunchMaterial);
        try
        {
            ValidateConfiguration(configuration);
            await VerifyExecutableAsync(configuration, cancellationToken).ConfigureAwait(false);
            var start = new ProcessStartInfo
            {
                FileName = configuration.ExecutablePath,
                WorkingDirectory = Path.GetDirectoryName(configuration.ExecutablePath)!,
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardInput = true,
                RedirectStandardOutput = true,
                RedirectStandardError = false,
            };
            start.Environment.Clear();
            var process = new Process { StartInfo = start, EnableRaisingEvents = true };
            SafeKernelObjectHandle? job = null;
            try
            {
                if (!process.Start())
                {
                    throw LaunchError(
                        WindowsLocalAgentHostLaunchFailure.ProcessLaunchFailed,
                        "Local Agent Host did not start.");
                }

                if (OperatingSystem.IsWindows())
                {
                    job = NativeConPty.CreateKillOnCloseJob();
                    NativeConPty.ThrowIfFalse(
                        NativeConPty.AssignProcessToJobObject(job, process.SafeHandle));
                }

                await WriteFrameAsync(
                    process.StandardInput.BaseStream,
                    configuration.LaunchMaterial.RequestJson,
                    cancellationToken).ConfigureAwait(false);
                process.StandardInput.Close();
                var ready = await ReadReadyAsync(
                    process.StandardOutput.BaseStream,
                    configuration.ReadyTimeout,
                    cancellationToken).ConfigureAwait(false);
                ValidateReady(ready, configuration, process.Id);
                return new SystemWindowsLocalAgentHostProcess(process, job, ready);
            }
            catch
            {
                await TerminateAsync(process, job).ConfigureAwait(false);
                throw;
            }
        }
        finally
        {
            configuration.LaunchMaterial.Dispose();
        }
    }

    private static void ValidateConfiguration(WindowsLocalAgentHostLaunchConfiguration configuration)
    {
        if (!Path.IsPathFullyQualified(configuration.ExecutablePath)
            || !string.Equals(Path.GetExtension(configuration.ExecutablePath), ".exe", StringComparison.OrdinalIgnoreCase)
            || string.IsNullOrWhiteSpace(configuration.LaunchId)
            || configuration.LaunchId != configuration.LaunchId.Trim()
            || string.IsNullOrWhiteSpace(configuration.ExpectedClientEndpoint)
            || configuration.ExpectedClientEndpoint != configuration.ExpectedClientEndpoint.Trim()
            || configuration.ReadyTimeout <= TimeSpan.Zero
            || configuration.ReadyTimeout > TimeSpan.FromSeconds(120)
            || !TryDecodeDigest(configuration.ExpectedExecutableSha256, out _))
        {
            throw LaunchError(
                WindowsLocalAgentHostLaunchFailure.InvalidConfiguration,
                "Local Agent Host launch configuration is invalid.");
        }
    }

    private static async Task VerifyExecutableAsync(
        WindowsLocalAgentHostLaunchConfiguration configuration,
        CancellationToken cancellationToken)
    {
        var information = new FileInfo(configuration.ExecutablePath);
        if (!information.Exists || information.Attributes.HasFlag(FileAttributes.ReparsePoint))
        {
            throw LaunchError(
                WindowsLocalAgentHostLaunchFailure.UntrustedExecutable,
                "Local Agent Host executable is missing or is a reparse point.");
        }

        await using var stream = new FileStream(
            configuration.ExecutablePath,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read,
            128 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        var actual = await SHA256.HashDataAsync(stream, cancellationToken).ConfigureAwait(false);
        _ = TryDecodeDigest(configuration.ExpectedExecutableSha256, out var expected);
        if (!CryptographicOperations.FixedTimeEquals(actual, expected))
        {
            throw LaunchError(
                WindowsLocalAgentHostLaunchFailure.UntrustedExecutable,
                "Local Agent Host executable digest does not match the signed package manifest.");
        }
    }

    private static async Task WriteFrameAsync(
        Stream stream,
        ReadOnlyMemory<byte> body,
        CancellationToken cancellationToken)
    {
        var length = new byte[sizeof(uint)];
        BinaryPrimitives.WriteUInt32BigEndian(length, checked((uint)body.Length));
        try
        {
            await stream.WriteAsync(length, cancellationToken).ConfigureAwait(false);
            await stream.WriteAsync(body, cancellationToken).ConfigureAwait(false);
            await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (Exception error) when (error is IOException or ObjectDisposedException)
        {
            throw LaunchError(
                WindowsLocalAgentHostLaunchFailure.LaunchFrameWriteFailed,
                "Local Agent Host launch frame could not be written.",
                error);
        }
    }

    private static async Task<WindowsLocalAgentHostReady> ReadReadyAsync(
        Stream stream,
        TimeSpan timeout,
        CancellationToken cancellationToken)
    {
        using var timeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeoutSource.CancelAfter(timeout);
        try
        {
            var lengthBytes = new byte[sizeof(uint)];
            await stream.ReadExactlyAsync(lengthBytes, timeoutSource.Token).ConfigureAwait(false);
            var length = BinaryPrimitives.ReadUInt32BigEndian(lengthBytes);
            if (length == 0 || length > LocalAgentHostLaunchProtocol.MaximumFrameBytes)
            {
                throw InvalidReady();
            }

            var body = new byte[checked((int)length)];
            await stream.ReadExactlyAsync(body, timeoutSource.Token).ConfigureAwait(false);
            return JsonSerializer.Deserialize<WindowsLocalAgentHostReady>(body)
                ?? throw InvalidReady();
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw LaunchError(
                WindowsLocalAgentHostLaunchFailure.ReadyTimeout,
                "Local Agent Host ready handshake timed out.");
        }
        catch (Exception error) when (error is IOException or JsonException or OverflowException)
        {
            throw InvalidReady(error);
        }
    }

    private static void ValidateReady(
        WindowsLocalAgentHostReady ready,
        WindowsLocalAgentHostLaunchConfiguration configuration,
        int processId)
    {
        if (ready.ProtocolVersion != LocalAgentHostLaunchProtocol.Version)
        {
            throw LaunchError(
                WindowsLocalAgentHostLaunchFailure.ProtocolMismatch,
                "Local Agent Host launch protocol does not match the client.");
        }

        if (!string.Equals(ready.LaunchId, configuration.LaunchId, StringComparison.Ordinal))
        {
            throw LaunchError(
                WindowsLocalAgentHostLaunchFailure.LaunchMismatch,
                "Local Agent Host launch identifier does not match.");
        }

        if (ready.ProcessId != checked((uint)processId))
        {
            throw LaunchError(
                WindowsLocalAgentHostLaunchFailure.ProcessMismatch,
                "Local Agent Host process identifier does not match.");
        }

        if (!string.Equals(
                ready.ClientEndpoint,
                configuration.ExpectedClientEndpoint,
                StringComparison.Ordinal))
        {
            throw LaunchError(
                WindowsLocalAgentHostLaunchFailure.EndpointMismatch,
                "Local Agent Host IPC endpoint does not match.");
        }
    }

    private static bool TryDecodeDigest(string value, out byte[] digest)
    {
        digest = [];
        if (!value.StartsWith("sha256:", StringComparison.Ordinal)
            || value.Length != "sha256:".Length + (SHA256.HashSizeInBytes * 2))
        {
            return false;
        }

        try
        {
            digest = Convert.FromHexString(value["sha256:".Length..]);
            return digest.Length == SHA256.HashSizeInBytes;
        }
        catch (FormatException)
        {
            return false;
        }
    }

    private static WindowsLocalAgentHostLaunchException InvalidReady(Exception? inner = null) =>
        LaunchError(
            WindowsLocalAgentHostLaunchFailure.InvalidReadyFrame,
            "Local Agent Host returned an invalid ready frame.",
            inner);

    private static WindowsLocalAgentHostLaunchException LaunchError(
        WindowsLocalAgentHostLaunchFailure failure,
        string message,
        Exception? inner = null) => new(failure, message, inner);

    private static async Task TerminateAsync(Process process, SafeKernelObjectHandle? job)
    {
        try
        {
            if (job is not null)
            {
                NativeConPty.TerminateJob(job, 1);
            }
            else if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
            }

            await process.WaitForExitAsync().ConfigureAwait(false);
        }
        catch (InvalidOperationException)
        {
        }
        finally
        {
            job?.Dispose();
            process.Dispose();
        }
    }

    private sealed class SystemWindowsLocalAgentHostProcess(
        Process process,
        SafeKernelObjectHandle? job,
        WindowsLocalAgentHostReady ready) : IWindowsLocalAgentHostProcess
    {
        private int _terminated;

        public WindowsLocalAgentHostReady Ready { get; } = ready;

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
            else if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
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
