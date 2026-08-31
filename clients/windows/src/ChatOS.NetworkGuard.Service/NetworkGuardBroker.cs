using System.Buffers.Binary;
using System.Net;
using System.Net.Sockets;
using ChatOS.NetworkGuard.Contracts;
using Microsoft.Extensions.Options;

namespace ChatOS.NetworkGuard.Service;

internal sealed class NetworkGuardBrokerState
{
    private int _ready;

    public bool IsReady => Volatile.Read(ref _ready) != 0;

    public void SetReady(bool ready) => Volatile.Write(ref _ready, ready ? 1 : 0);
}

internal sealed record NetworkGuardRedirectContext(Guid LeaseId, int OriginalPort);

internal interface INetworkGuardRedirectContextResolver
{
    NetworkGuardRedirectContext Resolve(Socket socket);
}

internal interface INetworkGuardAddressResolver
{
    Task<IReadOnlyList<IPAddress>> ResolveAsync(
        string host,
        CancellationToken cancellationToken = default);
}

internal interface INetworkGuardUpstreamConnector
{
    Task<TcpClient> ConnectAsync(
        string host,
        int port,
        CancellationToken cancellationToken = default);
}

internal sealed class SystemNetworkGuardAddressResolver : INetworkGuardAddressResolver
{
    public async Task<IReadOnlyList<IPAddress>> ResolveAsync(
        string host,
        CancellationToken cancellationToken = default) =>
        (await Dns.GetHostAddressesAsync(host, cancellationToken).ConfigureAwait(false))
        .Where(address => address.AddressFamily is AddressFamily.InterNetwork or AddressFamily.InterNetworkV6)
        .Distinct()
        .ToArray();
}

internal sealed class NetworkGuardUpstreamConnector(
    INetworkGuardAddressResolver addressResolver,
    IOptions<NetworkGuardServiceOptions> options) : INetworkGuardUpstreamConnector
{
    public async Task<TcpClient> ConnectAsync(
        string host,
        int port,
        CancellationToken cancellationToken = default)
    {
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(options.Value.ConnectTimeout);
        var addresses = await addressResolver.ResolveAsync(host, timeout.Token).ConfigureAwait(false);
        Exception? lastError = null;
        foreach (var address in addresses)
        {
            var client = new TcpClient(address.AddressFamily) { NoDelay = true };
            try
            {
                await client.ConnectAsync(address, port, timeout.Token).ConfigureAwait(false);
                return client;
            }
            catch (Exception exception) when (
                exception is SocketException or OperationCanceledException)
            {
                lastError = exception;
                client.Dispose();
                if (timeout.IsCancellationRequested) break;
            }
        }
        throw new IOException("NetworkGuard could not connect to an approved host.", lastError);
    }
}

internal sealed class WindowsWfpRedirectContextResolver : INetworkGuardRedirectContextResolver
{
    internal const uint ContextMagic = 0x31524743;
    private const int QueryWfpRedirectContext = 0x580000DD;

    public NetworkGuardRedirectContext Resolve(Socket socket)
    {
        if (!OperatingSystem.IsWindows()) throw new PlatformNotSupportedException();
        var buffer = new byte[32];
        int count;
        try
        {
            count = socket.IOControl(
                unchecked((IOControlCode)QueryWfpRedirectContext),
                null,
                buffer);
        }
        catch (SocketException exception)
        {
            throw new InvalidOperationException("Connection has no trusted WFP redirect context.", exception);
        }
        if (count < 24 || BinaryPrimitives.ReadUInt32LittleEndian(buffer) != ContextMagic)
        {
            throw new InvalidOperationException("WFP redirect context is invalid.");
        }
        var version = BinaryPrimitives.ReadUInt16LittleEndian(buffer.AsSpan(4));
        var port = BinaryPrimitives.ReadUInt16LittleEndian(buffer.AsSpan(6));
        if (version != NetworkGuardProtocol.MajorVersion || port is not (80 or 443))
        {
            throw new InvalidOperationException("WFP redirect context version or port is invalid.");
        }
        return new NetworkGuardRedirectContext(new Guid(buffer.AsSpan(8, 16)), port);
    }
}

