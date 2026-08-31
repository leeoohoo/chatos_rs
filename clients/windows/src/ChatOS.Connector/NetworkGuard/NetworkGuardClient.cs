using ChatOS.NetworkGuard.Contracts;

namespace ChatOS.Connector.NetworkGuard;

public enum NetworkGuardReadinessState
{
    Ready,
    UnsupportedPlatform,
    ServiceUnavailable,
    ProtocolMismatch,
    DriverUnavailable,
    SelfTestFailed,
    InvalidResponse,
}

public sealed record NetworkGuardReadiness(
    NetworkGuardReadinessState State,
    string? ServiceVersion = null,
    string? DriverVersion = null,
    int ActiveLeaseCount = 0)
{
    public bool IsReady => State is NetworkGuardReadinessState.Ready;
}

public sealed record NetworkGuardLease(
    string LeaseId,
    DateTimeOffset ExpiresAt,
    string PolicyRevision,
    string AppContainerSid,
    int ProcessId);

internal interface INetworkGuardTransport
{
    Task<NetworkGuardResponse> SendAsync(
        NetworkGuardRequest request,
        CancellationToken cancellationToken = default);
}

public interface IControlledNetworkGuardClient
{
    Task<NetworkGuardReadiness> CheckReadinessAsync(CancellationToken cancellationToken = default);

    Task<NetworkGuardLease> AcquireLeaseAsync(
        ControlledNetworkPolicyEnvelope policy,
        string appContainerSid,
        int processId,
        CancellationToken cancellationToken = default);

    Task<NetworkGuardLease> RenewLeaseAsync(
        NetworkGuardLease lease,
        CancellationToken cancellationToken = default);

    Task ReleaseLeaseAsync(
        NetworkGuardLease lease,
        CancellationToken cancellationToken = default);
}

