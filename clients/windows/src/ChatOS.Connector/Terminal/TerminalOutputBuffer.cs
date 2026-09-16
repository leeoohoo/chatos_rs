using System.Text;

namespace ChatOS.Connector.Terminal;

public sealed class TerminalOutputBuffer
{
    private sealed record Chunk(long Sequence, byte[] Data);

    private readonly object _gate = new();
    private readonly int _maximumBytes;
    private readonly List<Chunk> _chunks = [];
    private int _byteCount;
    private long _lastSequence;
    private bool _discardedOutput;

    public TerminalOutputBuffer(int maximumCharacters = 4 * 1024 * 1024)
    {
        if (maximumCharacters <= 0)
        {
            throw new ArgumentOutOfRangeException(nameof(maximumCharacters));
        }

        // Keep the historical parameter name for source compatibility. The
        // journal is byte bounded because Relay limits are measured in bytes.
        _maximumBytes = maximumCharacters;
    }

    public long Append(string value)
    {
        if (string.IsNullOrEmpty(value))
        {
            lock (_gate)
            {
                return _lastSequence;
            }
        }

        var data = Encoding.UTF8.GetBytes(value);
        lock (_gate)
        {
            var sequence = checked(++_lastSequence);
            _chunks.Add(new Chunk(sequence, data));
            _byteCount += data.Length;
            while (_byteCount > _maximumBytes && _chunks.Count > 1)
            {
                _byteCount -= _chunks[0].Data.Length;
                _chunks.RemoveAt(0);
                _discardedOutput = true;
            }

            if (_byteCount > _maximumBytes)
            {
                var retained = Utf8Suffix(_chunks[0].Data, _maximumBytes);
                _chunks[0] = new Chunk(sequence, retained);
                _byteCount = retained.Length;
                _discardedOutput = true;
            }

            return sequence;
        }
    }

    public string Snapshot(int maximumLines = 500)
        => SnapshotState(maximumLines, int.MaxValue).Data;

    public TerminalSnapshot SnapshotState(
        int maximumLines = 500,
        int maximumTransportBytes = 16 * 1024)
    {
        maximumLines = Math.Clamp(maximumLines, 1, 5_000);
        if (maximumTransportBytes <= 0)
        {
            throw new ArgumentOutOfRangeException(nameof(maximumTransportBytes));
        }

        lock (_gate)
        {
            var all = new byte[_byteCount];
            var offset = 0;
            foreach (var chunk in _chunks)
            {
                chunk.Data.CopyTo(all, offset);
                offset += chunk.Data.Length;
            }

            var transportTruncated = all.Length > maximumTransportBytes;
            var visible = transportTruncated ? Utf8Suffix(all, maximumTransportBytes) : all;
            var text = Encoding.UTF8.GetString(visible);
            var start = text.Length;
            var lines = 0;
            while (start > 0)
            {
                start--;
                if (text[start] == '\n' && ++lines >= maximumLines)
                {
                    start++;
                    break;
                }
            }

            var omittedLines = start > 0;
            return new TerminalSnapshot(
                text[start..],
                _chunks.Count == 0 ? _lastSequence : _chunks[0].Sequence,
                _lastSequence,
                _discardedOutput || transportTruncated || omittedLines);
        }
    }

    private static byte[] Utf8Suffix(byte[] value, int maximumBytes)
    {
        if (value.Length <= maximumBytes)
        {
            return value;
        }

        var start = value.Length - maximumBytes;
        while (start < value.Length && (value[start] & 0b1100_0000) == 0b1000_0000)
        {
            start++;
        }
        return value[start..];
    }
}
