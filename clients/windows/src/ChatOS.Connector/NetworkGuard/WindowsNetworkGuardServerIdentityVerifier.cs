using System.IO.Pipes;
using ChatOS.Connector.Security;

namespace ChatOS.Connector.NetworkGuard;

internal interface INetworkGuardServerIdentityVerifier
{
    void Verify(NamedPipeClientStream pipe);
}

internal sealed class WindowsNetworkGuardServerIdentityVerifier : INetworkGuardServerIdentityVerifier
{
    private const string LocalSystemSid = "S-1-5-18";

    public void Verify(NamedPipeClientStream pipe)
    {
        ArgumentNullException.ThrowIfNull(pipe);
        var sid = WindowsProcessTokenIdentityReader.ReadNamedPipeServerUserSid(pipe);
        if (!IsTrustedServiceSid(sid))
        {
            throw new UnauthorizedAccessException("NetworkGuard pipe is not owned by a trusted Windows service account.");
        }
    }

    internal static bool IsTrustedServiceSid(string value) =>
        string.Equals(value, LocalSystemSid, StringComparison.Ordinal);

}
