using ChatOS.Connector.Connection;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Security;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class LocalConnectorControlServiceTests
{
    [Fact]
    public async Task PairAndDisconnectKeepAuthoritativeRuntimeStateInSync()
    {
        var root = Path.Combine(Path.GetTempPath(), $"chatos-control-{Guid.NewGuid():N}");
        Directory.CreateDirectory(root);
        try
        {
            var connectorTokens = new MemoryConnectorTokenStore();
            var stateStore = new MemoryStateStore();
            var runtime = new ConnectorRuntimeContext(stateStore, connectorTokens);
            var identity = new ConnectorDeviceIdentityProvider(new MemorySecretStore());
            var gateway = new GatewayDouble();
            var connection = new ConnectorConnectionStateMachine();
            var pairing = new ConnectorPairingService(gateway, identity, connectorTokens, runtime);
            var service = new LocalConnectorControlService(
                runtime,
                pairing,
                connection,
                gateway,
                connectorTokens);

            var paired = await service.PairAsync(
                new LocalConnectorPairingDraft(
                    "https://gateway.example",
                    "Windows PC",
                    [new LocalConnectorWorkspaceDraft(root, "Workspace")]),
                "ticket-1");

            Assert.True(paired.IsPaired);
            Assert.Equal("Stopped", paired.ConnectionPhase);
            Assert.Equal("gateway-token", connectorTokens.Token);
            Assert.Equal("device-1", paired.DeviceId);
            Assert.Single(paired.Workspaces);

            await service.DisconnectAsync();
            var disconnected = await service.GetStatusAsync();

            Assert.False(disconnected.IsPaired);
            Assert.Equal("Unconfigured", disconnected.ConnectionPhase);
            Assert.Null(connectorTokens.Token);
            Assert.Equal("device-1", gateway.DisconnectedDeviceId);
            Assert.Null(stateStore.Value);
        }
        finally
        {
            Directory.Delete(root, recursive: true);
        }
    }

    [Fact]
    public async Task DisconnectClearsLocalPairingWhenGatewayIsUnavailable()
    {
        var connectorTokens = new MemoryConnectorTokenStore("gateway-token");
        var stateStore = new MemoryStateStore { Value = State(Path.GetTempPath()) };
        var runtime = new ConnectorRuntimeContext(stateStore, connectorTokens);
        var gateway = new GatewayDouble { DisconnectError = new HttpRequestException("offline") };
        var connection = new ConnectorConnectionStateMachine();
        var pairing = new ConnectorPairingService(
            gateway,
            new ConnectorDeviceIdentityProvider(new MemorySecretStore()),
            connectorTokens,
            runtime);
        var service = new LocalConnectorControlService(
            runtime, pairing, connection, gateway, connectorTokens);

        var error = await Assert.ThrowsAsync<InvalidOperationException>(() => service.DisconnectAsync());

        Assert.Contains("本机配对已清除", error.Message);
        Assert.Null(stateStore.Value);
        Assert.Null(connectorTokens.Token);
        Assert.False((await service.GetStatusAsync()).IsPaired);
    }

    private static ConnectorPersistentState State(string root) => new(
        new Uri("https://gateway.example"),
        new ConnectorUser("owner-1", "owner", "Owner", "user"),
        "device-1",
        "Windows PC",
        [new ChatOS.Connector.Workspaces.ConnectorWorkspace(
            "workspace-1", "Workspace", root, "fingerprint")],
        new RemoteControlTrust(true, 300, new Dictionary<string, string>()));

    private sealed class GatewayDouble : IConnectorGatewayClient
    {
        public Exception? DisconnectError { get; init; }

        public string? DisconnectedDeviceId { get; private set; }

        public Task<ConnectorGatewayLogin> ExchangeTicketAsync(
            Uri gatewayBaseUri,
            string ticket,
            string deviceName,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new ConnectorGatewayLogin(
                "gateway-token",
                new ConnectorGatewayUser("owner-1", "owner", "Owner", "user")));

        public Task<ConnectorGatewayDevice> CreateDeviceAsync(
            Uri gatewayBaseUri,
            string token,
            string displayName,
            string publicKey,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new ConnectorGatewayDevice(
                "device-1", "owner-1", displayName, publicKey, "online"));

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
            CancellationToken cancellationToken = default)
        {
            DisconnectedDeviceId = deviceId;
            return DisconnectError is null ? Task.CompletedTask : Task.FromException(DisconnectError);
        }

        public Task<IReadOnlyList<ConnectorGatewayWorkspace>> ListWorkspacesAsync(
            Uri gatewayBaseUri,
            string token,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ConnectorGatewayWorkspace>>([]);

        public Task<ConnectorGatewayWorkspace> CreateWorkspaceAsync(
            Uri gatewayBaseUri,
            string token,
            string deviceId,
            string alias,
            string fingerprint,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new ConnectorGatewayWorkspace(
                "workspace-1", deviceId, alias, fingerprint));

        public Task<ConnectorGatewayWorkspace> MoveWorkspaceAsync(
            Uri gatewayBaseUri,
            string token,
            string workspaceId,
            string deviceId,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task<RemoteControlTrust> GetRemoteControlTrustAsync(
            Uri gatewayBaseUri,
            string token,
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new RemoteControlTrust(
                true, 300, new Dictionary<string, string>()));
    }

    private sealed class MemoryStateStore : IConnectorPersistentStateStore
    {
        public ConnectorPersistentState? Value { get; set; }

        public Task<ConnectorPersistentState?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(Value);

        public Task SaveAsync(
            ConnectorPersistentState? state,
            CancellationToken cancellationToken = default)
        {
            Value = state;
            return Task.CompletedTask;
        }
    }

    private sealed class MemoryConnectorTokenStore(string? token = null) : IConnectorAccessTokenStore
    {
        public string? Token { get; private set; } = token;

        public ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(Token);

        public ValueTask SetAccessTokenAsync(
            string tokenValue,
            CancellationToken cancellationToken = default)
        {
            Token = tokenValue;
            return ValueTask.CompletedTask;
        }

        public ValueTask ClearAsync(CancellationToken cancellationToken = default)
        {
            Token = null;
            return ValueTask.CompletedTask;
        }
    }

    private sealed class MemorySecretStore : IConnectorSecretStore
    {
        private readonly Dictionary<string, string> _values = new(StringComparer.Ordinal);

        public ValueTask<string?> GetAsync(string key, CancellationToken cancellationToken = default) =>
            ValueTask.FromResult(_values.GetValueOrDefault(key));

        public ValueTask SetAsync(
            string key,
            string value,
            CancellationToken cancellationToken = default)
        {
            _values[key] = value;
            return ValueTask.CompletedTask;
        }

        public ValueTask DeleteAsync(string key, CancellationToken cancellationToken = default)
        {
            _values.Remove(key);
            return ValueTask.CompletedTask;
        }
    }
}
