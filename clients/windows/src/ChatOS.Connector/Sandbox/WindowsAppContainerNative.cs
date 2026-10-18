using System.ComponentModel;
using System.Runtime.InteropServices;

namespace ChatOS.Connector.Sandbox;

internal static partial class WindowsAppContainerSandbox
{
    private static IntPtr CreateOrDeriveProfileSid(string profileName)
    {
        var result = CreateAppContainerProfile(
            profileName,
            "ChatOS Windows command sandbox",
            "Isolated command execution for the ChatOS Windows client.",
            IntPtr.Zero,
            0,
            out var sid);
        if (result == 0)
        {
            return sid;
        }
        if (result != ErrorAlreadyExistsHResult)
        {
            Marshal.ThrowExceptionForHR(result);
        }

        result = DeriveAppContainerSidFromAppContainerName(profileName, out sid);
        if (result != 0)
        {
            Marshal.ThrowExceptionForHR(result);
        }
        return sid;
    }

    private static string SafeAclError(string error, string output)
    {
        var value = string.IsNullOrWhiteSpace(error) ? output : error;
        value = new string(value
            .Where(character => !char.IsControl(character) || character == ' ')
            .Take(500)
            .ToArray());
        return string.IsNullOrWhiteSpace(value) ? "ACL update failed" : value;
    }

    private static IntPtr CapabilitySid(string value)
    {
        if (!ConvertStringSidToSid(value, out var sid))
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        return sid;
    }

    private static string SidToString(IntPtr sid)
    {
        if (!ConvertSidToStringSid(sid, out var value))
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        try
        {
            return Marshal.PtrToStringUni(value)
                ?? throw new InvalidOperationException("Windows returned an empty AppContainer SID.");
        }
        finally
        {
            _ = LocalFree(value);
        }
    }

    [DllImport("userenv.dll", CharSet = CharSet.Unicode)]
    private static extern int CreateAppContainerProfile(
        string appContainerName,
        string displayName,
        string description,
        IntPtr capabilities,
        uint capabilityCount,
        out IntPtr appContainerSid);

    [DllImport("userenv.dll", CharSet = CharSet.Unicode)]
    private static extern int DeriveAppContainerSidFromAppContainerName(
        string appContainerName,
        out IntPtr appContainerSid);

    [DllImport("userenv.dll", CharSet = CharSet.Unicode)]
    private static extern int DeleteAppContainerProfile(string appContainerName);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool ConvertStringSidToSid(string stringSid, out IntPtr sid);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool ConvertSidToStringSid(IntPtr sid, out IntPtr stringSid);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool SetFileSecurity(
        string fileName,
        uint securityInformation,
        byte[] securityDescriptor);

    [DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr memory);

    internal static IntPtr FreeLocalMemory(IntPtr memory) => LocalFree(memory);

    [DllImport("advapi32.dll")]
    internal static extern IntPtr FreeSid(IntPtr sid);

    private static bool DeleteControlledProfile(string profileName)
    {
        var result = DeleteAppContainerProfile(profileName);
        return result == 0 ||
            result == unchecked((int)0x80070490) ||
            result == unchecked((int)0x80070002);
    }
}
