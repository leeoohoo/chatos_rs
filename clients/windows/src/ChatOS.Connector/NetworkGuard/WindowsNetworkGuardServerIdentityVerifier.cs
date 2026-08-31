using System.ComponentModel;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using ChatOS.Connector.Terminal;
using Microsoft.Win32.SafeHandles;

namespace ChatOS.Connector.NetworkGuard;

internal interface INetworkGuardServerIdentityVerifier
{
    void Verify(NamedPipeClientStream pipe);
}

internal sealed class WindowsNetworkGuardServerIdentityVerifier : INetworkGuardServerIdentityVerifier
{
    private const uint ProcessQueryLimitedInformation = 0x1000;
    private const uint TokenQuery = 0x0008;
    private const int TokenUserInformationClass = 1;
    private const int ErrorInsufficientBuffer = 122;
    private const string LocalSystemSid = "S-1-5-18";

    public void Verify(NamedPipeClientStream pipe)
    {
        ArgumentNullException.ThrowIfNull(pipe);
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException("NetworkGuard server identity verification requires Windows.");
        }
        if (!pipe.IsConnected || !GetNamedPipeServerProcessId(pipe.SafePipeHandle, out var processId) ||
            processId == 0)
        {
            throw new UnauthorizedAccessException("NetworkGuard server process identity is unavailable.");
        }

        using var process = OpenProcess(ProcessQueryLimitedInformation, inheritHandle: false, processId);
        if (process.IsInvalid)
        {
            throw new UnauthorizedAccessException("NetworkGuard server process cannot be inspected.");
        }
        if (!OpenProcessToken(process, TokenQuery, out var token))
        {
            throw new UnauthorizedAccessException("NetworkGuard server token cannot be inspected.");
        }
        using (token)
        {
            var sid = ReadTokenUserSid(token);
            if (!IsTrustedServiceSid(sid))
            {
                throw new UnauthorizedAccessException("NetworkGuard pipe is not owned by a trusted Windows service account.");
            }
        }
    }

    internal static bool IsTrustedServiceSid(string value) =>
        string.Equals(value, LocalSystemSid, StringComparison.Ordinal);

    private static string ReadTokenUserSid(SafeKernelObjectHandle token)
    {
        _ = GetTokenInformation(
            token,
            TokenUserInformationClass,
            IntPtr.Zero,
            0,
            out var required);
        if (required == 0 || Marshal.GetLastWin32Error() != ErrorInsufficientBuffer)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }

        var buffer = Marshal.AllocHGlobal(checked((int)required));
        try
        {
            if (!GetTokenInformation(
                    token,
                    TokenUserInformationClass,
                    buffer,
                    required,
                    out _))
            {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            var tokenUser = Marshal.PtrToStructure<TokenUser>(buffer);
            if (!ConvertSidToStringSid(tokenUser.User.Sid, out var sidText))
            {
                throw new Win32Exception(Marshal.GetLastWin32Error());
            }
            try
            {
                return Marshal.PtrToStringUni(sidText)
                    ?? throw new InvalidOperationException("Windows returned an empty NetworkGuard service SID.");
            }
            finally
            {
                _ = LocalFree(sidText);
            }
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly record struct TokenUser(TokenSidAndAttributes User);

    [StructLayout(LayoutKind.Sequential)]
    private readonly record struct TokenSidAndAttributes(IntPtr Sid, uint Attributes);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetNamedPipeServerProcessId(
        SafePipeHandle pipe,
        out uint serverProcessId);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern SafeKernelObjectHandle OpenProcess(
        uint desiredAccess,
        bool inheritHandle,
        uint processId);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool OpenProcessToken(
        SafeKernelObjectHandle process,
        uint desiredAccess,
        out SafeKernelObjectHandle token);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool GetTokenInformation(
        SafeKernelObjectHandle token,
        int tokenInformationClass,
        IntPtr tokenInformation,
        uint tokenInformationLength,
        out uint returnLength);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool ConvertSidToStringSid(IntPtr sid, out IntPtr stringSid);

    [DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr memory);
}
