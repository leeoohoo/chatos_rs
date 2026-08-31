using System.Net;
using System.Net.Sockets;
using System.Text;
using ChatOS.NetworkGuard.Contracts;
using ChatOS.NetworkGuard.Service;
using Microsoft.Extensions.Options;

namespace ChatOS.NetworkGuard.Tests;

public sealed class NetworkGuardBrokerTests
{
    [Fact]
    public async Task AllowedHttpHostIsForwardedBidirectionally()
    {
        var leaseId = Guid.NewGuid();
        using var destination = new TcpListener(IPAddress.Loopback, 0);
        destination.Start();
        var destinationPort = ((IPEndPoint)destination.LocalEndpoint).Port;
        var connector = new TestUpstreamConnector(destinationPort);
        var handler = CreateHandler(leaseId, connector);
        var (client, accepted) = await CreateConnectedPairAsync();
        using var clientLifetime = client;
        using var acceptedLifetime = accepted;
        using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        var handling = handler.HandleAsync(accepted, cancellation.Token);

        var request = "GET /status HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n";
        await client.GetStream().WriteAsync(Encoding.ASCII.GetBytes(request), cancellation.Token);
        using var upstream = await destination.AcceptTcpClientAsync(cancellation.Token);
        var received = new byte[request.Length];
        await upstream.GetStream().ReadExactlyAsync(received, cancellation.Token);
        await upstream.GetStream().WriteAsync("HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nOK"u8.ToArray(), cancellation.Token);
        upstream.Client.Shutdown(SocketShutdown.Send);

        var response = new byte[128];
        var count = await client.GetStream().ReadAsync(response, cancellation.Token);
        client.Dispose();
        await handling;

        Assert.Equal(request, Encoding.ASCII.GetString(received));
        Assert.Contains("200 OK", Encoding.ASCII.GetString(response, 0, count), StringComparison.Ordinal);
        Assert.Equal(("example.com", 80), connector.LastTarget);
    }

    [Fact]
    public async Task DeniedHostClosesWithoutOpeningUpstream()
    {
        var connector = new TestUpstreamConnector(destinationPort: 1);
        var handler = CreateHandler(Guid.NewGuid(), connector);
        var (client, accepted) = await CreateConnectedPairAsync();
        using var clientLifetime = client;
        using var acceptedLifetime = accepted;
        using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        var handling = handler.HandleAsync(accepted, cancellation.Token);

        await client.GetStream().WriteAsync(
            "GET / HTTP/1.1\r\nHost: denied.example.com\r\n\r\n"u8.ToArray(),
            cancellation.Token);
        var buffer = new byte[1];
        var count = await client.GetStream().ReadAsync(buffer, cancellation.Token);
        await handling;

        Assert.Equal(0, count);
        Assert.Null(connector.LastTarget);
    }

    private static NetworkGuardBrokerConnectionHandler CreateHandler(
        Guid leaseId,
        TestUpstreamConnector connector)
    {
        var policy = new ControlledNetworkPolicy(
            "policy-1",
            "owner-1",
            "device-1",
            "workspace-1",
            "S-1-5-21-100-200-300-400",
            ["example.com"],
            [80],
            DateTimeOffset.UtcNow.AddHours(1),
            "key-1");
        var lease = new ActiveNetworkGuardLease(
            leaseId,
            policy,
            "S-1-15-2-111-222",
            41,
            "S-1-5-21-100-200-300-400",
            DateTimeOffset.UtcNow.AddMinutes(2));
        return new NetworkGuardBrokerConnectionHandler(
            new TestLeaseStore(lease),
            new TestRedirectResolver(leaseId),
            connector,
            Options.Create(new NetworkGuardServiceOptions
            {
                HandshakeTimeout = TimeSpan.FromSeconds(2),
                ConnectTimeout = TimeSpan.FromSeconds(2),
            }));
    }

    private static async Task<(TcpClient Client, TcpClient Accepted)> CreateConnectedPairAsync()
    {
        var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        try
        {
            var client = new TcpClient();
            var connect = client.ConnectAsync((IPEndPoint)listener.LocalEndpoint);
            var accepted = await listener.AcceptTcpClientAsync();
            await connect;
            return (client, accepted);
        }
        finally
        {
            listener.Stop();
        }
    }

    private sealed class TestLeaseStore(ActiveNetworkGuardLease lease) : INetworkGuardLeasePolicyStore
    {
        public bool TryGetActive(Guid leaseId, out ActiveNetworkGuardLease? value)
        {
            value = leaseId == lease.LeaseId ? lease : null;
            return value is not null;
        }
    }

    private sealed class TestRedirectResolver(Guid leaseId) : INetworkGuardRedirectContextResolver
    {
        public NetworkGuardRedirectContext Resolve(Socket socket) => new(leaseId, 80);
    }

    private sealed class TestUpstreamConnector(int destinationPort) : INetworkGuardUpstreamConnector
    {
        public (string Host, int Port)? LastTarget { get; private set; }

        public async Task<TcpClient> ConnectAsync(
            string host,
            int port,
            CancellationToken cancellationToken = default)
        {
            LastTarget = (host, port);
            var client = new TcpClient();
            await client.ConnectAsync(IPAddress.Loopback, destinationPort, cancellationToken);
            return client;
        }
    }
}
