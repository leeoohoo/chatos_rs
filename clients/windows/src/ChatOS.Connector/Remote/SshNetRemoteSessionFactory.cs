using ChatOS.Connector.Security;
using ChatOS.Core.Domain;
using Renci.SshNet;
using Renci.SshNet.Common;

namespace ChatOS.Connector.Remote;

public interface IRemoteSshSessionFactory
{
    Task<RemoteSshSession> ConnectAsync(RemoteConnectionDraft draft, string? verificationCode, CancellationToken cancellationToken = default);
    Task<RemoteSftpSession> ConnectSftpAsync(RemoteConnectionDraft draft, string? verificationCode, CancellationToken cancellationToken = default);
}

public sealed class RemoteSshSession : IDisposable
{
    private readonly SshClient? _jumpClient;
    private readonly ForwardedPortLocal? _forward;

    internal RemoteSshSession(SshClient targetClient, SshClient? jumpClient = null, ForwardedPortLocal? forward = null)
    {
        TargetClient = targetClient;
        _jumpClient = jumpClient;
        _forward = forward;
    }

    public SshClient TargetClient { get; }

    public void Dispose()
    {
        try { if (TargetClient.IsConnected) TargetClient.Disconnect(); } catch { }
        TargetClient.Dispose();
        try { if (_forward?.IsStarted == true) _forward.Stop(); } catch { }
        _forward?.Dispose();
        try { if (_jumpClient?.IsConnected == true) _jumpClient.Disconnect(); } catch { }
        _jumpClient?.Dispose();
    }
}

public sealed class RemoteSftpSession : IDisposable
{
    private readonly SshClient? _jumpClient;
    private readonly ForwardedPortLocal? _forward;

    internal RemoteSftpSession(SftpClient client, SshClient? jumpClient, ForwardedPortLocal? forward)
    { Client = client; _jumpClient = jumpClient; _forward = forward; }

    public SftpClient Client { get; }

    public void Dispose()
    {
        try { if (Client.IsConnected) Client.Disconnect(); } catch { }
        Client.Dispose();
        try { if (_forward?.IsStarted == true) _forward.Stop(); } catch { }
        _forward?.Dispose();
        try { if (_jumpClient?.IsConnected == true) _jumpClient.Disconnect(); } catch { }
        _jumpClient?.Dispose();
    }
}

public sealed class SshNetRemoteSessionFactory : IRemoteSshSessionFactory
{
    private readonly IConnectorSecretStore _secrets;

    public SshNetRemoteSessionFactory(IConnectorSecretStore secrets) => _secrets = secrets;

    public async Task<RemoteSshSession> ConnectAsync(RemoteConnectionDraft draft, string? verificationCode, CancellationToken cancellationToken = default)
    {
        ValidateTarget(draft);
        SshClient? jumpClient = null;
        ForwardedPortLocal? forward = null;
        SshClient? targetClient = null;
        try
        {
            var networkHost = draft.Host.Trim();
            var networkPort = draft.Port;
            if (draft.JumpEnabled)
            {
                ValidateJump(draft);
                jumpClient = new SshClient(CreateConnectionInfo(draft.JumpHost!.Trim(), draft.JumpPort ?? 22, draft.JumpUsername!.Trim(), draft.JumpPassword, draft.JumpPrivateKeyPath, draft.JumpCertificatePath, verificationCode, out var jumpPrompt));
                await ConnectClientAsync(jumpClient, draft.JumpHost.Trim(), draft.JumpPort ?? 22, draft.HostKeyPolicy, jumpPrompt, verificationCode, cancellationToken).ConfigureAwait(false);
                forward = new ForwardedPortLocal("127.0.0.1", 0, draft.Host.Trim(), (uint)draft.Port);
                jumpClient.AddForwardedPort(forward);
                forward.Start();
                networkHost = "127.0.0.1";
                networkPort = checked((int)forward.BoundPort);
            }

            targetClient = new SshClient(CreateConnectionInfo(networkHost, networkPort, draft.Username.Trim(), draft.Password, draft.PrivateKeyPath, draft.CertificatePath, verificationCode, out var targetPrompt));
            await ConnectClientAsync(targetClient, draft.Host.Trim(), draft.Port, draft.HostKeyPolicy, targetPrompt, verificationCode, cancellationToken).ConfigureAwait(false);
            return new RemoteSshSession(targetClient, jumpClient, forward);
        }
        catch
        {
            targetClient?.Dispose();
            try { if (forward?.IsStarted == true) forward.Stop(); } catch { }
            forward?.Dispose();
            try { if (jumpClient?.IsConnected == true) jumpClient.Disconnect(); } catch { }
            jumpClient?.Dispose();
            throw;
        }
    }

