using ChatOS.Connector.Remote;
using ChatOS.Connector.Security;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class WindowsRemoteConnectionServiceTests
{
    [Fact]
    public async Task CreateKeepsSecretsLocalAndDecoratesCredentialFlags()
    {
        var cloud = new CloudDouble();
        var secrets = new MemorySecretStore();
        var service = new WindowsRemoteConnectionService(
            cloud,
            new RemoteConnectionCredentialStore(secrets),
            new TesterDouble());

        var created = await service.CreateAsync(Draft());

        Assert.Null(cloud.LastDraft?.Password);
        Assert.Null(cloud.LastDraft?.PrivateKeyPath);
        Assert.Null(cloud.LastDraft?.JumpPassword);
        Assert.True(created.HasPassword);
        var rawSecret = Assert.Single(secrets.Values).Value;
        Assert.Contains("password-1", rawSecret);
    }

    [Fact]
    public async Task FailedCredentialCommitDeletesCloudRecord()
    {
        var cloud = new CloudDouble();
        var service = new WindowsRemoteConnectionService(
            cloud,
            new RemoteConnectionCredentialStore(new MemorySecretStore { FailSet = true }),
            new TesterDouble());

        await Assert.ThrowsAsync<IOException>(() => service.CreateAsync(Draft()));

        Assert.Equal("remote-1", cloud.DeletedId);
    }

    [Fact]
    public async Task TestSavedRestoresLocalCredentialBeforeCallingSshTester()
    {
        var cloud = new CloudDouble();
        var secrets = new MemorySecretStore();
        var store = new RemoteConnectionCredentialStore(secrets);
        await store.SaveAsync("remote-1", RemoteConnectionCredentials.From(Draft()));
        var tester = new TesterDouble();
        var service = new WindowsRemoteConnectionService(cloud, store, tester);

        var result = await service.TestSavedAsync("remote-1", "123456");

        Assert.True(result.Success);
        Assert.Equal("password-1", tester.LastDraft?.Password);
        Assert.Equal("123456", tester.LastVerificationCode);
    }

    [Fact]
    public async Task DeleteRemovesCloudMetadataAndLocalCredential()
    {
        var cloud = new CloudDouble();
        var secrets = new MemorySecretStore();
        var store = new RemoteConnectionCredentialStore(secrets);
        await store.SaveAsync("remote-1", RemoteConnectionCredentials.From(Draft()));
        var service = new WindowsRemoteConnectionService(cloud, store, new TesterDouble());

        await service.DeleteAsync("remote-1");

        Assert.Equal("remote-1", cloud.DeletedId);
        Assert.Empty(secrets.Values);
    }

    [Fact]
    public async Task SshTesterRejectsMissingPasswordBeforeOpeningNetworkConnection()
    {
        var tester = new SshNetRemoteConnectionTester(
            new SshNetRemoteSessionFactory(new MemorySecretStore()));

        var error = await Assert.ThrowsAsync<ArgumentException>(() =>
            tester.TestAsync(Draft() with { Password = null }, null));

        Assert.Contains("登录密码", error.Message);
    }

    [Fact]
    public async Task SshTesterRequiresJumpHostCredentialsBeforeOpeningNetworkConnection()
    {
        var tester = new SshNetRemoteConnectionTester(
            new SshNetRemoteSessionFactory(new MemorySecretStore()));

        var error = await Assert.ThrowsAsync<ArgumentException>(() => tester.TestAsync(Draft() with
        {
            JumpEnabled = true,
            JumpHost = "jump.example",
            JumpUsername = "jump-user",
            JumpPassword = null,
        }, null));

        Assert.Contains("跳板机密码或私钥", error.Message);
    }

    [Fact]
    public async Task TestSavedResolvesJumpMetadataAndCredentialsFromReferencedConnection()
    {
        var cloud = new CloudDouble();
        cloud.Values.Add(Connection() with
        {
            Id = "remote-target",
            JumpEnabled = true,
            JumpConnectionId = "remote-1",
        });
        var store = new RemoteConnectionCredentialStore(new MemorySecretStore());
        await store.SaveAsync("remote-1", new RemoteConnectionCredentials(
            "jump-secret", null, null, null, null, null));
        var tester = new TesterDouble();
        var service = new WindowsRemoteConnectionService(cloud, store, tester);

        await service.TestSavedAsync("remote-target", null);

        Assert.True(tester.LastDraft?.JumpEnabled);
        Assert.Equal("server.example", tester.LastDraft?.JumpHost);
        Assert.Equal("deploy", tester.LastDraft?.JumpUsername);
        Assert.Equal("jump-secret", tester.LastDraft?.JumpPassword);
    }

    private static RemoteConnectionDraft Draft() => new(
        "Server", "server.example", 22, "deploy", RemoteAuthenticationType.Password,
        "password-1", null, null, "/srv/app", RemoteHostKeyPolicy.AcceptNew,
        "device-1", "workspace-1", false, null, null, null, null, null, null, "jump-password");

    private static RemoteConnection Connection() => new(
        "remote-1", "Server", "server.example", 22, "deploy",
        RemoteAuthenticationType.Password, false, false, false, "/srv/app",
        RemoteHostKeyPolicy.AcceptNew, "device-1", "workspace-1", false,
        null, null, null, null, false, false, false, null);

    private sealed class CloudDouble : IRemoteConnectionCloudService
    {
        public List<RemoteConnection> Values { get; } = [Connection()];
        public RemoteConnectionDraft? LastDraft { get; private set; }
        public string? DeletedId { get; private set; }

        public Task<IReadOnlyList<RemoteConnection>> ListAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<RemoteConnection>>(Values);

        public Task<RemoteConnection> CreateAsync(
            RemoteConnectionDraft draft,
            CancellationToken cancellationToken = default)
        {
            LastDraft = draft;
            return Task.FromResult(Connection());
        }

        public Task<RemoteConnection> UpdateAsync(
            string id,
            RemoteConnectionDraft draft,
            CancellationToken cancellationToken = default)
        {
            LastDraft = draft;
            return Task.FromResult(Connection());
        }

        public Task DeleteAsync(string id, CancellationToken cancellationToken = default)
        {
            DeletedId = id;
            return Task.CompletedTask;
        }
    }

    private sealed class TesterDouble : IRemoteConnectionTester
    {
        public RemoteConnectionDraft? LastDraft { get; private set; }
        public string? LastVerificationCode { get; private set; }

        public Task<RemoteConnectionTestResult> TestAsync(
            RemoteConnectionDraft draft,
            string? verificationCode,
            CancellationToken cancellationToken = default)
        {
            LastDraft = draft;
            LastVerificationCode = verificationCode;
            return Task.FromResult(new RemoteConnectionTestResult(true, "ok"));
        }
    }

    private sealed class MemorySecretStore : IConnectorSecretStore
    {
        public Dictionary<string, string> Values { get; } = new(StringComparer.Ordinal);
        public bool FailSet { get; init; }

        public ValueTask<string?> GetAsync(string key, CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(Values.GetValueOrDefault(key));

        public ValueTask SetAsync(string key, string value, CancellationToken cancellationToken = default)
        {
            if (FailSet) throw new IOException("vault unavailable");
            Values[key] = value;
            return ValueTask.CompletedTask;
        }

        public ValueTask DeleteAsync(string key, CancellationToken cancellationToken = default)
        {
            Values.Remove(key);
            return ValueTask.CompletedTask;
        }
    }
}
