using ChatOS.Connector.Gateway;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Security;

namespace ChatOS.Connector.Tests;

public sealed class ConnectorPairingServiceTests
{
    [Fact]
    public async Task PairingMovesMatchingWorkspaceAndCommitsTokenWithState()
    {
        using var root = TestDirectory.Create();
        var secrets = new MemorySecretStore();
        var identity = await new ConnectorDeviceIdentityProvider(secrets).GetAsync();
        var fingerprint = ConnectorPairingService.Fingerprint(root.Path, identity.PublicKey);
        var gateway = new FakeGateway(identity.PublicKey)
        {
            Workspaces =
            [
                new ConnectorGatewayWorkspace("workspace-1", "old-device", "Existing", fingerprint),
            ],
        };
        var tokens = new MemoryTokenStore("old-token");
        var store = new MemoryStateStore();
        var runtime = new ConnectorRuntimeContext(store, tokens);
        await runtime.InitializeAsync();
        var service = new ConnectorPairingService(
            gateway,
            new ConnectorDeviceIdentityProvider(secrets),
            tokens,
            runtime);

        var state = await service.PairAsync(new ConnectorPairingRequest(
            new Uri("https://gateway.example"),
            "ticket-1",
            "Windows PC",
            [new ConnectorWorkspacePairing(root.Path, "My workspace")]));

        Assert.Equal("new-token", await tokens.GetAccessTokenAsync());
        Assert.Same(state, store.Value);
        Assert.Equal("device-1", state.DeviceId);
        Assert.Equal("workspace-1", state.Workspaces.Single().Id);
        Assert.Equal("device-1", gateway.MovedToDeviceId);
        Assert.Equal(0, gateway.CreatedWorkspaceCount);
    }

    [Fact]
    public async Task PairingCreatesMissingWorkspaceAndLoadsTrustBeforeCommit()
    {
        using var root = TestDirectory.Create();
        var secrets = new MemorySecretStore();
        var gateway = new FakeGateway(
            (await new ConnectorDeviceIdentityProvider(secrets).GetAsync()).PublicKey);
        var tokens = new MemoryTokenStore(null);
        var store = new MemoryStateStore();
        var runtime = new ConnectorRuntimeContext(store, tokens);
        await runtime.InitializeAsync();
        var service = new ConnectorPairingService(
            gateway,
            new ConnectorDeviceIdentityProvider(secrets),
            tokens,
            runtime);

        var state = await service.PairAsync(new ConnectorPairingRequest(
            new Uri("https://gateway.example"),
            "ticket-1",
            "Windows PC",
            [new ConnectorWorkspacePairing(root.Path)]));

        Assert.Equal(1, gateway.CreatedWorkspaceCount);
        Assert.True(state.RemoteControlTrust.RequireSignedMessages);
        Assert.StartsWith("ed25519:", gateway.RegisteredPublicKey);
    }

    [Fact]
    public async Task FailedLocalCommitRestoresPreviousToken()
    {
        using var root = TestDirectory.Create();
        var secrets = new MemorySecretStore();
        var gateway = new FakeGateway(
            (await new ConnectorDeviceIdentityProvider(secrets).GetAsync()).PublicKey);
        var tokens = new MemoryTokenStore("previous-token");
        var store = new MemoryStateStore { FailSave = true };
        var runtime = new ConnectorRuntimeContext(store, tokens);
        await runtime.InitializeAsync();
        var service = new ConnectorPairingService(
            gateway,
            new ConnectorDeviceIdentityProvider(secrets),
            tokens,
            runtime);

        await Assert.ThrowsAsync<IOException>(() => service.PairAsync(new ConnectorPairingRequest(
            new Uri("https://gateway.example"),
            "ticket-1",
            "Windows PC",
            [new ConnectorWorkspacePairing(root.Path)])));

        Assert.Equal("previous-token", await tokens.GetAccessTokenAsync());
        Assert.Null(runtime.Snapshot.State);
    }

    private sealed class FakeGateway(string publicKey) : IConnectorGatewayClient
    {
        public IReadOnlyList<ConnectorGatewayWorkspace> Workspaces { get; init; } = [];

        public int CreatedWorkspaceCount { get; private set; }

        public string? MovedToDeviceId { get; private set; }

        public string? RegisteredPublicKey { get; private set; }

