using System.IO.Pipes;
using System.Security.Principal;
using ChatOS.Connector.Security;

namespace ChatOS.Connector.LocalAgent;

internal interface ILocalAgentServerIdentityVerifier
{
    void Verify(NamedPipeClientStream pipe);
}

internal sealed class WindowsLocalAgentServerIdentityVerifier : ILocalAgentServerIdentityVerifier
{
    private readonly string _expectedUserSid;

    internal WindowsLocalAgentServerIdentityVerifier(string? expectedUserSid = null)
    {
        if (!OperatingSystem.IsWindows() && expectedUserSid is null)
        {
            throw new PlatformNotSupportedException("Local Agent identity verification requires Windows.");
        }
        _expectedUserSid = expectedUserSid ?? WindowsIdentity.GetCurrent().User?.Value
            ?? throw new UnauthorizedAccessException("The current Windows user SID is unavailable.");
        ArgumentException.ThrowIfNullOrWhiteSpace(_expectedUserSid);
    }

    public void Verify(NamedPipeClientStream pipe)
    {
        var actual = WindowsProcessTokenIdentityReader.ReadNamedPipeServerUserSid(pipe);
        if (!IsExpectedUserSid(actual, _expectedUserSid))
        {
            throw new UnauthorizedAccessException(
                "Local Agent Host does not run as the current desktop user.");
        }
    }

    internal static bool IsExpectedUserSid(string actual, string expected) =>
        !string.IsNullOrWhiteSpace(actual) &&
        !string.IsNullOrWhiteSpace(expected) &&
        string.Equals(actual, expected, StringComparison.OrdinalIgnoreCase);
}