internal sealed class NetworkGuardBrokerConnectionHandler(
    INetworkGuardLeasePolicyStore leases,
    INetworkGuardRedirectContextResolver redirectContextResolver,
    INetworkGuardUpstreamConnector upstreamConnector,
    IOptions<NetworkGuardServiceOptions> options)
{
    public async Task HandleAsync(TcpClient client, CancellationToken cancellationToken)
    {
        using (client)
        {
            client.NoDelay = true;
            var redirect = redirectContextResolver.Resolve(client.Client);
            if (!leases.TryGetActive(redirect.LeaseId, out var lease) || lease is null)
            {
                return;
            }

            var (prefix, inspection) = await ReadAndInspectAsync(
                client.GetStream(),
                redirect.OriginalPort,
                lease.Policy,
                cancellationToken).ConfigureAwait(false);
            if (!inspection.IsAllowed ||
                inspection.Port != redirect.OriginalPort ||
                string.IsNullOrWhiteSpace(inspection.Host))
            {
                return;
            }

            using var remote = await upstreamConnector.ConnectAsync(
                inspection.Host,
                redirect.OriginalPort,
                cancellationToken).ConfigureAwait(false);
            await RelayAsync(client, remote, prefix, cancellationToken).ConfigureAwait(false);
        }
    }

    private async Task<(byte[] Prefix, NetworkGuardInspectionResult Inspection)> ReadAndInspectAsync(
        NetworkStream stream,
        int originalPort,
        ControlledNetworkPolicy policy,
        CancellationToken cancellationToken)
    {
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(options.Value.HandshakeTimeout);
        var buffer = new byte[NetworkGuardProtocolInspector.MaximumHandshakeBytes];
        var count = 0;
        while (count < buffer.Length)
        {
            var read = await stream.ReadAsync(buffer.AsMemory(count), timeout.Token).ConfigureAwait(false);
            if (read == 0) throw new EndOfStreamException("NetworkGuard client closed before inspection.");
            count += read;
            var inspection = originalPort switch
            {
                80 => NetworkGuardProtocolInspector.InspectHttp(buffer.AsSpan(0, count), policy),
                443 => NetworkGuardProtocolInspector.InspectTlsClientHello(buffer.AsSpan(0, count), policy),
                _ => new NetworkGuardInspectionResult(NetworkGuardInspectionState.UnsupportedProtocol),
            };
            if (inspection.State is not NetworkGuardInspectionState.Incomplete)
            {
                return (buffer.AsSpan(0, count).ToArray(), inspection);
            }
        }
        return (
            buffer,
            new NetworkGuardInspectionResult(NetworkGuardInspectionState.Malformed));
    }

    private static async Task RelayAsync(
        TcpClient local,
        TcpClient remote,
        byte[] prefix,
        CancellationToken cancellationToken)
    {
        var localStream = local.GetStream();
        var remoteStream = remote.GetStream();
        await remoteStream.WriteAsync(prefix, cancellationToken).ConfigureAwait(false);
        await remoteStream.FlushAsync(cancellationToken).ConfigureAwait(false);

        var upload = localStream.CopyToAsync(remoteStream, cancellationToken);
        var download = remoteStream.CopyToAsync(localStream, cancellationToken);
        await Task.WhenAny(upload, download).ConfigureAwait(false);
        TryShutdown(local.Client);
        TryShutdown(remote.Client);
        try
        {
            await Task.WhenAll(upload, download).ConfigureAwait(false);
        }
        catch (Exception exception) when (
            exception is IOException or SocketException or OperationCanceledException)
        {
        }
    }

    private static void TryShutdown(Socket socket)
    {
        try
        {
            socket.Shutdown(SocketShutdown.Both);
        }
        catch (SocketException)
        {
        }
        catch (ObjectDisposedException)
        {
        }
    }
}

internal sealed class NetworkGuardBrokerHostedService(
    NetworkGuardBrokerConnectionHandler handler,
    NetworkGuardBrokerState state,
    IOptions<NetworkGuardServiceOptions> options,
    ILogger<NetworkGuardBrokerHostedService> logger) : BackgroundService
{
    private readonly List<TcpListener> _listeners = [];
    private readonly SemaphoreSlim _connections = new(128, 128);

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException("NetworkGuard broker requires Windows.");
        }

        try
        {
            foreach (var port in new[] { options.Value.HttpBrokerPort, options.Value.HttpsBrokerPort }.Distinct())
            {
                foreach (var address in new[] { IPAddress.Loopback, IPAddress.IPv6Loopback })
                {
                    var listener = new TcpListener(address, port);
                    listener.Server.ExclusiveAddressUse = true;
                    if (address.AddressFamily == AddressFamily.InterNetworkV6)
                    {
                        listener.Server.DualMode = false;
                    }
                    listener.Start(128);
                    _listeners.Add(listener);
                }
            }
            state.SetReady(true);

            var accepts = _listeners.Select(listener => AcceptLoopAsync(listener, stoppingToken)).ToArray();
            await Task.WhenAll(accepts).ConfigureAwait(false);
        }
        finally
        {
            state.SetReady(false);
            foreach (var listener in _listeners) listener.Stop();
            _listeners.Clear();
        }
    }

    private async Task AcceptLoopAsync(TcpListener listener, CancellationToken stoppingToken)
    {
        while (!stoppingToken.IsCancellationRequested)
        {
            TcpClient client;
            try
            {
                client = await listener.AcceptTcpClientAsync(stoppingToken).ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
                return;
            }
            await _connections.WaitAsync(stoppingToken).ConfigureAwait(false);
            _ = HandleConnectionAsync(client, stoppingToken);
        }
    }

    private async Task HandleConnectionAsync(TcpClient client, CancellationToken stoppingToken)
    {
        try
        {
            await handler.HandleAsync(client, stoppingToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            logger.LogWarning(
                "NetworkGuard broker connection failed. Failure type: {FailureType}.",
                exception.GetType().Name);
            client.Dispose();
        }
        finally
        {
            _connections.Release();
        }
    }
}
