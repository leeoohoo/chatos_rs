using ChatOS.Core.Domain;

namespace ChatOS.Connector.Remote;

public sealed class SshNetRemoteConnectionTester : IRemoteConnectionTester
{
    private readonly IRemoteSshSessionFactory _sessions;

    public SshNetRemoteConnectionTester(IRemoteSshSessionFactory sessions) => _sessions = sessions;

    public async Task<RemoteConnectionTestResult> TestAsync(
        RemoteConnectionDraft draft,
        string? verificationCode,
        CancellationToken cancellationToken = default)
    {
        using var session = await _sessions.ConnectAsync(draft, verificationCode, cancellationToken).ConfigureAwait(false);
        return new RemoteConnectionTestResult(true, draft.JumpEnabled
            ? "SSH 已通过跳板机连接，目标主机密钥和认证均已验证。"
            : "SSH 连接成功，主机密钥和认证均已验证。");
    }
}