internal sealed class ControlledNetworkGuardClient(
    INetworkGuardTransport transport,
    TimeProvider? timeProvider = null,
    bool requireWindowsPlatform = true) : IControlledNetworkGuardClient
{
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;
    private readonly bool _requireWindowsPlatform = requireWindowsPlatform;

    public async Task<NetworkGuardReadiness> CheckReadinessAsync(
        CancellationToken cancellationToken = default)
    {
        if (_requireWindowsPlatform && !OperatingSystem.IsWindows())
        {
            return new NetworkGuardReadiness(NetworkGuardReadinessState.UnsupportedPlatform);
        }

        try
        {
            var response = await SendAsync(
                NetworkGuardOperation.Health,
                cancellationToken: cancellationToken).ConfigureAwait(false);
            if (response.ProtocolMajor != NetworkGuardProtocol.MajorVersion)
            {
                return new NetworkGuardReadiness(
                    NetworkGuardReadinessState.ProtocolMismatch,
                    response.ServiceVersion,
                    response.DriverVersion,
                    response.ActiveLeaseCount);
            }
            if (!response.Success || !response.DriverReady)
            {
                return new NetworkGuardReadiness(
                    NetworkGuardReadinessState.DriverUnavailable,
                    response.ServiceVersion,
                    response.DriverVersion,
                    response.ActiveLeaseCount);
            }
            if (!response.SelfTestPassed)
            {
                return new NetworkGuardReadiness(
                    NetworkGuardReadinessState.SelfTestFailed,
                    response.ServiceVersion,
                    response.DriverVersion,
                    response.ActiveLeaseCount);
            }
            return new NetworkGuardReadiness(
                NetworkGuardReadinessState.Ready,
                response.ServiceVersion,
                response.DriverVersion,
                response.ActiveLeaseCount);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception exception) when (exception is IOException or TimeoutException or UnauthorizedAccessException)
        {
            return new NetworkGuardReadiness(NetworkGuardReadinessState.ServiceUnavailable);
        }
        catch
        {
            return new NetworkGuardReadiness(NetworkGuardReadinessState.InvalidResponse);
        }
    }

    public async Task<NetworkGuardLease> AcquireLeaseAsync(
        ControlledNetworkPolicyEnvelope policy,
        string appContainerSid,
        int processId,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(policy);
        ValidateSidAndProcess(appContainerSid, processId);
        var response = await SendAsync(
            NetworkGuardOperation.AcquireLease,
            policy,
            appContainerSid,
            processId,
            cancellationToken: cancellationToken).ConfigureAwait(false);
        return ValidateLeaseResponse(response, policy.PolicyRevision, appContainerSid, processId);
    }

    public async Task<NetworkGuardLease> RenewLeaseAsync(
        NetworkGuardLease lease,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(lease);
        ValidateSidAndProcess(lease.AppContainerSid, lease.ProcessId);
        var response = await SendAsync(
            NetworkGuardOperation.RenewLease,
            appContainerSid: lease.AppContainerSid,
            processId: lease.ProcessId,
            leaseId: lease.LeaseId,
            cancellationToken: cancellationToken).ConfigureAwait(false);
        return ValidateLeaseResponse(
            response,
            lease.PolicyRevision,
            lease.AppContainerSid,
            lease.ProcessId);
    }

    public async Task ReleaseLeaseAsync(
        NetworkGuardLease lease,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(lease);
        var response = await SendAsync(
            NetworkGuardOperation.ReleaseLease,
            appContainerSid: lease.AppContainerSid,
            processId: lease.ProcessId,
            leaseId: lease.LeaseId,
            cancellationToken: cancellationToken).ConfigureAwait(false);
        if (!response.Success)
        {
            throw new InvalidOperationException("NetworkGuard lease release failed.");
        }
    }

    private async Task<NetworkGuardResponse> SendAsync(
        NetworkGuardOperation operation,
        ControlledNetworkPolicyEnvelope? policy = null,
        string? appContainerSid = null,
        int? processId = null,
        string? leaseId = null,
        CancellationToken cancellationToken = default)
    {
        var correlationId = Guid.NewGuid().ToString("N");
        var response = await transport.SendAsync(new NetworkGuardRequest(
            NetworkGuardProtocol.MajorVersion,
            NetworkGuardProtocol.MinorVersion,
            correlationId,
            operation,
            policy,
            appContainerSid,
            processId,
            leaseId), cancellationToken).ConfigureAwait(false);
        if (!string.Equals(response.CorrelationId, correlationId, StringComparison.Ordinal))
        {
            throw new InvalidDataException("NetworkGuard response correlation is invalid.");
        }
        return response;
    }

    private NetworkGuardLease ValidateLeaseResponse(
        NetworkGuardResponse response,
        string policyRevision,
        string appContainerSid,
        int processId)
    {
        if (response.ProtocolMajor != NetworkGuardProtocol.MajorVersion || !response.Success ||
            string.IsNullOrWhiteSpace(response.LeaseId) || response.LeaseId.Length > 128 ||
            response.LeaseId.Any(char.IsControl) || response.LeaseExpiresAt is not { } expiresAt ||
            expiresAt <= _timeProvider.GetUtcNow() || expiresAt > _timeProvider.GetUtcNow().AddHours(24))
        {
            throw new InvalidDataException("NetworkGuard returned an invalid lease.");
        }
        return new NetworkGuardLease(
            response.LeaseId,
            expiresAt,
            policyRevision,
            appContainerSid,
            processId);
    }

    private static void ValidateSidAndProcess(string appContainerSid, int processId)
    {
        if (string.IsNullOrWhiteSpace(appContainerSid) || appContainerSid.Length > 256 ||
            appContainerSid.Any(char.IsControl) || !appContainerSid.StartsWith("S-1-15-2-", StringComparison.Ordinal) ||
            processId <= 0)
        {
            throw new ArgumentException("NetworkGuard AppContainer identity is invalid.");
        }
    }
}