        public Task<ConnectorGatewayLogin> ExchangeTicketAsync(
            Uri gatewayBaseUri,
            string ticket,
            string deviceName,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new ConnectorGatewayLogin(
                "new-token",
                new ConnectorGatewayUser("owner-1", "owner", "Owner", "user")));

        public Task<ConnectorGatewayDevice> CreateDeviceAsync(
            Uri gatewayBaseUri,
            string token,
            string displayName,
            string publicKeyValue,
            CancellationToken cancellationToken = default)
        {
            RegisteredPublicKey = publicKeyValue;
            return Task.FromResult(new ConnectorGatewayDevice(
                "device-1",
                "owner-1",
                displayName,
                publicKey,
                "online"));
        }

        public Task<ConnectorGatewayDevice?> GetDeviceAsync(
            Uri gatewayBaseUri,
            string token,
            string deviceId,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<ConnectorGatewayDevice?>(null);

        public Task DisconnectDeviceAsync(
            Uri gatewayBaseUri,
            string token,
            string deviceId,
            CancellationToken cancellationToken = default) =>
            Task.CompletedTask;

        public Task<IReadOnlyList<ConnectorGatewayWorkspace>> ListWorkspacesAsync(
            Uri gatewayBaseUri,
            string token,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(Workspaces);

        public Task<ConnectorGatewayWorkspace> CreateWorkspaceAsync(
            Uri gatewayBaseUri,
            string token,
            string deviceId,
            string alias,
            string fingerprint,
            CancellationToken cancellationToken = default)
        {
            CreatedWorkspaceCount++;
            return Task.FromResult(new ConnectorGatewayWorkspace(
                "created-workspace",
                deviceId,
                alias,
                fingerprint));
        }

        public Task<ConnectorGatewayWorkspace> MoveWorkspaceAsync(
            Uri gatewayBaseUri,
            string token,
            string workspaceId,
            string deviceId,
            CancellationToken cancellationToken = default)
        {
            MovedToDeviceId = deviceId;
            var workspace = Workspaces.Single(item => item.Id == workspaceId);
            return Task.FromResult(workspace with { DeviceId = deviceId });
        }

        public Task<RemoteControlTrust> GetRemoteControlTrustAsync(
            Uri gatewayBaseUri,
            string token,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new RemoteControlTrust(
                true,
                300,
                new Dictionary<string, string> { ["relay-1"] = "ed25519:key" }));
    }

    private sealed class MemoryStateStore : IConnectorPersistentStateStore
    {
        public ConnectorPersistentState? Value { get; private set; }

        public bool FailSave { get; init; }

        public Task<ConnectorPersistentState?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(Value);

        public Task SaveAsync(
            ConnectorPersistentState? state,
            CancellationToken cancellationToken = default)
        {
            if (FailSave)
            {
                throw new IOException("disk unavailable");
            }

            Value = state;
            return Task.CompletedTask;
        }
    }

    private sealed class MemoryTokenStore(string? token) : IConnectorAccessTokenStore
    {
        private string? _token = token;

        public ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(_token);

        public ValueTask SetAccessTokenAsync(
            string tokenValue,
            CancellationToken cancellationToken = default)
        {
            _token = tokenValue;
            return ValueTask.CompletedTask;
        }

        public ValueTask ClearAsync(CancellationToken cancellationToken = default)
        {
            _token = null;
            return ValueTask.CompletedTask;
        }
    }

    private sealed class MemorySecretStore : IConnectorSecretStore
    {
        private readonly Dictionary<string, string> _values = new(StringComparer.Ordinal);

        public ValueTask<string?> GetAsync(
            string key,
            CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(_values.GetValueOrDefault(key));

        public ValueTask SetAsync(
            string key,
            string value,
            CancellationToken cancellationToken = default)
        {
            _values[key] = value;
            return ValueTask.CompletedTask;
        }

        public ValueTask DeleteAsync(
            string key,
            CancellationToken cancellationToken = default)
        {
            _values.Remove(key);
            return ValueTask.CompletedTask;
        }
    }

    private sealed class TestDirectory : IDisposable
    {
        private TestDirectory(string path)
        {
            Path = path;
        }

        public string Path { get; }

        public static TestDirectory Create()
        {
            var path = System.IO.Path.Combine(
                System.IO.Path.GetTempPath(),
                $"chatos-pairing-{Guid.NewGuid():N}");
            Directory.CreateDirectory(path);
            return new TestDirectory(path);
        }

        public void Dispose() => Directory.Delete(Path, recursive: true);
    }
}
