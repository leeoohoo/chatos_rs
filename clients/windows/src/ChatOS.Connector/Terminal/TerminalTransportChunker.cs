using System.Text;

namespace ChatOS.Connector.Terminal;

internal static class TerminalTransportChunker
{
    public const int MaximumOutputBytes = 16 * 1024;
    public const int MaximumInputBytes = 64 * 1024;

    public static IEnumerable<string> SplitOutput(string value)
    {
        if (string.IsNullOrEmpty(value))
        {
            yield break;
        }

        var bytes = Encoding.UTF8.GetBytes(value);
        var start = 0;
        while (start < bytes.Length)
        {
            var end = Math.Min(start + MaximumOutputBytes, bytes.Length);
            if (end < bytes.Length)
            {
                while (end > start && (bytes[end] & 0b1100_0000) == 0b1000_0000)
                {
                    end--;
                }
            }

            yield return Encoding.UTF8.GetString(bytes, start, end - start);
            start = end;
        }
    }

    public static void ValidateInput(string value)
    {
        if (Encoding.UTF8.GetByteCount(value) > MaximumInputBytes)
        {
            throw new ArgumentException("Terminal input frame exceeds the 64 KiB limit.", nameof(value));
        }
    }
}
