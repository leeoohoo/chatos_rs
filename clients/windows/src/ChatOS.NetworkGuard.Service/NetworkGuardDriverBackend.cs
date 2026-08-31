using System.Collections.Concurrent;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Security.Principal;
using ChatOS.NetworkGuard.Contracts;
using Microsoft.Extensions.Options;
using Microsoft.Win32.SafeHandles;

namespace ChatOS.NetworkGuard.Service;

public sealed record NetworkGuardDriverHealth(
    bool DriverReady,
    bool SelfTestPassed,
    string? DriverVersion = null,
    int ActiveLeaseCount = 0);

public sealed record NetworkGuardDriverLease(string LeaseId, DateTimeOffset ExpiresAt);

public interface INetworkGuardDriverBackend
{
    Task<NetworkGuardDriverHealth> CheckHealthAsync(CancellationToken cancellationToken = default);

    Task<NetworkGuardDriverLease> AcquireAsync(
        ControlledNetworkPolicy policy,
        string appContainerSid,
        int processId,
        string callerWindowsUserSid,
        CancellationToken cancellationToken = default);

    Task<NetworkGuardDriverLease> RenewAsync(
        string leaseId,
        string appContainerSid,
        int processId,
        string callerWindowsUserSid,
        CancellationToken cancellationToken = default);

    Task ReleaseAsync(
        string leaseId,
        string appContainerSid,
        int processId,
        string callerWindowsUserSid,
        CancellationToken cancellationToken = default);
}

internal sealed record ActiveNetworkGuardLease(
    Guid LeaseId,
    ControlledNetworkPolicy Policy,
    string AppContainerSid,
    int ProcessId,
    string CallerWindowsUserSid,
    DateTimeOffset ExpiresAt);

internal interface INetworkGuardLeasePolicyStore
{
    bool TryGetActive(Guid leaseId, out ActiveNetworkGuardLease? lease);
}

internal interface INetworkGuardNativeController
{
    Task<NetworkGuardDriverHealth> CheckHealthAsync(CancellationToken cancellationToken = default);

    Task ResetAsync(CancellationToken cancellationToken = default);

    Task ApplyLeaseAsync(
        ActiveNetworkGuardLease lease,
        int httpBrokerPort,
        int httpsBrokerPort,
        CancellationToken cancellationToken = default);

    Task RemoveLeaseAsync(Guid leaseId, CancellationToken cancellationToken = default);
}

