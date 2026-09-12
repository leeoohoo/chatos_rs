using ChatOS.Api.Http;
using ChatOS.Connector.LocalAgent;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using System.Reflection;

namespace ChatOS.Connector.Tests;

public sealed class WindowsLocalAgentAccountSessionTests
{
    [Fact]
    public async Task RestoredLoginStartsOneHostAndCreatesClientFromCurrentEndpoint()
    {
        var fixture = new SessionFixture();
        await using var session = fixture.Create();

        await session.ActivateAsync("user-1");
        _ = await session.GetClientAsync("user-1");

        Assert.Equal(["user-1"], fixture.Supervisor.StartedAccounts);
        Assert.Collection(
            fixture.ClientFactory.Endpoints,
            endpoint => Assert.Equal(fixture.Supervisor.State.ClientEndpoint, endpoint),
            endpoint => Assert.Equal(fixture.Supervisor.State.ClientEndpoint, endpoint));
        Assert.Equal("access-token", fixture.Credentials.String("user-1",
            WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference));
        Assert.Equal(32, fixture.Credentials.Bytes("user-1",
            WindowsLocalAgentHostBootstrapBuilder.ProviderContextKeyReference)?.Length);
        Assert.Equal(32, fixture.Credentials.Bytes("user-1",
            WindowsLocalAgentAccountSession.SqliteEncryptionKeyReference)?.Length);
    }

    [Fact]
    public async Task TokenUpdateRestartsTheHostWithTheNewCredential()
    {
        var fixture = new SessionFixture();
        await using var session = fixture.Create();
        await session.ActivateAsync("user-1");
        fixture.Tokens.Token = "rotated-token";

        await session.UpdateAccessTokenAsync("user-1");

        Assert.Equal(["user-1", "user-1"], fixture.Supervisor.StartedAccounts);
        Assert.Equal("rotated-token", fixture.Credentials.String("user-1",
            WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference));
        Assert.Equal(1, fixture.Supervisor.StopCount);
    }

    [Fact]
    public async Task AccountSwitchStopsOldHostAndKeepsCredentialsIsolated()
    {
        var fixture = new SessionFixture();
        await using var session = fixture.Create();
        await session.ActivateAsync("user-1");
        fixture.Tokens.Token = "account-two-token";

        await session.ActivateAsync("user-2");

        Assert.Equal(["user-1", "user-2"], fixture.Supervisor.StartedAccounts);
        Assert.Null(fixture.Credentials.String("user-1",
            WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference));
        Assert.NotNull(fixture.Credentials.Bytes("user-1",
            WindowsLocalAgentHostBootstrapBuilder.ProviderContextKeyReference));
        Assert.Equal("account-two-token", fixture.Credentials.String("user-2",
            WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference));
        var mismatch = await Assert.ThrowsAsync<WindowsLocalAgentAccountSessionException>(() =>
            session.GetClientAsync("user-1"));
        Assert.Equal(WindowsLocalAgentAccountSessionFailure.AccountMismatch, mismatch.Failure);
    }

    [Fact]
    public async Task LogoutStopsHostWithoutRestartAndDeletesOnlyReplaceableToken()
    {
        var fixture = new SessionFixture();
        await using var session = fixture.Create();
        await session.ActivateAsync("user-1");

        await session.LogoutAsync();

        Assert.Equal(WindowsLocalAgentHostStatus.Stopped, fixture.Supervisor.State.Status);
        Assert.Null(fixture.Credentials.String("user-1",
            WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference));
        Assert.NotNull(fixture.Credentials.String("user-1",
            WindowsLocalAgentAccountSession.DeviceIdReference));
        Assert.NotNull(fixture.Credentials.Bytes("user-1",
            WindowsLocalAgentHostBootstrapBuilder.ProviderContextKeyReference));
        Assert.Single(fixture.Supervisor.StartedAccounts);
    }

