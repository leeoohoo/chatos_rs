using System.Buffers.Binary;

namespace ChatOS.Connector.LocalAgent;

internal static class LocalAgentHostFrameCodec
{
    internal const int MaximumFrameBytes = 1024 * 1024;

    public static async Task WriteAsync(
        Stream stream,
        ReadOnlyMemory<byte> payload,
        CancellationToken cancellationToken)
    {
        if (payload.IsEmpty || payload.Length > MaximumFrameBytes)
        {
            throw new InvalidDataException("Local Agent Host frame size is invalid.");
        }
        var header = new byte[4];
        BinaryPrimitives.WriteUInt32BigEndian(header, checked((uint)payload.Length));
        await stream.WriteAsync(header, cancellationToken).ConfigureAwait(false);
        await stream.WriteAsync(payload, cancellationToken).ConfigureAwait(false);
        await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
    }

    public static async Task<byte[]> ReadAsync(Stream stream, CancellationToken cancellationToken)
    {
        var header = new byte[4];
        await stream.ReadExactlyAsync(header, cancellationToken).ConfigureAwait(false);
        var length = BinaryPrimitives.ReadUInt32BigEndian(header);
        if (length is 0 or > MaximumFrameBytes)
        {
            throw new InvalidDataException("Local Agent Host returned an invalid frame size.");
        }
        var payload = new byte[checked((int)length)];
        await stream.ReadExactlyAsync(payload, cancellationToken).ConfigureAwait(false);
        return payload;
    }
}