internal sealed class NetworkGuardDriverBackend(
    INetworkGuardNativeController nativeController,
    NetworkGuardBrokerState brokerState,
    IOptions<NetworkGuardServiceOptions> options,
    TimeProvider? timeProvider = null) : INetworkGuardDriverBackend, INetworkGuardLeasePolicyStore
{
    private readonly ConcurrentDictionary<Guid, ActiveNetworkGuardLease> _leases = new();
    private readonly TimeProvider _timeProvider = timeProvider ?? TimeProvider.System;

    public async Task<NetworkGuardDriverHealth> CheckHealthAsync(
        CancellationToken cancellationToken = default)
    {
        var health = await nativeController.CheckHealthAsync(cancellationToken).ConfigureAwait(false);
        return health with
        {
            SelfTestPassed = health.DriverReady && health.SelfTestPassed && brokerState.IsReady,
        };
    }

    public async Task<NetworkGuardDriverLease> AcquireAsync(
        ControlledNetworkPolicy policy,
        string appContainerSid,
        int processId,
        string callerWindowsUserSid,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(policy);
        var now = _timeProvider.GetUtcNow();
        var expiresAt = Minimum(policy.ExpiresAt, now + options.Value.LeaseDuration);
        if (expiresAt <= now)
        {
            throw new InvalidOperationException("Controlled network policy is expired.");
        }

        ActiveNetworkGuardLease lease;
        do
        {
            lease = new ActiveNetworkGuardLease(
                NewLeaseId(),
                policy,
                appContainerSid,
                processId,
                callerWindowsUserSid,
                expiresAt);
        }
        while (!_leases.TryAdd(lease.LeaseId, lease));

        try
        {
            await nativeController.ApplyLeaseAsync(
                lease,
                options.Value.HttpBrokerPort,
                options.Value.HttpsBrokerPort,
                cancellationToken).ConfigureAwait(false);
            return new NetworkGuardDriverLease(lease.LeaseId.ToString("N"), lease.ExpiresAt);
        }
        catch
        {
            _leases.TryRemove(lease.LeaseId, out _);
            try
            {
                await nativeController.RemoveLeaseAsync(lease.LeaseId, CancellationToken.None)
                    .ConfigureAwait(false);
            }
            catch
            {
            }
            throw;
        }
    }

    public async Task<NetworkGuardDriverLease> RenewAsync(
        string leaseId,
        string appContainerSid,
        int processId,
        string callerWindowsUserSid,
        CancellationToken cancellationToken = default)
    {
        var id = ParseLeaseId(leaseId);
        if (!_leases.TryGetValue(id, out var current) ||
            !IdentityMatches(current, appContainerSid, processId, callerWindowsUserSid))
        {
            throw new InvalidOperationException("NetworkGuard lease identity does not match.");
        }

        var now = _timeProvider.GetUtcNow();
        if (current.ExpiresAt <= now || current.Policy.ExpiresAt <= now)
        {
            await RemoveExpiredAsync(id).ConfigureAwait(false);
            throw new InvalidOperationException("NetworkGuard lease is expired.");
        }

        var renewed = current with
        {
            ExpiresAt = Minimum(current.Policy.ExpiresAt, now + options.Value.LeaseDuration),
        };
        if (!_leases.TryUpdate(id, renewed, current))
        {
            throw new InvalidOperationException("NetworkGuard lease changed concurrently.");
        }
        try
        {
            await nativeController.ApplyLeaseAsync(
                renewed,
                options.Value.HttpBrokerPort,
                options.Value.HttpsBrokerPort,
                cancellationToken).ConfigureAwait(false);
            return new NetworkGuardDriverLease(id.ToString("N"), renewed.ExpiresAt);
        }
        catch
        {
            _leases.TryRemove(id, out _);
            try
            {
                await nativeController.RemoveLeaseAsync(id, CancellationToken.None).ConfigureAwait(false);
            }
            catch
            {
            }
            throw;
        }
    }

    public async Task ReleaseAsync(
        string leaseId,
        string appContainerSid,
        int processId,
        string callerWindowsUserSid,
        CancellationToken cancellationToken = default)
    {
        var id = ParseLeaseId(leaseId);
        if (!_leases.TryGetValue(id, out var current) ||
            !IdentityMatches(current, appContainerSid, processId, callerWindowsUserSid))
        {
            throw new InvalidOperationException("NetworkGuard lease identity does not match.");
        }
        if (!_leases.TryRemove(new KeyValuePair<Guid, ActiveNetworkGuardLease>(id, current)))
        {
            throw new InvalidOperationException("NetworkGuard lease changed concurrently.");
        }
        await nativeController.RemoveLeaseAsync(id, cancellationToken).ConfigureAwait(false);
    }

    public bool TryGetActive(Guid leaseId, out ActiveNetworkGuardLease? lease)
    {
        var now = _timeProvider.GetUtcNow();
        if (_leases.TryGetValue(leaseId, out var value) &&
            value.ExpiresAt > now &&
            value.Policy.ExpiresAt > now)
        {
            lease = value;
            return true;
        }
        lease = null;
        if (value is not null) _ = RemoveExpiredAsync(leaseId);
        return false;
    }

    private async Task RemoveExpiredAsync(Guid leaseId)
    {
        _leases.TryRemove(leaseId, out _);
        try
        {
            await nativeController.RemoveLeaseAsync(leaseId, CancellationToken.None).ConfigureAwait(false);
        }
        catch
        {
        }
    }

    private static bool IdentityMatches(
        ActiveNetworkGuardLease lease,
        string appContainerSid,
        int processId,
        string callerWindowsUserSid) =>
        lease.ProcessId == processId &&
        string.Equals(lease.AppContainerSid, appContainerSid, StringComparison.Ordinal) &&
        string.Equals(lease.CallerWindowsUserSid, callerWindowsUserSid, StringComparison.Ordinal);

    private static Guid ParseLeaseId(string value) =>
        Guid.TryParseExact(value, "N", out var result)
            ? result
            : throw new ArgumentException("Lease id is invalid.", nameof(value));

    private static DateTimeOffset Minimum(DateTimeOffset left, DateTimeOffset right) =>
        left <= right ? left : right;

    private static Guid NewLeaseId()
    {
        Span<byte> bytes = stackalloc byte[16];
        RandomNumberGenerator.Fill(bytes);
        return new Guid(bytes);
    }
}

internal sealed class WindowsNetworkGuardNativeController : INetworkGuardNativeController
{
    private const string DevicePath = @"\\.\ChatOSNetworkGuard";
    private const uint IoctlHealth = 0x0012E000;
    private const uint IoctlApplyLease = 0x0012E004;
    private const uint IoctlRemoveLease = 0x0012E008;
    private const uint IoctlResetLeases = 0x0012E00C;
    private const uint HealthMagic = 0x31474E43;
    private const uint ApplyMagic = 0x32474E43;
    private const uint ResetMagic = 0x33474E43;
    private const int HealthResponseBytes = 28;

