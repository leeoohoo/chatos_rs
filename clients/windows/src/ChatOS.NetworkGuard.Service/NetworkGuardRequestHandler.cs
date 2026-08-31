using ChatOS.NetworkGuard.Contracts;
using Microsoft.Extensions.Options;

namespace ChatOS.NetworkGuard.Service;

public sealed class NetworkGuardRequestHandler(
    INetworkGuardDriverBackend driver,
    IOptions<NetworkGuardServiceOptions> options,
    INetworkGuardProcessIdentityVerifier processIdentityVerifier,
    TimeProvider? timeProvider = null)
{
    private const string ServiceVersion = "1.0.0";
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;

    public async Task<NetworkGuardResponse> HandleAsync(
        NetworkGuardRequest request,
        NetworkGuardCallerIdentity? caller,
        CancellationToken cancellationToken = default)
    {
        if (request.ProtocolMajor != NetworkGuardProtocol.MajorVersion ||
            request.ProtocolMinor is < 0 or > NetworkGuardProtocol.MinorVersion ||
            string.IsNullOrWhiteSpace(request.CorrelationId) ||
            request.CorrelationId.Length > 128 || request.CorrelationId.Any(char.IsControl))
        {
            return Failure(request, "protocol_mismatch");
        }

        NetworkGuardDriverHealth health;
        try
        {
            health = await driver.CheckHealthAsync(cancellationToken).ConfigureAwait(false);
        }
        catch
        {
            return Failure(request, "driver_unavailable");
        }
        if (request.Operation is NetworkGuardOperation.Health)
        {
            return new NetworkGuardResponse(
                NetworkGuardProtocol.MajorVersion,
                NetworkGuardProtocol.MinorVersion,
                request.CorrelationId,
                Success: health.DriverReady && health.SelfTestPassed,
                FailureCode: health.DriverReady && health.SelfTestPassed
                    ? null
                    : health.DriverReady ? "self_test_failed" : "driver_unavailable",
                ServiceVersion,
                health.DriverVersion,
                health.DriverReady,
                health.SelfTestPassed,
                ActiveLeaseCount: health.ActiveLeaseCount);
        }
        if (!health.DriverReady || !health.SelfTestPassed)
        {
            return Failure(request, "driver_unavailable", health);
        }

        try
        {
            return request.Operation switch
            {
                NetworkGuardOperation.AcquireLease => await AcquireAsync(request, caller, health, cancellationToken)
                    .ConfigureAwait(false),
                NetworkGuardOperation.RenewLease => await RenewAsync(request, caller, health, cancellationToken)
                    .ConfigureAwait(false),
                NetworkGuardOperation.ReleaseLease => await ReleaseAsync(request, caller, health, cancellationToken)
                    .ConfigureAwait(false),
                _ => Failure(request, "unsupported_operation", health),
            };
        }
        catch (NetworkGuardDriverUnavailableException)
        {
            return Failure(request, "driver_unavailable", health);
        }
        catch (ArgumentException)
        {
            return Failure(request, "invalid_request", health);
        }
        catch (InvalidOperationException)
        {
            return Failure(request, "policy_rejected", health);
        }
        catch
        {
            return Failure(request, "operation_failed", health);
        }
    }

    private async Task<NetworkGuardResponse> AcquireAsync(
        NetworkGuardRequest request,
        NetworkGuardCallerIdentity? caller,
        NetworkGuardDriverHealth health,
        CancellationToken cancellationToken)
    {
        if (request.Policy is null) throw new ArgumentException("Policy is required.");
        ValidateIdentity(request);
        var validator = new ControlledNetworkPolicyValidator(
            options.Value.TrustedPolicyPublicKeys,
            _timeProvider);
        var policy = validator.Validate(request.Policy);
        caller = ValidateCaller(caller, policy, request);
        var lease = await driver.AcquireAsync(
            policy,
            request.AppContainerSid!,
            request.ProcessId!.Value,
            caller.WindowsUserSid,
            cancellationToken).ConfigureAwait(false);
        return Success(request, health, lease);
    }

    private async Task<NetworkGuardResponse> RenewAsync(
        NetworkGuardRequest request,
        NetworkGuardCallerIdentity? caller,
        NetworkGuardDriverHealth health,
        CancellationToken cancellationToken)
    {
        ValidateIdentity(request);
        caller = ValidateCaller(caller, null, request);
        var leaseId = RequiredLeaseId(request.LeaseId);
        var lease = await driver.RenewAsync(
            leaseId,
            request.AppContainerSid!,
            request.ProcessId!.Value,
            caller.WindowsUserSid,
            cancellationToken).ConfigureAwait(false);
        return Success(request, health, lease);
    }

    private async Task<NetworkGuardResponse> ReleaseAsync(
        NetworkGuardRequest request,
        NetworkGuardCallerIdentity? caller,
        NetworkGuardDriverHealth health,
        CancellationToken cancellationToken)
    {
        ValidateIdentity(request);
        caller = ValidateCaller(caller, null, request);
        await driver.ReleaseAsync(
            RequiredLeaseId(request.LeaseId),
            request.AppContainerSid!,
            request.ProcessId!.Value,
            caller.WindowsUserSid,
            cancellationToken).ConfigureAwait(false);
        return new NetworkGuardResponse(
            NetworkGuardProtocol.MajorVersion,
            NetworkGuardProtocol.MinorVersion,
            request.CorrelationId,
            Success: true,
            ServiceVersion: ServiceVersion,
            DriverVersion: health.DriverVersion,
            DriverReady: health.DriverReady,
            SelfTestPassed: health.SelfTestPassed,
            ActiveLeaseCount: health.ActiveLeaseCount);
    }

    private static NetworkGuardResponse Success(
        NetworkGuardRequest request,
        NetworkGuardDriverHealth health,
        NetworkGuardDriverLease lease) => new(
            NetworkGuardProtocol.MajorVersion,
            NetworkGuardProtocol.MinorVersion,
            request.CorrelationId,
            Success: true,
            ServiceVersion: ServiceVersion,
            DriverVersion: health.DriverVersion,
            DriverReady: health.DriverReady,
            SelfTestPassed: health.SelfTestPassed,
            LeaseId: lease.LeaseId,
            LeaseExpiresAt: lease.ExpiresAt,
            ActiveLeaseCount: health.ActiveLeaseCount);

    private static NetworkGuardResponse Failure(
        NetworkGuardRequest request,
        string code,
        NetworkGuardDriverHealth? health = null) => new(
            NetworkGuardProtocol.MajorVersion,
            NetworkGuardProtocol.MinorVersion,
            request.CorrelationId ?? string.Empty,
            Success: false,
            FailureCode: code,
            ServiceVersion: ServiceVersion,
            DriverVersion: health?.DriverVersion,
            DriverReady: health?.DriverReady == true,
            SelfTestPassed: health?.SelfTestPassed == true,
            ActiveLeaseCount: health?.ActiveLeaseCount ?? 0);

    private static void ValidateIdentity(NetworkGuardRequest request)
    {
        if (string.IsNullOrWhiteSpace(request.AppContainerSid) ||
            !request.AppContainerSid.StartsWith("S-1-15-2-", StringComparison.Ordinal) ||
            request.AppContainerSid.Length > 256 || request.AppContainerSid.Any(char.IsControl) ||
            request.ProcessId is null or <= 0)
        {
            throw new ArgumentException("AppContainer identity is invalid.");
        }
    }

    private NetworkGuardCallerIdentity ValidateCaller(
        NetworkGuardCallerIdentity? caller,
        ControlledNetworkPolicy? policy,
        NetworkGuardRequest request)
    {
        if (caller is null)
        {
            throw new InvalidOperationException("NetworkGuard caller identity is missing.");
        }
        if (policy is not null &&
            !string.Equals(policy.WindowsUserSid, caller.WindowsUserSid, StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Controlled network policy is not issued for this Windows user.");
        }
        if (!processIdentityVerifier.Verify(
                request.ProcessId!.Value,
                caller.WindowsUserSid,
                request.AppContainerSid!))
        {
            throw new ArgumentException("Target process identity does not match the request.");
        }
        return caller;
    }

    private static string RequiredLeaseId(string? value)
    {
        var result = value?.Trim();
        if (string.IsNullOrWhiteSpace(result) || result.Length > 128 || result.Any(char.IsControl))
        {
            throw new ArgumentException("Lease id is invalid.");
        }
        return result;
    }
}
