using System.ComponentModel;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using System.Security.Principal;
using Microsoft.Win32.SafeHandles;

namespace ChatOS.NetworkGuard.Service;

public sealed record NetworkGuardCallerIdentity(string WindowsUserSid, int ClientProcessId);

public interface INetworkGuardProcessIdentityVerifier
{
    bool Verify(int processId, string expectedWindowsUserSid, string expectedAppContainerSid);
}

internal sealed class WindowsNetworkGuardProcessIdentityVerifier : INetworkGuardProcessIdentityVerifier
{
    public bool Verify(int processId, string expectedWindowsUserSid, string expectedAppContainerSid)
    {
        if (!OperatingSystem.IsWindows()) return false;
        try
        {
            var identity = WindowsProcessIdentityReader.Read(processId);
            return string.Equals(
                    identity.WindowsUserSid,
                    expectedWindowsUserSid,
                    StringComparison.Ordinal) &&
                string.Equals(
                    identity.AppContainerSid,
                    expectedAppContainerSid,
                    StringComparison.Ordinal);
        }
        catch (Exception exception) when (
            exception is Win32Exception or UnauthorizedAccessException or ArgumentException)
        {
            return false;
        }
    }
}

internal sealed record WindowsProcessIdentity(string WindowsUserSid, string? AppContainerSid);

internal static class WindowsProcessIdentityReader
{
    private const uint ProcessQueryLimitedInformation = 0x1000;
    private const uint TokenQuery = 0x0008;
    private const int TokenAppContainerSid = 31;

    public static NetworkGuardCallerIdentity ReadPipeClient(NamedPipeServerStream pipe)
    {
        if (!OperatingSystem.IsWindows()) throw new PlatformNotSupportedException();
        if (!NativeMethods.GetNamedPipeClientProcessId(pipe.SafePipeHandle, out var processId))
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        var identity = Read(checked((int)processId));
        return new NetworkGuardCallerIdentity(identity.WindowsUserSid, checked((int)processId));
    }

    public static WindowsProcessIdentity Read(int processId)
    {
        using var process = NativeMethods.OpenProcess(
            ProcessQueryLimitedInformation,
            false,
            checked((uint)processId));
        if (process.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error());
        if (!NativeMethods.OpenProcessToken(process, TokenQuery, out var token))
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        using (token)
        using (var identity = new WindowsIdentity(token.DangerousGetHandle()))
        {
            var userSid = identity.User?.Value
                ?? throw new UnauthorizedAccessException("Process has no Windows user SID.");
            return new WindowsProcessIdentity(userSid, ReadAppContainerSid(token));
        }
    }

    private static string? ReadAppContainerSid(SafeAccessTokenHandle token)
    {
        NativeMethods.GetTokenInformation(
            token,
            TokenAppContainerSid,
            IntPtr.Zero,
            0,
            out var required);
        if (required == 0) return null;
        var buffer = Marshal.AllocHGlobal(checked((int)required));
        try
        {
            if (!NativeMethods.GetTokenInformation(
                    token,
                    TokenAppContainerSid,
                    buffer,
                    required,
                    out _))
            {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            var sidPointer = Marshal.ReadIntPtr(buffer);
            return sidPointer == IntPtr.Zero ? null : new SecurityIdentifier(sidPointer).Value;
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    private static class NativeMethods
    {
        [DllImport("kernel32.dll", SetLastError = true)]
        internal static extern SafeProcessHandle OpenProcess(
            uint desiredAccess,
            [MarshalAs(UnmanagedType.Bool)] bool inheritHandle,
            uint processId);

        [DllImport("advapi32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool OpenProcessToken(
            SafeProcessHandle processHandle,
            uint desiredAccess,
            out SafeAccessTokenHandle tokenHandle);

        [DllImport("advapi32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool GetTokenInformation(
            SafeAccessTokenHandle tokenHandle,
            int tokenInformationClass,
            IntPtr tokenInformation,
            uint tokenInformationLength,
            out uint returnLength);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        internal static extern bool GetNamedPipeClientProcessId(
            SafePipeHandle pipe,
            out uint clientProcessId);
    }
}
