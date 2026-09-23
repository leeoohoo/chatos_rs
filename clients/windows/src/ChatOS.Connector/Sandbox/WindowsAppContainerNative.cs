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

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode)]
    private static extern uint GetNamedSecurityInfo(
        string objectName,
        SeObjectType objectType,
        uint securityInformation,
        out IntPtr owner,
        out IntPtr group,
        out IntPtr dacl,
        out IntPtr sacl,
        out IntPtr securityDescriptor);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode)]
    private static extern uint SetNamedSecurityInfo(
        string objectName,
        SeObjectType objectType,
        uint securityInformation,
        IntPtr owner,
        IntPtr group,
        IntPtr dacl,
        IntPtr sacl);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode)]
    private static extern uint SetEntriesInAcl(
        uint entryCount,
        ref ExplicitAccess explicitEntry,
        IntPtr oldAcl,
        out IntPtr newAcl);

    [DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr memory);

    internal static IntPtr FreeLocalMemory(IntPtr memory) => LocalFree(memory);

    [DllImport("advapi32.dll")]
    internal static extern IntPtr FreeSid(IntPtr sid);

    private enum SeObjectType
    {
        FileObject = 1,
    }

    private enum TrusteeForm
    {
        Sid = 0,
    }

    private enum TrusteeType
    {
        Unknown = 0,
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct Trustee
    {
        public IntPtr MultipleTrustee;
        public int MultipleTrusteeOperation;
        public TrusteeForm TrusteeForm;
        public TrusteeType TrusteeType;
        public IntPtr Name;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ExplicitAccess
    {
        public uint AccessPermissions;
        public uint AccessMode;
        public uint Inheritance;
        public Trustee Trustee;
    }
}