    public async Task<RemoteSftpSession> ConnectSftpAsync(RemoteConnectionDraft draft, string? verificationCode, CancellationToken cancellationToken = default)
    {
        ValidateTarget(draft);
        SshClient? jumpClient = null;
        ForwardedPortLocal? forward = null;
        SftpClient? targetClient = null;
        try
        {
            var networkHost = draft.Host.Trim();
            var networkPort = draft.Port;
            if (draft.JumpEnabled)
            {
                ValidateJump(draft);
                jumpClient = new SshClient(CreateConnectionInfo(draft.JumpHost!.Trim(), draft.JumpPort ?? 22, draft.JumpUsername!.Trim(), draft.JumpPassword, draft.JumpPrivateKeyPath, draft.JumpCertificatePath, verificationCode, out var jumpPrompt));
                await ConnectClientAsync(jumpClient, draft.JumpHost.Trim(), draft.JumpPort ?? 22, draft.HostKeyPolicy, jumpPrompt, verificationCode, cancellationToken).ConfigureAwait(false);
                forward = new ForwardedPortLocal("127.0.0.1", 0, draft.Host.Trim(), (uint)draft.Port);
                jumpClient.AddForwardedPort(forward);
                forward.Start();
                networkHost = "127.0.0.1";
                networkPort = checked((int)forward.BoundPort);
            }
            targetClient = new SftpClient(CreateConnectionInfo(networkHost, networkPort, draft.Username.Trim(), draft.Password, draft.PrivateKeyPath, draft.CertificatePath, verificationCode, out var targetPrompt));
            await ConnectClientAsync(targetClient, draft.Host.Trim(), draft.Port, draft.HostKeyPolicy, targetPrompt, verificationCode, cancellationToken).ConfigureAwait(false);
            return new RemoteSftpSession(targetClient, jumpClient, forward);
        }
        catch
        {
            targetClient?.Dispose();
            try { if (forward?.IsStarted == true) forward.Stop(); } catch { }
            forward?.Dispose();
            try { if (jumpClient?.IsConnected == true) jumpClient.Disconnect(); } catch { }
            jumpClient?.Dispose();
            throw;
        }
    }

