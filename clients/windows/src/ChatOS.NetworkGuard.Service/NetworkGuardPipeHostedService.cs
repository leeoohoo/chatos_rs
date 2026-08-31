using System.IO.Pipes;
using System.Security.AccessControl;
using System.Security.Principal;
using ChatOS.NetworkGuard.Contracts;

namespace ChatOS.NetworkGuard.Service;

internal sealed class NetworkGuardPipeHostedService(
    NetworkGuardRequestHandler handler,
    ILogger<NetworkGuardPipeHostedService> logger) : BackgroundService
{
    private readonly SemaphoreSlim _connections = new(16, 16);

    protected override async Task ExecuteAsync(CancellationToken stoppingToken)
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException("NetworkGuard service requires Windows.");
        }

        while (!stoppingToken.IsCancellationRequested)
        {
            var pipe = CreateServerPipe();
            try
            {
                await pipe.WaitForConnectionAsync(stoppingToken).ConfigureAwait(false);
                await _connections.WaitAsync(stoppingToken).ConfigureAwait(false);
                _ = HandleConnectionAsync(pipe, stoppingToken);
                pipe = null;
            }
            catch
            {
                pipe?.Dispose();
                if (stoppingToken.IsCancellationRequested) break;
                throw;
            }
        }
    }

    private async Task HandleConnectionAsync(
        NamedPipeServerStream pipe,
        CancellationToken stoppingToken)
    {
        await using (pipe)
        {
            try
            {
                var caller = WindowsProcessIdentityReader.ReadPipeClient(pipe);
                var request = await NetworkGuardProtocolCodec.ReadAsync<NetworkGuardRequest>(
                    pipe,
                    stoppingToken).ConfigureAwait(false);
                var response = await handler.HandleAsync(request, caller, stoppingToken)
                    .ConfigureAwait(false);
                await NetworkGuardProtocolCodec.WriteAsync(pipe, response, stoppingToken)
                    .ConfigureAwait(false);
            }
            catch (OperationCanceledException) when (stoppingToken.IsCancellationRequested)
            {
            }
            catch (Exception exception)
            {
                logger.LogWarning(
                    "NetworkGuard pipe request failed. Failure type: {FailureType}.",
                    exception.GetType().Name);
            }
            finally
            {
                _connections.Release();
            }
        }
    }

    private static NamedPipeServerStream CreateServerPipe()
    {
        var security = new PipeSecurity();
        security.SetAccessRuleProtection(isProtected: true, preserveInheritance: false);
        security.AddAccessRule(new PipeAccessRule(
            new SecurityIdentifier(WellKnownSidType.NetworkSid, null),
            PipeAccessRights.ReadWrite,
            AccessControlType.Deny));
        security.AddAccessRule(new PipeAccessRule(
            new SecurityIdentifier(WellKnownSidType.LocalSystemSid, null),
            PipeAccessRights.FullControl,
            AccessControlType.Allow));
        security.AddAccessRule(new PipeAccessRule(
            new SecurityIdentifier(WellKnownSidType.AuthenticatedUserSid, null),
            PipeAccessRights.ReadWrite,
            AccessControlType.Allow));
        return NamedPipeServerStreamAcl.Create(
            NetworkGuardProtocol.PipeName,
            PipeDirection.InOut,
            16,
            PipeTransmissionMode.Byte,
            PipeOptions.Asynchronous | PipeOptions.WriteThrough,
            64 * 1024,
            64 * 1024,
            security,
            HandleInheritability.None,
            (PipeAccessRights)0);
    }
}