    public Task<NetworkGuardDriverHealth> CheckHealthAsync(
        CancellationToken cancellationToken = default)
    {
        if (!OperatingSystem.IsWindows())
        {
            return Task.FromResult(new NetworkGuardDriverHealth(false, false));
        }
        try
        {
            using var handle = OpenDevice();
            var response = DeviceControl(handle, IoctlHealth, [], HealthResponseBytes);
            if (response.Length < HealthResponseBytes ||
                BitConverter.ToUInt32(response, 0) != HealthMagic)
            {
                return Task.FromResult(new NetworkGuardDriverHealth(false, false));
            }
            var protocolMajor = BitConverter.ToUInt16(response, 4);
            var flags = BitConverter.ToUInt32(response, 8);
            var version = $"{BitConverter.ToUInt32(response, 12)}.{BitConverter.ToUInt32(response, 16)}.{BitConverter.ToUInt32(response, 20)}";
            return Task.FromResult(new NetworkGuardDriverHealth(
                protocolMajor == NetworkGuardProtocol.MajorVersion && (flags & 1) != 0,
                (flags & 2) != 0,
                version,
                checked((int)BitConverter.ToUInt32(response, 24))));
        }
        catch (Exception exception) when (
            exception is IOException or UnauthorizedAccessException or Win32Exception)
        {
            return Task.FromResult(new NetworkGuardDriverHealth(false, false));
        }
    }

    public Task ApplyLeaseAsync(
        ActiveNetworkGuardLease lease,
        int httpBrokerPort,
        int httpsBrokerPort,
        CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (!OperatingSystem.IsWindows()) throw new NetworkGuardDriverUnavailableException();
        var sid = new SecurityIdentifier(lease.AppContainerSid);
        var sidBytes = new byte[sid.BinaryLength];
        sid.GetBinaryForm(sidBytes, 0);
        if (sidBytes.Length > byte.MaxValue) throw new ArgumentException("AppContainer SID is too large.");

        var payload = new byte[48 + sidBytes.Length];
        BitConverter.GetBytes(ApplyMagic).CopyTo(payload, 0);
        BitConverter.GetBytes((ushort)NetworkGuardProtocol.MajorVersion).CopyTo(payload, 4);
        lease.LeaseId.TryWriteBytes(payload.AsSpan(8, 16));
        BitConverter.GetBytes(lease.ExpiresAt.ToUnixTimeSeconds()).CopyTo(payload, 24);
        BitConverter.GetBytes((uint)lease.ProcessId).CopyTo(payload, 32);
        BitConverter.GetBytes((ushort)httpBrokerPort).CopyTo(payload, 36);
        BitConverter.GetBytes((ushort)httpsBrokerPort).CopyTo(payload, 38);
        BitConverter.GetBytes((uint)Environment.ProcessId).CopyTo(payload, 40);
        payload[44] = (byte)sidBytes.Length;
        sidBytes.CopyTo(payload, 48);

        using var handle = OpenDevice();
        DeviceControl(handle, IoctlApplyLease, payload, 0);
        return Task.CompletedTask;
    }

    public Task ResetAsync(CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (!OperatingSystem.IsWindows()) throw new NetworkGuardDriverUnavailableException();
        var payload = new byte[8];
        BitConverter.GetBytes(ResetMagic).CopyTo(payload, 0);
        BitConverter.GetBytes((ushort)NetworkGuardProtocol.MajorVersion).CopyTo(payload, 4);
        using var handle = OpenDevice();
        DeviceControl(handle, IoctlResetLeases, payload, 0);
        return Task.CompletedTask;
    }

    public Task RemoveLeaseAsync(Guid leaseId, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        if (!OperatingSystem.IsWindows()) throw new NetworkGuardDriverUnavailableException();
        var payload = new byte[24];
        BitConverter.GetBytes(ApplyMagic).CopyTo(payload, 0);
        BitConverter.GetBytes((ushort)NetworkGuardProtocol.MajorVersion).CopyTo(payload, 4);
        leaseId.TryWriteBytes(payload.AsSpan(8, 16));
        using var handle = OpenDevice();
        DeviceControl(handle, IoctlRemoveLease, payload, 0);
        return Task.CompletedTask;
    }

    private static SafeFileHandle OpenDevice()
    {
        var handle = NativeMethods.CreateFile(
            DevicePath,
            0xC0000000,
            0x00000001 | 0x00000002,
            IntPtr.Zero,
            3,
            0x00000080,
            IntPtr.Zero);
        if (handle.IsInvalid)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        return handle;
    }

    private static byte[] DeviceControl(
        SafeFileHandle handle,
        uint ioctl,
        byte[] input,
        int outputLength)
    {
        var output = new byte[outputLength];
        if (!NativeMethods.DeviceIoControl(
                handle,
                ioctl,
                input,
                input.Length,
                output,
                output.Length,
                out var returned,
                IntPtr.Zero))
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        if (returned == output.Length) return output;
        return output.AsSpan(0, checked((int)returned)).ToArray();
    }

    private static class NativeMethods
    {
        [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
        internal static extern SafeFileHandle CreateFile(
            string fileName,
            uint desiredAccess,
            uint shareMode,
            IntPtr securityAttributes,
            uint creationDisposition,
            uint flagsAndAttributes,
            IntPtr templateFile);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool DeviceIoControl(
            SafeFileHandle device,
            uint controlCode,
            byte[] input,
            int inputSize,
            byte[] output,
            int outputSize,
            out uint bytesReturned,
            IntPtr overlapped);
    }
}

internal sealed class NetworkGuardDriverUnavailableException : Exception
{
}