    private async Task ConnectClientAsync(BaseClient client, string identityHost, int identityPort, RemoteHostKeyPolicy policy, Func<string?> interactivePrompt, string? verificationCode, CancellationToken cancellationToken)
    {
        var hostKeyId = $"remote-host-key-v1:{identityHost.ToLowerInvariant()}:{identityPort}";
        var trustedFingerprint = await _secrets.GetAsync(hostKeyId, cancellationToken).ConfigureAwait(false);
        string? acceptedFingerprint = null;
        client.HostKeyReceived += (_, args) =>
        {
            var fingerprint = $"SHA256:{args.FingerPrintSHA256}";
            if (!string.IsNullOrWhiteSpace(trustedFingerprint)) args.CanTrust = string.Equals(trustedFingerprint, fingerprint, StringComparison.Ordinal);
            else
            {
                args.CanTrust = policy == RemoteHostKeyPolicy.AcceptNew;
                if (args.CanTrust) acceptedFingerprint = fingerprint;
            }
        };

        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromSeconds(20));
        try
        {
            await client.ConnectAsync(timeout.Token).ConfigureAwait(false);
            if (acceptedFingerprint is not null) await _secrets.SetAsync(hostKeyId, acceptedFingerprint, cancellationToken).ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            throw new TimeoutException($"SSH 连接 {identityHost}:{identityPort} 超时，请检查网络和端口。");
        }
        catch (SshAuthenticationException exception)
        {
            if (!string.IsNullOrWhiteSpace(interactivePrompt()) && string.IsNullOrWhiteSpace(verificationCode)) throw new RemoteVerificationRequiredException(interactivePrompt()!);
            throw new InvalidOperationException($"SSH 认证 {identityHost}:{identityPort} 失败，请检查用户名、本机凭据或二次验证码。", exception);
        }
        catch (SshConnectionException exception)
        {
            var message = string.IsNullOrWhiteSpace(trustedFingerprint) && policy == RemoteHostKeyPolicy.Strict
                ? $"{identityHost}:{identityPort} 的严格主机密钥校验未通过。请确认主机身份后使用“首次接受”。"
                : $"SSH 无法连接 {identityHost}:{identityPort}，请检查网络和主机密钥。";
            throw new InvalidOperationException(message, exception);
        }
    }

    private static ConnectionInfo CreateConnectionInfo(string host, int port, string username, string? password, string? privateKeyPath, string? certificatePath, string? verificationCode, out Func<string?> promptAccessor)
    {
        var promptText = string.Empty;
        var keyboard = new KeyboardInteractiveAuthenticationMethod(username);
        keyboard.AuthenticationPrompt += (_, args) =>
        {
            foreach (var prompt in args.Prompts)
            {
                if (LooksLikePassword(prompt.Request) && !string.IsNullOrWhiteSpace(password)) prompt.Response = password;
                else
                {
                    promptText = string.IsNullOrWhiteSpace(prompt.Request) ? "请输入 SSH 二次验证码。" : prompt.Request.Trim();
                    prompt.Response = verificationCode?.Trim() ?? string.Empty;
                }
            }
        };
        var methods = new List<AuthenticationMethod>();
        if (!string.IsNullOrWhiteSpace(privateKeyPath))
        {
            var key = !string.IsNullOrWhiteSpace(certificatePath) ? new PrivateKeyFile(privateKeyPath.Trim(), null, certificatePath.Trim()) : new PrivateKeyFile(privateKeyPath.Trim());
            methods.Add(new PrivateKeyAuthenticationMethod(username, key));
        }
        if (!string.IsNullOrWhiteSpace(password)) methods.Add(new PasswordAuthenticationMethod(username, password));
        methods.Add(keyboard);
        promptAccessor = () => promptText;
        return new ConnectionInfo(host, port, username, methods.ToArray()) { Timeout = TimeSpan.FromSeconds(15) };
    }

    private static void ValidateTarget(RemoteConnectionDraft draft)
    {
        if (string.IsNullOrWhiteSpace(draft.Host)) throw new ArgumentException("请输入远端主机地址。");
        if (string.IsNullOrWhiteSpace(draft.Username)) throw new ArgumentException("请输入登录用户名。");
        if (draft.Port is < 1 or > 65535) throw new ArgumentException("SSH 端口必须在 1 到 65535 之间。");
        if (draft.AuthenticationType == RemoteAuthenticationType.Password && string.IsNullOrWhiteSpace(draft.Password)) throw new ArgumentException("本机没有保存这条连接的登录密码。");
        if (draft.AuthenticationType != RemoteAuthenticationType.Password) ValidateKey(draft.PrivateKeyPath, draft.AuthenticationType == RemoteAuthenticationType.PrivateKeyCertificate ? draft.CertificatePath : null);
    }

    private static void ValidateJump(RemoteConnectionDraft draft)
    {
        if (string.IsNullOrWhiteSpace(draft.JumpHost) || string.IsNullOrWhiteSpace(draft.JumpUsername)) throw new ArgumentException("跳板机地址和用户名不能为空。");
        if ((draft.JumpPort ?? 22) is < 1 or > 65535) throw new ArgumentException("跳板机端口必须在 1 到 65535 之间。");
        if (string.IsNullOrWhiteSpace(draft.JumpPassword) && string.IsNullOrWhiteSpace(draft.JumpPrivateKeyPath)) throw new ArgumentException("本机没有保存跳板机密码或私钥。");
        if (!string.IsNullOrWhiteSpace(draft.JumpPrivateKeyPath)) ValidateKey(draft.JumpPrivateKeyPath, draft.JumpCertificatePath);
    }

    private static void ValidateKey(string? privateKeyPath, string? certificatePath)
    {
        if (string.IsNullOrWhiteSpace(privateKeyPath) || !File.Exists(privateKeyPath)) throw new ArgumentException("本机私钥文件不存在或不可读。");
        if (!string.IsNullOrWhiteSpace(certificatePath) && !File.Exists(certificatePath)) throw new ArgumentException("本机 SSH 证书文件不存在或不可读。");
    }

    private static bool LooksLikePassword(string value) => value.Contains("password", StringComparison.OrdinalIgnoreCase) || value.Contains("密码", StringComparison.OrdinalIgnoreCase);
}
