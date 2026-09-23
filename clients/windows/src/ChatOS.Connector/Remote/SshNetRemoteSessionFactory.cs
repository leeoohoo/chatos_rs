using System.Collections.Concurrent;
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
    private const int MaxPendingAttemptsPerKind = 8;
    private static readonly TimeSpan PendingAttemptLifetime = TimeSpan.FromMinutes(5);
    private static readonly TimeSpan ConnectionPhaseTimeout = TimeSpan.FromSeconds(20);
    private readonly IConnectorSecretStore _secrets;
    private readonly ConcurrentDictionary<RemoteConnectionDraft, SshSessionAttempt<RemoteSshSession>>
        _pendingSsh = new();
    private readonly ConcurrentDictionary<RemoteConnectionDraft, SshSessionAttempt<RemoteSftpSession>>
        _pendingSftp = new();

    public SshNetRemoteSessionFactory(IConnectorSecretStore secrets) => _secrets = secrets;

    public Task<RemoteSshSession> ConnectAsync(
        RemoteConnectionDraft draft,
        string? verificationCode,
        CancellationToken cancellationToken = default)
    {
        ValidateTarget(draft);
        if (draft.JumpEnabled) ValidateJump(draft);
        return ConnectWithContinuationAsync(
            draft, verificationCode, _pendingSsh, ConnectSshCoreAsync, cancellationToken);
    }

    public Task<RemoteSftpSession> ConnectSftpAsync(
        RemoteConnectionDraft draft,
        string? verificationCode,
        CancellationToken cancellationToken = default)
    {
        ValidateTarget(draft);
        if (draft.JumpEnabled) ValidateJump(draft);
        return ConnectWithContinuationAsync(
            draft, verificationCode, _pendingSftp, ConnectSftpCoreAsync, cancellationToken);
    }

    private async Task<T> ConnectWithContinuationAsync<T>(
        RemoteConnectionDraft draft,
        string? verificationCode,
        ConcurrentDictionary<RemoteConnectionDraft, SshSessionAttempt<T>> pending,
        Func<RemoteConnectionDraft, SshVerificationExchange, CancellationToken, Task<T>> connect,
        CancellationToken cancellationToken)
        where T : class, IDisposable
    {
        var code = Clean(verificationCode);
        if (code is not null && pending.TryRemove(draft, out var continued))
        {
            var version = continued.VerificationVersion;
            if (continued.Submit(version, code))
            {
                return await ObserveOrParkAsync(
                    draft, continued, version, pending, cancellationToken).ConfigureAwait(false);
            }
            continued.Cancel();
        }
        else if (pending.TryRemove(draft, out var abandoned))
        {
            abandoned.Cancel();
        }

        var attempt = new SshSessionAttempt<T>(
            code,
            PendingAttemptLifetime,
            (verification, lifetimeToken) => connect(draft, verification, lifetimeToken));
        return await ObserveOrParkAsync(
            draft, attempt, afterVersion: 0, pending, cancellationToken).ConfigureAwait(false);
    }

    private async Task<T> ObserveOrParkAsync<T>(
        RemoteConnectionDraft draft,
        SshSessionAttempt<T> attempt,
        long afterVersion,
        ConcurrentDictionary<RemoteConnectionDraft, SshSessionAttempt<T>> pending,
        CancellationToken cancellationToken)
        where T : class, IDisposable
    {
        try
        {
            var challenge = await attempt.ObserveAsync(
                afterVersion, ConnectionPhaseTimeout, cancellationToken).ConfigureAwait(false);
            if (challenge is null) return await attempt.TakeAsync().ConfigureAwait(false);

            ParkBounded(draft, attempt, pending);
            throw new RemoteVerificationRequiredException(challenge.Prompt);
        }
        catch (RemoteVerificationRequiredException)
        {
            throw;
        }
        catch
        {
            RemoveExact(pending, draft, attempt);
            attempt.Cancel();
            throw;
        }
    }

    private static void ParkBounded<T>(
        RemoteConnectionDraft draft,
        SshSessionAttempt<T> attempt,
        ConcurrentDictionary<RemoteConnectionDraft, SshSessionAttempt<T>> pending)
        where T : class, IDisposable
    {
        while (pending.Count >= MaxPendingAttemptsPerKind)
        {
            var oldest = pending.MinBy(pair => pair.Value.CreatedAt);
            if (oldest.Value is null) break;
            if (RemoveExact(pending, oldest.Key, oldest.Value)) oldest.Value.Cancel();
        }
        if (pending.TryGetValue(draft, out var replaced) &&
            RemoveExact(pending, draft, replaced))
        {
            replaced.Cancel();
        }
        if (!pending.TryAdd(draft, attempt))
        {
            attempt.Cancel();
            throw new InvalidOperationException("SSH 验证会话冲突，请重新测试连接。");
        }
        if (attempt.TryStartReaper()) _ = ReapAttemptAsync(draft, attempt, pending);
    }

    private static async Task ReapAttemptAsync<T>(
        RemoteConnectionDraft draft,
        SshSessionAttempt<T> attempt,
        ConcurrentDictionary<RemoteConnectionDraft, SshSessionAttempt<T>> pending)
        where T : class, IDisposable
    {
        await Task.WhenAny(attempt.Completion, Task.Delay(PendingAttemptLifetime))
            .ConfigureAwait(false);
        if (RemoveExact(pending, draft, attempt)) attempt.Cancel();
    }

    private static bool RemoveExact<T>(
        ConcurrentDictionary<RemoteConnectionDraft, SshSessionAttempt<T>> pending,
        RemoteConnectionDraft draft,
        SshSessionAttempt<T> attempt)
        where T : class, IDisposable =>
        ((ICollection<KeyValuePair<RemoteConnectionDraft, SshSessionAttempt<T>>>)pending)
            .Remove(new KeyValuePair<RemoteConnectionDraft, SshSessionAttempt<T>>(draft, attempt));

    private async Task<RemoteSshSession> ConnectSshCoreAsync(
        RemoteConnectionDraft draft,
        SshVerificationExchange verification,
        CancellationToken cancellationToken)
    {
        SshClient? jumpClient = null;
        ForwardedPortLocal? forward = null;
        SshClient? targetClient = null;
        try
        {
            var networkHost = draft.Host.Trim();
            var networkPort = draft.Port;
            if (draft.JumpEnabled)
            {
                jumpClient = new SshClient(CreateConnectionInfo(
                    draft.JumpHost!.Trim(), draft.JumpPort ?? 22,
                    draft.JumpUsername!.Trim(), draft.JumpPassword,
                    draft.JumpPrivateKeyPath, draft.JumpCertificatePath, verification));
                await ConnectClientAsync(
                    jumpClient, draft.JumpHost.Trim(), draft.JumpPort ?? 22,
                    draft.HostKeyPolicy, cancellationToken).ConfigureAwait(false);
                forward = new ForwardedPortLocal(
                    "127.0.0.1", 0, draft.Host.Trim(), (uint)draft.Port);
                jumpClient.AddForwardedPort(forward);
                forward.Start();
                networkHost = "127.0.0.1";
                networkPort = checked((int)forward.BoundPort);
            }

            targetClient = new SshClient(CreateConnectionInfo(
                networkHost, networkPort, draft.Username.Trim(), draft.Password,
                draft.PrivateKeyPath, draft.CertificatePath, verification));
            await ConnectClientAsync(
                targetClient, draft.Host.Trim(), draft.Port,
                draft.HostKeyPolicy, cancellationToken).ConfigureAwait(false);
            return new RemoteSshSession(targetClient, jumpClient, forward);
        }
        catch
        {
            targetClient?.Dispose();
            DisposeJump(jumpClient, forward);
            throw;
        }
    }

    private async Task<RemoteSftpSession> ConnectSftpCoreAsync(
        RemoteConnectionDraft draft,
        SshVerificationExchange verification,
        CancellationToken cancellationToken)
    {
        SshClient? jumpClient = null;
        ForwardedPortLocal? forward = null;
        SftpClient? targetClient = null;
        try
        {
            var networkHost = draft.Host.Trim();
            var networkPort = draft.Port;
            if (draft.JumpEnabled)
            {
                jumpClient = new SshClient(CreateConnectionInfo(
                    draft.JumpHost!.Trim(), draft.JumpPort ?? 22,
                    draft.JumpUsername!.Trim(), draft.JumpPassword,
                    draft.JumpPrivateKeyPath, draft.JumpCertificatePath, verification));
                await ConnectClientAsync(
                    jumpClient, draft.JumpHost.Trim(), draft.JumpPort ?? 22,
                    draft.HostKeyPolicy, cancellationToken).ConfigureAwait(false);
                forward = new ForwardedPortLocal(
                    "127.0.0.1", 0, draft.Host.Trim(), (uint)draft.Port);
                jumpClient.AddForwardedPort(forward);
                forward.Start();
                networkHost = "127.0.0.1";
                networkPort = checked((int)forward.BoundPort);
            }
            targetClient = new SftpClient(CreateConnectionInfo(
                networkHost, networkPort, draft.Username.Trim(), draft.Password,
                draft.PrivateKeyPath, draft.CertificatePath, verification));
            await ConnectClientAsync(
                targetClient, draft.Host.Trim(), draft.Port,
                draft.HostKeyPolicy, cancellationToken).ConfigureAwait(false);
            return new RemoteSftpSession(targetClient, jumpClient, forward);
        }
        catch
        {
            targetClient?.Dispose();
            DisposeJump(jumpClient, forward);
            throw;
        }
    }

    private async Task ConnectClientAsync(
        BaseClient client,
        string identityHost,
        int identityPort,
        RemoteHostKeyPolicy policy,
        CancellationToken cancellationToken)
    {
        var hostKeyId = $"remote-host-key-v1:{identityHost.ToLowerInvariant()}:{identityPort}";
        var trustedFingerprint = await _secrets.GetAsync(hostKeyId, cancellationToken)
            .ConfigureAwait(false);
        string? acceptedFingerprint = null;
        client.HostKeyReceived += (_, args) =>
        {
            var fingerprint = $"SHA256:{args.FingerPrintSHA256}";
            if (!string.IsNullOrWhiteSpace(trustedFingerprint))
            {
                args.CanTrust = string.Equals(
                    trustedFingerprint, fingerprint, StringComparison.Ordinal);
            }
            else
            {
                args.CanTrust = policy == RemoteHostKeyPolicy.AcceptNew;
                if (args.CanTrust) acceptedFingerprint = fingerprint;
            }
        };

        try
        {
            await client.ConnectAsync(cancellationToken).ConfigureAwait(false);
            if (acceptedFingerprint is not null)
            {
                await _secrets.SetAsync(
                    hostKeyId, acceptedFingerprint, cancellationToken).ConfigureAwait(false);
            }
        }
        catch (SshAuthenticationException exception)
        {
            throw new InvalidOperationException(
                $"SSH 认证 {identityHost}:{identityPort} 失败，请检查用户名、本机凭据或二次验证码。",
                exception);
        }
        catch (SshConnectionException exception)
        {
            var message = string.IsNullOrWhiteSpace(trustedFingerprint) &&
                          policy == RemoteHostKeyPolicy.Strict
                ? $"{identityHost}:{identityPort} 的严格主机密钥校验未通过。请确认主机身份后使用“首次接受”。"
                : $"SSH 无法连接 {identityHost}:{identityPort}，请检查网络和主机密钥。";
            throw new InvalidOperationException(message, exception);
        }
    }

    private static ConnectionInfo CreateConnectionInfo(
        string host,
        int port,
        string username,
        string? password,
        string? privateKeyPath,
        string? certificatePath,
        SshVerificationExchange verification)
    {
        var keyboard = new KeyboardInteractiveAuthenticationMethod(username);
        keyboard.AuthenticationPrompt += (_, args) =>
        {
            foreach (var prompt in args.Prompts)
            {
                prompt.Response = verification.ResolveResponse(prompt.Request, password);
            }
        };
        var methods = new List<AuthenticationMethod>();
        if (!string.IsNullOrWhiteSpace(privateKeyPath))
        {
            var key = !string.IsNullOrWhiteSpace(certificatePath)
                ? new PrivateKeyFile(privateKeyPath.Trim(), null, certificatePath.Trim())
                : new PrivateKeyFile(privateKeyPath.Trim());
            methods.Add(new PrivateKeyAuthenticationMethod(username, key));
        }
        if (!string.IsNullOrWhiteSpace(password))
        {
            methods.Add(new PasswordAuthenticationMethod(username, password));
        }
        methods.Add(keyboard);
        return new ConnectionInfo(host, port, username, methods.ToArray())
        {
            Timeout = PendingAttemptLifetime,
        };
    }

    private static void DisposeJump(SshClient? jumpClient, ForwardedPortLocal? forward)
    {
        try { if (forward?.IsStarted == true) forward.Stop(); } catch { }
        forward?.Dispose();
        try { if (jumpClient?.IsConnected == true) jumpClient.Disconnect(); } catch { }
        jumpClient?.Dispose();
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

    private static string? Clean(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();
}
