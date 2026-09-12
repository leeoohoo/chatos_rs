using System.Buffers.Binary;
using System.IO.Pipes;
using System.Security.Principal;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

internal interface ILocalAgentFrameTransport
{
    Task<byte[]> ExchangeAsync(byte[] request, CancellationToken cancellationToken = default);
}

internal static class LocalAgentFrameCodec
{
    internal static async Task WriteAsync(
        Stream stream,
        ReadOnlyMemory<byte> body,
        int maximumFrameBytes,
        CancellationToken cancellationToken = default)
    {
        if (body.IsEmpty || body.Length > maximumFrameBytes)
        {
            throw new InvalidDataException("Local Agent request frame size is invalid.");
        }
        var header = new byte[sizeof(uint)];
        BinaryPrimitives.WriteUInt32BigEndian(header, checked((uint)body.Length));
        await stream.WriteAsync(header, cancellationToken).ConfigureAwait(false);
        await stream.WriteAsync(body, cancellationToken).ConfigureAwait(false);
        await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
    }

    internal static async Task<byte[]> ReadAsync(
        Stream stream,
        int maximumFrameBytes,
        CancellationToken cancellationToken = default)
    {
        var header = new byte[sizeof(uint)];
        await ReadExactlyAsync(stream, header, cancellationToken).ConfigureAwait(false);
        var length = BinaryPrimitives.ReadUInt32BigEndian(header);
        if (length == 0 || length > maximumFrameBytes)
        {
            throw new InvalidDataException($"Local Agent response frame size is invalid ({length} bytes).");
        }
        var body = new byte[checked((int)length)];
        await ReadExactlyAsync(stream, body, cancellationToken).ConfigureAwait(false);
        return body;
    }

    private static async Task ReadExactlyAsync(
        Stream stream,
        Memory<byte> destination,
        CancellationToken cancellationToken)
    {
        var offset = 0;
        while (offset < destination.Length)
        {
            var read = await stream.ReadAsync(destination[offset..], cancellationToken)
                .ConfigureAwait(false);
            if (read == 0)
            {
                throw new EndOfStreamException(
                    "Local Agent Host closed the pipe before returning a complete frame.");
            }
            offset += read;
        }
    }
}

internal sealed class NamedPipeLocalAgentTransport : ILocalAgentFrameTransport
{
    private const string PipePrefix = "chatos-local-agent-";
    private const int MinimumOpaqueIdBytes = 8;
    private const int MaximumOpaqueIdBytes = 128;
    private static readonly TimeSpan MaximumTimeout = TimeSpan.FromMinutes(5);

    private readonly string _pipeName;
    private readonly TimeSpan _connectTimeout;
    private readonly TimeSpan _ioTimeout;
    private readonly int _maximumFrameBytes;
    private readonly ILocalAgentServerIdentityVerifier _identityVerifier;

    internal NamedPipeLocalAgentTransport(
        string pipeName,
        TimeSpan? connectTimeout = null,
        TimeSpan? ioTimeout = null,
        int maximumFrameBytes = LocalAgentProtocol.MaximumFrameBytes,
        ILocalAgentServerIdentityVerifier? identityVerifier = null)
    {
        ValidatePipeName(pipeName);
        if (connectTimeout is { } connection && (connection <= TimeSpan.Zero || connection > MaximumTimeout) ||
            ioTimeout is { } io && (io <= TimeSpan.Zero || io > MaximumTimeout) ||
            maximumFrameBytes <= 0)
        {
            throw new ArgumentOutOfRangeException(nameof(maximumFrameBytes), "Local Agent IPC limits must be positive.");
        }
        _pipeName = pipeName;
        _connectTimeout = connectTimeout ?? TimeSpan.FromSeconds(5);
        _ioTimeout = ioTimeout ?? TimeSpan.FromSeconds(35);
        _maximumFrameBytes = maximumFrameBytes;
        _identityVerifier = identityVerifier ?? new WindowsLocalAgentServerIdentityVerifier();
    }

    internal string PipeName => _pipeName;

    public async Task<byte[]> ExchangeAsync(
        byte[] request,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException("Local Agent named pipes require Windows.");
        }
        if (request.Length == 0 || request.Length > _maximumFrameBytes)
        {
            throw new InvalidDataException("Local Agent request frame size is invalid.");
        }

        await using var pipe = new NamedPipeClientStream(
            ".",
            _pipeName,
            PipeDirection.InOut,
            PipeOptions.Asynchronous | PipeOptions.WriteThrough,
            TokenImpersonationLevel.Identification);
        using var connectTimeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        connectTimeout.CancelAfter(_connectTimeout);
        try
        {
            await pipe.ConnectAsync(connectTimeout.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new TimeoutException("Local Agent Host connection timed out.");
        }

        _identityVerifier.Verify(pipe);
        using var ioTimeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        ioTimeout.CancelAfter(_ioTimeout);
        try
        {
            await LocalAgentFrameCodec.WriteAsync(
                pipe,
                request,
                _maximumFrameBytes,
                ioTimeout.Token).ConfigureAwait(false);
            return await LocalAgentFrameCodec.ReadAsync(
                pipe,
                _maximumFrameBytes,
                ioTimeout.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new TimeoutException("Local Agent Host I/O timed out.");
        }
    }

    internal static void ValidatePipeName(string pipeName)
    {
        ArgumentNullException.ThrowIfNull(pipeName);
        if (!pipeName.StartsWith(PipePrefix, StringComparison.Ordinal))
        {
            throw new ArgumentException("Local Agent pipe must use the private ChatOS namespace.", nameof(pipeName));
        }
        var opaqueId = pipeName.AsSpan(PipePrefix.Length);
        if (opaqueId.Length is < MinimumOpaqueIdBytes or > MaximumOpaqueIdBytes ||
            !IsAsciiOpaqueId(opaqueId))
        {
            throw new ArgumentException("Local Agent pipe identifier is invalid.", nameof(pipeName));
        }
    }

    private static bool IsAsciiOpaqueId(ReadOnlySpan<char> value)
    {
        foreach (var character in value)
        {
            if (!((character is >= 'a' and <= 'z') ||
                  (character is >= 'A' and <= 'Z') ||
                  (character is >= '0' and <= '9') ||
                  character is '-' or '_'))
            {
                return false;
            }
        }
        return true;
    }
}
