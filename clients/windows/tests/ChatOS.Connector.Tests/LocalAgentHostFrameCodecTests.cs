using System.Buffers.Binary;
using ChatOS.Connector.LocalAgent;

namespace ChatOS.Connector.Tests;

public sealed class LocalAgentHostFrameCodecTests
{
    [Fact]
    public async Task WritesAndReadsBigEndianLengthPrefixedFrames()
    {
        var payload = "{\"command\":{\"type\":\"health\"}}"u8.ToArray();
        await using var stream = new MemoryStream();

        await LocalAgentHostFrameCodec.WriteAsync(stream, payload, CancellationToken.None);
        var encoded = stream.ToArray();
        Assert.Equal(payload.Length, BinaryPrimitives.ReadUInt32BigEndian(encoded.AsSpan(0, 4)));
        Assert.Equal(payload, encoded[4..]);

        stream.Position = 0;
        Assert.Equal(payload, await LocalAgentHostFrameCodec.ReadAsync(stream, CancellationToken.None));
    }

    [Fact]
    public async Task RejectsEmptyOversizedAndTruncatedFrames()
    {
        await using var sink = new MemoryStream();
        await Assert.ThrowsAsync<InvalidDataException>(() => LocalAgentHostFrameCodec.WriteAsync(
            sink,
            ReadOnlyMemory<byte>.Empty,
            CancellationToken.None));
        await Assert.ThrowsAsync<InvalidDataException>(() => LocalAgentHostFrameCodec.WriteAsync(
            sink,
            new byte[LocalAgentHostFrameCodec.MaximumFrameBytes + 1],
            CancellationToken.None));

        var truncated = new byte[6];
        BinaryPrimitives.WriteUInt32BigEndian(truncated.AsSpan(0, 4), 4);
        truncated[4] = 1;
        truncated[5] = 2;
        await using var source = new MemoryStream(truncated);
        await Assert.ThrowsAsync<EndOfStreamException>(() =>
            LocalAgentHostFrameCodec.ReadAsync(source, CancellationToken.None));
    }
}
