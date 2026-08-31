using System.Buffers.Binary;
using ChatOS.Connector.NetworkGuard;
using ChatOS.NetworkGuard.Contracts;

namespace ChatOS.Connector.Tests;

public sealed class NetworkGuardProtocolTests
{
    [Fact]
    public async Task CodecRoundTripsLengthPrefixedFramesAcrossPartialReads()
    {
        var request = new NetworkGuardRequest(
            NetworkGuardProtocol.MajorVersion,
            NetworkGuardProtocol.MinorVersion,
            "correlation-1",
            NetworkGuardOperation.AcquireLease,
            PolicyEnvelope(),
            "S-1-15-2-1234",
            42);
        await using var buffer = new MemoryStream();
        await NetworkGuardProtocolCodec.WriteAsync(buffer, request);
        buffer.Position = 0;
        await using var partial = new PartialReadStream(buffer, 3);

        var decoded = await NetworkGuardProtocolCodec.ReadAsync<NetworkGuardRequest>(partial);

        Assert.Equal(request.ProtocolMajor, decoded.ProtocolMajor);
        Assert.Equal(request.ProtocolMinor, decoded.ProtocolMinor);
        Assert.Equal(request.CorrelationId, decoded.CorrelationId);
        Assert.Equal(request.Operation, decoded.Operation);
        Assert.Equal(request.AppContainerSid, decoded.AppContainerSid);
        Assert.Equal(request.ProcessId, decoded.ProcessId);
        Assert.Equal(request.Policy!.PolicyRevision, decoded.Policy!.PolicyRevision);
        Assert.Equal(request.Policy.AllowedHosts, decoded.Policy.AllowedHosts);
        Assert.Equal(request.Policy.AllowedPorts, decoded.Policy.AllowedPorts);
    }

    [Fact]
    public async Task CodecRejectsOversizedTruncatedAndInvalidJsonFrames()
    {
        await using var oversized = Header(NetworkGuardProtocol.MaximumFrameBytes + 1);
        await Assert.ThrowsAsync<InvalidDataException>(() =>
            NetworkGuardProtocolCodec.ReadAsync<NetworkGuardResponse>(oversized));

        await using var truncated = Header(10, [1, 2, 3]);
        await Assert.ThrowsAsync<EndOfStreamException>(() =>
            NetworkGuardProtocolCodec.ReadAsync<NetworkGuardResponse>(truncated));

        await using var invalid = Header(3, "bad"u8.ToArray());
        await Assert.ThrowsAsync<InvalidDataException>(() =>
            NetworkGuardProtocolCodec.ReadAsync<NetworkGuardResponse>(invalid));
    }

    [Fact]
    public async Task ClientRequiresMatchingProtocolDriverAndSelfTest()
    {
        var transport = new StubTransport(request => Response(request, driverReady: true, selfTest: true));
        var client = new ControlledNetworkGuardClient(transport, requireWindowsPlatform: false);

        Assert.True((await client.CheckReadinessAsync()).IsReady);

        transport.Handler = request => Response(request, protocolMajor: 2, driverReady: true, selfTest: true);
        Assert.Equal(NetworkGuardReadinessState.ProtocolMismatch, (await client.CheckReadinessAsync()).State);

        transport.Handler = request => Response(request, driverReady: false, selfTest: true);
        Assert.Equal(NetworkGuardReadinessState.DriverUnavailable, (await client.CheckReadinessAsync()).State);

        transport.Handler = request => Response(request, driverReady: true, selfTest: false);
        Assert.Equal(NetworkGuardReadinessState.SelfTestFailed, (await client.CheckReadinessAsync()).State);
    }

    [Fact]
    public async Task ClientRejectsCorrelationMismatchAndUnsafeLease()
    {
        var now = DateTimeOffset.Parse("2026-08-30T12:00:00Z");
        var time = new FrozenTimeProvider(now);
        var transport = new StubTransport(request => Response(
            request,
            correlationId: "wrong",
            driverReady: true,
            selfTest: true));
        var client = new ControlledNetworkGuardClient(
            transport,
            time,
            requireWindowsPlatform: false);
        Assert.Equal(NetworkGuardReadinessState.InvalidResponse, (await client.CheckReadinessAsync()).State);

        transport.Handler = request => Response(
            request,
            leaseId: "lease-1",
            leaseExpiresAt: now.AddHours(25));
        await Assert.ThrowsAsync<InvalidDataException>(() => client.AcquireLeaseAsync(
            PolicyEnvelope(now.AddHours(1)),
            "S-1-15-2-1234",
            42));
    }

