using System.Buffers.Binary;
using System.Text;
using ChatOS.NetworkGuard.Contracts;

namespace ChatOS.Connector.Tests;

public sealed class NetworkGuardProtocolInspectorTests
{
    private static readonly ControlledNetworkPolicy Policy = new(
        "policy-1",
        "owner-1",
        "device-1",
        "workspace-1",
        "S-1-5-21-100-200-300-400",
        ["api.example.com", "*.allowed.example"],
        [80, 443],
        DateTimeOffset.UtcNow.AddHours(1),
        "key-1");

    [Fact]
    public void HttpInspectorAllowsExactHostAndRejectsDuplicatesOrDeniedHost()
    {
        var allowed = NetworkGuardProtocolInspector.InspectHttp(
            "GET /v1 HTTP/1.1\r\nHost: API.EXAMPLE.COM:80\r\nConnection: close\r\n\r\n"u8,
            Policy);
        Assert.True(allowed.IsAllowed);
        Assert.Equal("api.example.com", allowed.Host);
        Assert.Equal(80, allowed.Port);

        var denied = NetworkGuardProtocolInspector.InspectHttp(
            "GET / HTTP/1.1\r\nHost: denied.example.com\r\n\r\n"u8,
            Policy);
        Assert.Equal(NetworkGuardInspectionState.Denied, denied.State);

        var duplicate = NetworkGuardProtocolInspector.InspectHttp(
            "GET / HTTP/1.1\r\nHost: api.example.com\r\nHost: api.example.com\r\n\r\n"u8,
            Policy);
        Assert.Equal(NetworkGuardInspectionState.Malformed, duplicate.State);
    }

    [Fact]
    public void HttpInspectorFailsClosedForMissingHostIpLiteralAndIncompleteHeaders()
    {
        Assert.Equal(
            NetworkGuardInspectionState.MissingHost,
            NetworkGuardProtocolInspector.InspectHttp(
                "GET / HTTP/1.1\r\nConnection: close\r\n\r\n"u8,
                Policy).State);
        Assert.Equal(
            NetworkGuardInspectionState.Malformed,
            NetworkGuardProtocolInspector.InspectHttp(
                "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"u8,
                Policy).State);
        Assert.Equal(
            NetworkGuardInspectionState.Incomplete,
            NetworkGuardProtocolInspector.InspectHttp(
                "GET / HTTP/1.1\r\nHost: api.example.com\r\n"u8,
                Policy).State);
        Assert.Equal(
            NetworkGuardInspectionState.Malformed,
            NetworkGuardProtocolInspector.InspectHttp(
                "GET http://denied.example.com/ HTTP/1.1\r\nHost: api.example.com\r\n\r\n"u8,
                Policy).State);

        var nonAscii = "GET / HTTP/1.1\r\nHost: api.example.com\r\nX-Test: ok\r\n\r\n"u8.ToArray();
        nonAscii[^6] = 0xff;
        Assert.Equal(
            NetworkGuardInspectionState.Malformed,
            NetworkGuardProtocolInspector.InspectHttp(nonAscii, Policy).State);
    }

    [Fact]
    public void TlsInspectorAllowsVisibleSniAndRejectsSameConnectionWithDeniedSni()
    {
        var allowed = NetworkGuardProtocolInspector.InspectTlsClientHello(
            ClientHello("api.example.com"),
            Policy);
        Assert.True(allowed.IsAllowed);
        Assert.Equal("api.example.com", allowed.Host);

        var wildcard = NetworkGuardProtocolInspector.InspectTlsClientHello(
            ClientHello("one.allowed.example"),
            Policy);
        Assert.True(wildcard.IsAllowed);

        var denied = NetworkGuardProtocolInspector.InspectTlsClientHello(
            ClientHello("denied.example.com"),
            Policy);
        Assert.Equal(NetworkGuardInspectionState.Denied, denied.State);

        var fragmented = NetworkGuardProtocolInspector.InspectTlsClientHello(
            FragmentAcrossRecords(ClientHello("api.example.com")),
            Policy);
        Assert.True(fragmented.IsAllowed);
    }

    [Fact]
    public void TlsInspectorFailsClosedForTruncationMissingSniAndNonHandshakeTraffic()
    {
        var hello = ClientHello("api.example.com");
        Assert.Equal(
            NetworkGuardInspectionState.Incomplete,
            NetworkGuardProtocolInspector.InspectTlsClientHello(hello[..^3], Policy).State);
        Assert.Equal(
            NetworkGuardInspectionState.MissingHost,
            NetworkGuardProtocolInspector.InspectTlsClientHello(ClientHello(null), Policy).State);
        Assert.Equal(
            NetworkGuardInspectionState.UnsupportedProtocol,
            NetworkGuardProtocolInspector.InspectTlsClientHello("plain text"u8, Policy).State);
    }

    private static byte[] ClientHello(string? host)
    {
        var body = new List<byte> { 0x03, 0x03 };
        body.AddRange(new byte[32]);
        body.Add(0);
        body.AddRange([0, 2, 0x13, 0x01]);
        body.AddRange([1, 0]);

        var extensions = new List<byte>();
        if (host is not null)
        {
            var name = Encoding.ASCII.GetBytes(host);
            var serverNames = new List<byte> { 0 };
            AddUInt16(serverNames, name.Length);
            serverNames.AddRange(name);
            var sni = new List<byte>();
            AddUInt16(sni, serverNames.Count);
            sni.AddRange(serverNames);
            AddUInt16(extensions, 0);
            AddUInt16(extensions, sni.Count);
            extensions.AddRange(sni);
        }
        AddUInt16(body, extensions.Count);
        body.AddRange(extensions);

        var handshake = new List<byte> { 1 };
        AddUInt24(handshake, body.Count);
        handshake.AddRange(body);
        var record = new List<byte> { 22, 0x03, 0x01 };
        AddUInt16(record, handshake.Count);
        record.AddRange(handshake);
        return record.ToArray();
    }

    private static byte[] FragmentAcrossRecords(byte[] singleRecord)
    {
        var payloadLength = BinaryPrimitives.ReadUInt16BigEndian(singleRecord.AsSpan(3, 2));
        var payload = singleRecord.AsSpan(5, payloadLength);
        var firstLength = Math.Min(11, payload.Length - 1);
        var output = new List<byte>();
        AddRecord(output, payload[..firstLength]);
        AddRecord(output, payload[firstLength..]);
        return output.ToArray();
    }

    private static void AddRecord(List<byte> output, ReadOnlySpan<byte> payload)
    {
        output.AddRange([22, 0x03, 0x01]);
        AddUInt16(output, payload.Length);
        output.AddRange(payload.ToArray());
    }

    private static void AddUInt16(List<byte> output, int value)
    {
        Span<byte> bytes = stackalloc byte[2];
        BinaryPrimitives.WriteUInt16BigEndian(bytes, checked((ushort)value));
        output.AddRange(bytes.ToArray());
    }

    private static void AddUInt24(List<byte> output, int value)
    {
        output.Add(checked((byte)(value >> 16)));
        output.Add(checked((byte)(value >> 8)));
        output.Add(checked((byte)value));
    }
}
