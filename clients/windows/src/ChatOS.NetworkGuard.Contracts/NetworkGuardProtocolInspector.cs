using System.Buffers.Binary;
using System.Text;

namespace ChatOS.NetworkGuard.Contracts;

public enum NetworkGuardInspectionState
{
    Allowed,
    Denied,
    Incomplete,
    Malformed,
    MissingHost,
    UnsupportedProtocol,
}

public sealed record NetworkGuardInspectionResult(
    NetworkGuardInspectionState State,
    string? Host = null,
    int? Port = null)
{
    public bool IsAllowed => State is NetworkGuardInspectionState.Allowed;
}

public static class NetworkGuardProtocolInspector
{
    public const int MaximumHandshakeBytes = 64 * 1024;

    public static NetworkGuardInspectionResult InspectHttp(
        ReadOnlySpan<byte> data,
        ControlledNetworkPolicy policy)
    {
        ArgumentNullException.ThrowIfNull(policy);
        if (data.Length > MaximumHandshakeBytes)
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
        }

        var end = data.IndexOf("\r\n\r\n"u8);
        if (end < 0)
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Incomplete);
        }

        var headerBytes = data[..end];
        foreach (var value in headerBytes)
        {
            if (value > 0x7e || value < 0x20 && value is not (byte)'\r' and not (byte)'\n' and not (byte)'\t')
            {
                return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
            }
        }
        var headers = Encoding.ASCII.GetString(headerBytes);

        var lines = headers.Split("\r\n", StringSplitOptions.None);
        if (lines.Length < 2 || !ValidRequestLine(lines[0]))
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
        }

        string? hostValue = null;
        foreach (var line in lines.Skip(1))
        {
            var separator = line.IndexOf(':');
            if (separator <= 0 || line[..separator].Any(character =>
                    !char.IsAsciiLetterOrDigit(character) && character != '-'))
            {
                return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
            }
            if (!line[..separator].Equals("Host", StringComparison.OrdinalIgnoreCase)) continue;
            if (hostValue is not null)
            {
                return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
            }
            hostValue = line[(separator + 1)..].Trim();
        }

        if (string.IsNullOrWhiteSpace(hostValue))
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.MissingHost);
        }
        if (!TrySplitHostAndPort(hostValue, 80, out var host, out var port))
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
        }

        return new NetworkGuardInspectionResult(
            policy.Allows(host, port)
                ? NetworkGuardInspectionState.Allowed
                : NetworkGuardInspectionState.Denied,
            host,
            port);
    }

    public static NetworkGuardInspectionResult InspectTlsClientHello(
        ReadOnlySpan<byte> data,
        ControlledNetworkPolicy policy)
    {
        ArgumentNullException.ThrowIfNull(policy);
        if (data.Length > MaximumHandshakeBytes)
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.UnsupportedProtocol);
        }
        var handshake = new byte[MaximumHandshakeBytes];
        var handshakeBytes = 0;
        var dataOffset = 0;
        var expectedHandshakeBytes = -1;
        while (expectedHandshakeBytes < 0 || handshakeBytes < expectedHandshakeBytes)
        {
            if (data.Length - dataOffset < 5)
            {
                return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Incomplete);
            }
            if (data[dataOffset] != 22)
            {
                return new NetworkGuardInspectionResult(NetworkGuardInspectionState.UnsupportedProtocol);
            }
            var recordLength = BinaryPrimitives.ReadUInt16BigEndian(data.Slice(dataOffset + 3, 2));
            dataOffset += 5;
            if (recordLength > MaximumHandshakeBytes - handshakeBytes)
            {
                return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
            }
            if (data.Length - dataOffset < recordLength)
            {
                return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Incomplete);
            }
            data.Slice(dataOffset, recordLength).CopyTo(handshake.AsSpan(handshakeBytes));
            handshakeBytes += recordLength;
            dataOffset += recordLength;
            if (handshakeBytes >= 4 && expectedHandshakeBytes < 0)
            {
                if (handshake[0] != 1)
                {
                    return new NetworkGuardInspectionResult(NetworkGuardInspectionState.UnsupportedProtocol);
                }
                expectedHandshakeBytes = 4 + ReadUInt24(handshake.AsSpan(1, 3));
                if (expectedHandshakeBytes > MaximumHandshakeBytes)
                {
                    return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
                }
            }
        }
        if (expectedHandshakeBytes < 4 || handshakeBytes < expectedHandshakeBytes)
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Incomplete);
        }
        var hello = handshake.AsSpan(4, expectedHandshakeBytes - 4);
        var offset = 0;
        if (!Skip(hello, ref offset, 2 + 32) ||
            !SkipVector8(hello, ref offset) ||
            !SkipVector16(hello, ref offset) ||
            !SkipVector8(hello, ref offset))
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
        }
        if (offset == hello.Length)
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.MissingHost);
        }
        if (!TryReadUInt16(hello, ref offset, out var extensionsLength) ||
            extensionsLength != hello.Length - offset)
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
        }

        string? serverName = null;
        var extensionEnd = offset + extensionsLength;
        while (offset < extensionEnd)
        {
            if (!TryReadUInt16(hello, ref offset, out var extensionType) ||
                !TryReadUInt16(hello, ref offset, out var extensionLength) ||
                extensionLength > extensionEnd - offset)
            {
                return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
            }
            if (extensionType == 0)
            {
                if (serverName is not null ||
                    !TryReadServerName(hello.Slice(offset, extensionLength), out serverName))
                {
                    return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
                }
            }
            offset += extensionLength;
        }

        if (string.IsNullOrWhiteSpace(serverName))
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.MissingHost);
        }
        string host;
        try
        {
            host = ControlledNetworkPolicyValidator.NormalizeHost(serverName, allowWildcard: false);
        }
        catch (ArgumentException)
        {
            return new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed);
        }

        return new NetworkGuardInspectionResult(
            policy.Allows(host, 443)
                ? NetworkGuardInspectionState.Allowed
                : NetworkGuardInspectionState.Denied,
            host,
            443);
    }

    private static bool TryReadServerName(ReadOnlySpan<byte> extension, out string? serverName)
    {
        serverName = null;
        if (extension.Length < 2) return false;
        var listLength = BinaryPrimitives.ReadUInt16BigEndian(extension[..2]);
        if (listLength != extension.Length - 2) return false;
        var offset = 2;
        while (offset < extension.Length)
        {
            if (extension.Length - offset < 3) return false;
            var nameType = extension[offset++];
            var nameLength = BinaryPrimitives.ReadUInt16BigEndian(extension.Slice(offset, 2));
            offset += 2;
            if (nameLength == 0 || nameLength > extension.Length - offset) return false;
            if (nameType == 0)
            {
                if (serverName is not null) return false;
                var value = extension.Slice(offset, nameLength);
                if (!IsVisibleAscii(value)) return false;
                serverName = Encoding.ASCII.GetString(value);
            }
            offset += nameLength;
        }
        return offset == extension.Length;
    }

    private static bool TrySplitHostAndPort(
        string value,
        int defaultPort,
        out string host,
        out int port)
    {
        host = value;
        port = defaultPort;
        if (value.StartsWith("[", StringComparison.Ordinal) || value.EndsWith("]", StringComparison.Ordinal))
        {
            return false;
        }
        var separator = value.LastIndexOf(':');
        if (separator >= 0)
        {
            if (value.IndexOf(':') != separator ||
                !int.TryParse(value[(separator + 1)..], out port) || port is < 1 or > 65_535)
            {
                return false;
            }
            host = value[..separator];
        }
        try
        {
            host = ControlledNetworkPolicyValidator.NormalizeHost(host, allowWildcard: false);
            return true;
        }
        catch (ArgumentException)
        {
            return false;
        }
    }

    private static bool ValidRequestLine(string value)
    {
        var parts = value.Split(' ', StringSplitOptions.RemoveEmptyEntries);
        return parts.Length == 3 && parts[0].Length is > 0 and <= 16 &&
            parts[0].All(character => character is >= 'A' and <= 'Z') &&
            !parts[0].Equals("CONNECT", StringComparison.Ordinal) &&
            parts[1].Length is > 0 and <= 8_192 &&
            (parts[1].StartsWith("/", StringComparison.Ordinal) || parts[1] == "*") &&
            parts[2] is "HTTP/1.0" or "HTTP/1.1";
    }

    private static bool IsVisibleAscii(ReadOnlySpan<byte> value)
    {
        foreach (var character in value)
        {
            if (character is < 0x21 or > 0x7e) return false;
        }
        return true;
    }

    private static int ReadUInt24(ReadOnlySpan<byte> value) =>
        value[0] << 16 | value[1] << 8 | value[2];

    private static bool TryReadUInt16(ReadOnlySpan<byte> value, ref int offset, out int result)
    {
        result = 0;
        if (value.Length - offset < 2) return false;
        result = BinaryPrimitives.ReadUInt16BigEndian(value.Slice(offset, 2));
        offset += 2;
        return true;
    }

    private static bool Skip(ReadOnlySpan<byte> value, ref int offset, int count)
    {
        if (count < 0 || count > value.Length - offset) return false;
        offset += count;
        return true;
    }

    private static bool SkipVector8(ReadOnlySpan<byte> value, ref int offset)
    {
        if (offset >= value.Length) return false;
        var count = value[offset++];
        return Skip(value, ref offset, count);
    }

    private static bool SkipVector16(ReadOnlySpan<byte> value, ref int offset)
    {
        if (!TryReadUInt16(value, ref offset, out var count)) return false;
        return Skip(value, ref offset, count);
    }
}
