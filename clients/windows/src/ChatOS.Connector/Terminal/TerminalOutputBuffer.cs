using System.Text;

namespace ChatOS.Connector.Terminal;

public sealed class TerminalOutputBuffer
{
    private readonly object _gate = new();
    private readonly int _maximumCharacters;
    private readonly StringBuilder _buffer = new();

    public TerminalOutputBuffer(int maximumCharacters = 512 * 1024)
    {
        if (maximumCharacters <= 0)
        {
            throw new ArgumentOutOfRangeException(nameof(maximumCharacters));
        }

        _maximumCharacters = maximumCharacters;
    }

    public void Append(string value)
    {
        if (string.IsNullOrEmpty(value))
        {
            return;
        }

        lock (_gate)
        {
            if (value.Length >= _maximumCharacters)
            {
                _buffer.Clear();
                _buffer.Append(value.AsSpan(value.Length - _maximumCharacters));
                return;
            }

            _buffer.Append(value);
            var overflow = _buffer.Length - _maximumCharacters;
            if (overflow > 0)
            {
                _buffer.Remove(0, overflow);
            }
        }
    }

    public string Snapshot(int maximumLines = 500)
    {
        maximumLines = Math.Clamp(maximumLines, 1, 5_000);
        lock (_gate)
        {
            var start = _buffer.Length;
            var lines = 0;
            while (start > 0)
            {
                start--;
                if (_buffer[start] == '\n' && ++lines >= maximumLines)
                {
                    start++;
                    break;
                }
            }

            return _buffer.ToString(start, _buffer.Length - start);
        }
    }
}
