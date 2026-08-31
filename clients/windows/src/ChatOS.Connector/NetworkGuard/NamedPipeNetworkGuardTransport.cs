using System.IO.Pipes;
using System.Security.Principal;
using ChatOS.NetworkGuard.Contracts;

namespace ChatOS.Connector.NetworkGuard;

internal sealed class NamedPipeNetworkGuardTransport(
    TimeSpan? connectTimeout = null,
    INetworkGuardServerIdentityVerifier? identityVerifier = null) : INetworkGuardTransport
{
    private readonly TimeSpan _connectTimeout = connectTimeout ?? TimeSpan.FromSeconds(2);
    private readonly INetworkGuardServerIdentityVerifier _identityVerifier =
        identityVerifier ?? new WindowsNetworkGuardServerIdentityVerifier();

    public async Task<NetworkGuardResponse> SendAsync(
        NetworkGuardRequest request,
        CancellationToken cancellationToken = default)
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException("NetworkGuard named pipes require Windows.");
        }

        await using var pipe = new NamedPipeClientStream(
            ".",
            NetworkGuardProtocol.PipeName,
            PipeDirection.InOut,
            PipeOptions.Asynchronous | PipeOptions.WriteThrough,
            TokenImpersonationLevel.Identification);
        using var timeout = new CancellationTokenSource(_connectTimeout);
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(
            cancellationToken,
            timeout.Token);
        try
        {
            await pipe.ConnectAsync(linked.Token).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new TimeoutException("NetworkGuard service connection timed out.");
        }

        _identityVerifier.Verify(pipe);
        await NetworkGuardProtocolCodec.WriteAsync(pipe, request, cancellationToken)
            .ConfigureAwait(false);
        return await NetworkGuardProtocolCodec.ReadAsync<NetworkGuardResponse>(pipe, cancellationToken)
            .ConfigureAwait(false);
    }
}