    [Fact]
    public async Task ClientAcquiresRenewsAndReleasesExactLeaseIdentity()
    {
        var now = DateTimeOffset.Parse("2026-08-30T12:00:00Z");
        var time = new FrozenTimeProvider(now);
        var operations = new List<NetworkGuardRequest>();
        var transport = new StubTransport(request =>
        {
            operations.Add(request);
            return Response(
                request,
                leaseId: request.LeaseId ?? "lease-1",
                leaseExpiresAt: now.AddMinutes(5));
        });
        var client = new ControlledNetworkGuardClient(
            transport,
            time,
            requireWindowsPlatform: false);

        var lease = await client.AcquireLeaseAsync(
            PolicyEnvelope(now.AddHours(1)),
            "S-1-15-2-1234",
            42);
        var renewed = await client.RenewLeaseAsync(lease);
        await client.ReleaseLeaseAsync(renewed);

        Assert.Equal(
            [NetworkGuardOperation.AcquireLease, NetworkGuardOperation.RenewLease, NetworkGuardOperation.ReleaseLease],
            operations.Select(value => value.Operation));
        Assert.All(operations, value => Assert.Equal("S-1-15-2-1234", value.AppContainerSid));
        Assert.Equal("lease-1", operations[1].LeaseId);
        Assert.Equal("lease-1", operations[2].LeaseId);
    }

    [Theory]
    [InlineData("S-1-5-18", true)]
    [InlineData("S-1-5-19", false)]
    [InlineData("S-1-5-20", false)]
    [InlineData("S-1-5-21-1234", false)]
    [InlineData("S-1-15-2-1234", false)]
    public void PipeServerMustRunAsTrustedWindowsServiceAccount(string sid, bool expected) =>
        Assert.Equal(expected, WindowsNetworkGuardServerIdentityVerifier.IsTrustedServiceSid(sid));

    private static NetworkGuardResponse Response(
        NetworkGuardRequest request,
        int protocolMajor = NetworkGuardProtocol.MajorVersion,
        string? correlationId = null,
        bool driverReady = true,
        bool selfTest = true,
        string? leaseId = null,
        DateTimeOffset? leaseExpiresAt = null) => new(
            protocolMajor,
            NetworkGuardProtocol.MinorVersion,
            correlationId ?? request.CorrelationId,
            Success: true,
            ServiceVersion: "1.0.0",
            DriverVersion: "1.0.0",
            DriverReady: driverReady,
            SelfTestPassed: selfTest,
            LeaseId: leaseId,
            LeaseExpiresAt: leaseExpiresAt);

    private static ControlledNetworkPolicyEnvelope PolicyEnvelope(DateTimeOffset? expiry = null) => new(
        "policy-1",
        "owner-1",
        "device-1",
        "workspace-1",
        "S-1-5-21-100-200-300-400",
        ["example.com"],
        [443],
        expiry ?? DateTimeOffset.UtcNow.AddHours(1),
        "key-1",
        "ed25519",
        "signature");

    private static MemoryStream Header(int length, byte[]? body = null)
    {
        var stream = new MemoryStream();
        var header = new byte[4];
        BinaryPrimitives.WriteInt32BigEndian(header, length);
        stream.Write(header);
        if (body is not null) stream.Write(body);
        stream.Position = 0;
        return stream;
    }

    private sealed class StubTransport(Func<NetworkGuardRequest, NetworkGuardResponse> handler)
        : INetworkGuardTransport
    {
        public Func<NetworkGuardRequest, NetworkGuardResponse> Handler { get; set; } = handler;

        public Task<NetworkGuardResponse> SendAsync(
            NetworkGuardRequest request,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(Handler(request));
    }

    private sealed class PartialReadStream(Stream inner, int maximumRead) : Stream
    {
        public override bool CanRead => true;
        public override bool CanSeek => false;
        public override bool CanWrite => false;
        public override long Length => inner.Length;
        public override long Position { get => inner.Position; set => throw new NotSupportedException(); }
        public override void Flush() => throw new NotSupportedException();
        public override int Read(byte[] buffer, int offset, int count) =>
            inner.Read(buffer, offset, Math.Min(count, maximumRead));
        public override ValueTask<int> ReadAsync(
            Memory<byte> buffer,
            CancellationToken cancellationToken = default) =>
            inner.ReadAsync(buffer[..Math.Min(buffer.Length, maximumRead)], cancellationToken);
        public override long Seek(long offset, SeekOrigin origin) => throw new NotSupportedException();
        public override void SetLength(long value) => throw new NotSupportedException();
        public override void Write(byte[] buffer, int offset, int count) => throw new NotSupportedException();
        protected override void Dispose(bool disposing)
        {
            if (disposing) inner.Dispose();
            base.Dispose(disposing);
        }
    }

    private sealed class FrozenTimeProvider(DateTimeOffset value) : TimeProvider
    {
        public override DateTimeOffset GetUtcNow() => value;
    }
}
