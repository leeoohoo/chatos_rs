using System.ComponentModel;
using System.IO.Pipes;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace ChatOS.Connector.Security;

/// <summary>
/// Reads the user SID from the process at the other end of a connected named
/// pipe. Trust policy deliberately lives in the caller so this code can be
/// shared by privileged services and same-user desktop helpers.
/// </summary>
internal static class WindowsProcessTokenIdentityReader
{
    private const uint ProcessQueryLimitedInformation = 0x1000;
    private const uint TokenQuery = 0x0008;
    private const int TokenUserInformationClass = 1;
    private const int ErrorInsufficientBuffer = 122;

    internal static string ReadNamedPipeServerUserSid(NamedPipeClientStream pipe)
    {
        ArgumentNullException.ThrowIfNull(pipe);
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException("Named-pipe process identity verification requires Windows.");
        }
        if (!pipe.IsConnected || !GetNamedPipeServerProcessId(pipe.SafePipeHandle, out var processId) ||
            processId == 0)
        {
            throw new UnauthorizedAccessException("Named-pipe server process identity is unavailable.");
        }

        using var process = OpenProcess(ProcessQueryLimitedInformation, inheritHandle: false, processId);
        if (process.IsInvalid)
        {
            throw new UnauthorizedAccessException("Named-pipe server process cannot be inspected.");
        }
        if (!OpenProcessToken(process, TokenQuery, out var token))
        {
            throw new UnauthorizedAccessException("Named-pipe server token cannot be inspected.");
        }
        using (token)
        {
            return ReadTokenUserSid(token);
        }
    }

    private static string ReadTokenUserSid(SafeAccessTokenHandle token)
    {
        _ = GetTokenInformation(token, TokenUserInformationClass, IntPtr.Zero, 0, out var required);
        if (required == 0 || Marshal.GetLastWin32Error() != ErrorInsufficientBuffer)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }

        var buffer = Marshal.AllocHGlobal(checked((int)required));
        try
        {
            if (!GetTokenInformation(token, TokenUserInformationClass, buffer, required, out _))
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
                    ?? throw new InvalidOperationException("Windows returned an empty process SID.");
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
    private static extern bool GetNamedPipeServerProcessId(SafePipeHandle pipe, out uint serverProcessId);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern SafeProcessHandle OpenProcess(uint desiredAccess, bool inheritHandle, uint processId);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool OpenProcessToken(
        SafeProcessHandle process,
        uint desiredAccess,
        out SafeAccessTokenHandle token);

    [DllImport("advapi32.dll", SetLastError = true)]
    private static extern bool GetTokenInformation(
        SafeAccessTokenHandle token,
        int tokenInformationClass,
        IntPtr tokenInformation,
        uint tokenInformationLength,
        out uint returnLength);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool ConvertSidToStringSid(IntPtr sid, out IntPtr stringSid);

    [DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr memory);
}