    [Fact]
    public async Task StartupFailureLeavesNoHalfActiveAccountOrAccessToken()
    {
        var fixture = new SessionFixture();
        fixture.Supervisor.StartFailure = new InvalidOperationException("launch failed");
        await using var session = fixture.Create();

        await Assert.ThrowsAsync<InvalidOperationException>(() => session.ActivateAsync("user-1"));

        Assert.Equal(WindowsLocalAgentHostStatus.Stopped, fixture.Supervisor.State.Status);
        Assert.Null(fixture.Credentials.String("user-1",
            WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference));
        var inactive = await Assert.ThrowsAsync<WindowsLocalAgentAccountSessionException>(() =>
            session.GetClientAsync("user-1"));
        Assert.Equal(WindowsLocalAgentAccountSessionFailure.Inactive, inactive.Failure);
    }

    [Fact]
    public async Task DamagedPersistentKeyFailsClosedInsteadOfReplacingIt()
    {
        var fixture = new SessionFixture();
        fixture.Credentials.SetBytes(
            "user-1",
            WindowsLocalAgentHostBootstrapBuilder.ProviderContextKeyReference,
            new byte[12]);
        await using var session = fixture.Create();

        var error = await Assert.ThrowsAsync<WindowsLocalAgentAccountSessionException>(() =>
            session.ActivateAsync("user-1"));

        Assert.Equal(WindowsLocalAgentAccountSessionFailure.InvalidPersistentKey, error.Failure);
        Assert.Empty(fixture.Supervisor.StartedAccounts);
        Assert.Null(fixture.Credentials.String("user-1",
            WindowsLocalAgentHostBootstrapBuilder.ModelAccessTokenReference));
    }

    private sealed class SessionFixture
    {
        private readonly string _root = Path.Combine(
            Path.GetTempPath(),
            $"chatos-account-{Guid.NewGuid():N}");

        public FakeTokenStore Tokens { get; } = new() { Token = "access-token" };
        public FakeCredentialStore Credentials { get; } = new();
        public FakeSupervisor Supervisor { get; } = new();
        public RecordingClientFactory ClientFactory { get; } = new();

        public WindowsLocalAgentAccountSession Create() => new(
            Tokens,
            Credentials,
            Supervisor,
            new WindowsLocalAgentHostBootstrapBuilder(),
            new FakeRuntimeConfiguration(_root),
            ClientFactory,
            count => Enumerable.Repeat((byte)0x5a, count).ToArray());
    }

    private sealed class FakeTokenStore : IAuthTokenStore
    {
        public string? Token { get; set; }
        public ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(Token);
        public ValueTask SetAccessTokenAsync(string token, CancellationToken cancellationToken = default)
        {
            Token = token;
            return ValueTask.CompletedTask;
        }
        public ValueTask ClearAsync(CancellationToken cancellationToken = default)
        {
            Token = null;
            return ValueTask.CompletedTask;
        }
    }

    private sealed class FakeCredentialStore : IWindowsLocalAgentCredentialStore
    {
        private readonly Dictionary<string, string> _strings = [];
        private readonly Dictionary<string, byte[]> _bytes = [];

        public ValueTask SaveCredentialAsync(string accountId, string reference, string secret,
            CancellationToken cancellationToken = default)
        {
            _strings[Key(accountId, reference)] = secret;
            return ValueTask.CompletedTask;
        }

        public ValueTask<string?> LoadCredentialAsync(string accountId, string reference,
            CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(_strings.GetValueOrDefault(Key(accountId, reference)));

        public ValueTask DeleteCredentialAsync(string accountId, string reference,
            CancellationToken cancellationToken = default)
        {
            _strings.Remove(Key(accountId, reference));
            return ValueTask.CompletedTask;
        }

        public Task SaveDeviceKeyAsync(string accountId, string reference, ReadOnlyMemory<byte> key,
            CancellationToken cancellationToken = default)
        {
            _bytes[Key(accountId, reference)] = key.ToArray();
            return Task.CompletedTask;
        }

        public Task<byte[]?> LoadDeviceKeyAsync(string accountId, string reference,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(_bytes.TryGetValue(Key(accountId, reference), out var value)
                ? value.ToArray()
                : null);

        public ValueTask DeleteDeviceKeyAsync(string accountId, string reference,
            CancellationToken cancellationToken = default)
        {
            _bytes.Remove(Key(accountId, reference));
            return ValueTask.CompletedTask;
        }

        public string? String(string accountId, string reference) =>
            _strings.GetValueOrDefault(Key(accountId, reference));
        public byte[]? Bytes(string accountId, string reference) =>
            _bytes.GetValueOrDefault(Key(accountId, reference));
        public void SetBytes(string accountId, string reference, byte[] value) =>
            _bytes[Key(accountId, reference)] = value;
        private static string Key(string accountId, string reference) => $"{accountId}\0{reference}";
    }

    private sealed class FakeSupervisor : IWindowsLocalAgentHostSupervisor
    {
        public List<string> StartedAccounts { get; } = [];
        public WindowsLocalAgentHostState State { get; private set; } =
            new(WindowsLocalAgentHostStatus.Stopped);
        public Exception? StartFailure { get; set; }
        public int StopCount { get; private set; }

        public Task<WindowsLocalAgentHostState> GetStateAsync() => Task.FromResult(State);

        public async Task StartAsync(string accountId,
            Func<CancellationToken, Task<WindowsLocalAgentHostLaunchConfiguration>> configurationProvider,
            CancellationToken cancellationToken = default)
        {
            if (State.Status != WindowsLocalAgentHostStatus.Stopped)
            {
                await LogoutAsync();
            }
            var configuration = await configurationProvider(cancellationToken);
            configuration.LaunchMaterial.Dispose();
            configuration.SecretMaterial.Dispose();
            if (StartFailure is not null)
            {
                throw StartFailure;
            }
            StartedAccounts.Add(accountId);
            State = new WindowsLocalAgentHostState(
                WindowsLocalAgentHostStatus.Running,
                accountId,
                (uint)StartedAccounts.Count,
                configuration.ExpectedClientEndpoint);
        }

        public Task LogoutAsync()
        {
            if (State.Status != WindowsLocalAgentHostStatus.Stopped)
            {
                StopCount++;
            }
            State = new WindowsLocalAgentHostState(WindowsLocalAgentHostStatus.Stopped);
            return Task.CompletedTask;
        }

        public ValueTask DisposeAsync() => ValueTask.CompletedTask;
    }

    private sealed class FakeRuntimeConfiguration(string root) : IWindowsLocalAgentRuntimeConfiguration
    {
        public WindowsLocalAgentHostBootstrapSettings Create(string accountId, string deviceId) => new()
        {
            ExecutablePath = Path.Combine(root, "local-agent-host.exe"),
            ExpectedExecutableSha256 = $"sha256:{new string('0', 64)}",
            AccountId = accountId,
            DeviceId = deviceId,
            AttachmentGrantDirectory = Path.Combine(root, accountId, "attachments"),
            PlatformStateDirectory = Path.Combine(root, accountId, "state"),
            ModelGatewayBaseUri = new Uri("https://gateway.example.test"),
            MemoryEngineBaseUri = new Uri("https://memory.example.test"),
            Storage = new WindowsLocalAgentSqliteBootstrap(
                Path.Combine(root, accountId, "client.sqlite3"),
                WindowsLocalAgentAccountSession.SqliteEncryptionKeyReference),
        };
    }

    private sealed class RecordingClientFactory : ILocalAgentIPCClientFactory
    {
        public List<string> Endpoints { get; } = [];
        public ILocalAgentIPCClient Create(string ownerUserId, string pipeName)
        {
            Endpoints.Add(pipeName);
            return DispatchProxy.Create<ILocalAgentIPCClient, NoopClientProxy>();
        }
    }

    private class NoopClientProxy : DispatchProxy
    {
        protected override object? Invoke(MethodInfo? targetMethod, object?[]? args) =>
            throw new NotSupportedException(targetMethod?.Name);
    }
}
