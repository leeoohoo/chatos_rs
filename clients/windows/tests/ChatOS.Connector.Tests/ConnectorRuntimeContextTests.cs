using ChatOS.Api.Http;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Workspaces;

namespace ChatOS.Connector.Tests;

public sealed class ConnectorRuntimeContextTests
{
    [Fact]
    public async Task SqliteStateRoundTripsConnectorIdentityTrustAndWorkspaces()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"chatos-runtime-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        try
        {
            var database = new LocalStateDatabase(Path.Combine(directory, "state.db"));
            await database.InitializeAsync();
            var store = new SqliteConnectorPersistentStateStore(database);
            var expected = State(directory);

            await store.SaveAsync(expected);
            var actual = await store.LoadAsync();

            Assert.NotNull(actual);
            Assert.Equal(expected.DeviceId, actual.DeviceId);
            Assert.Equal(expected.User.Id, actual.User.Id);
            Assert.Equal(expected.Workspaces[0].AbsoluteRoot, actual.Workspaces[0].AbsoluteRoot);
            Assert.True(actual.RemoteControlTrust.RequireSignedMessages);

            await store.SaveAsync(null);
            Assert.Null(await store.LoadAsync());
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task RuntimePublishesOnlyPersistedStateAndBuildsSessionFromSecureToken()
    {
        var root = Path.GetTempPath();
        var store = new MemoryStateStore();
        var tokens = new MemoryTokenStore("token-1");
        var runtime = new ConnectorRuntimeContext(store, tokens);
        await runtime.InitializeAsync();
        var revision = runtime.Snapshot.Revision;
        var changed = runtime.WaitForChangeAsync(revision, TimeSpan.FromSeconds(1), CancellationToken.None);

        var state = State(root);
        await runtime.ReplaceAsync(state);

        Assert.True(await changed > revision);
        Assert.Same(state, store.Value);
        Assert.Equal("workspace-1", runtime.Find("workspace-1")?.Id);
        var security = await runtime.GetAsync(CancellationToken.None);
        Assert.Equal("owner-1", security.OwnerUserId);
        var session = await runtime.SessionConfigurationAsync();
        Assert.NotNull(session);
        Assert.Equal("token-1", session.AccessToken);
        Assert.Equal("device-1", session.DeviceId);
    }

    [Fact]
    public async Task RuntimeDoesNotCreateSessionWhenUnpairedOrSignedOut()
    {
        var store = new MemoryStateStore();
        var tokens = new MemoryTokenStore(null);
        var runtime = new ConnectorRuntimeContext(store, tokens);
        await runtime.InitializeAsync();

        Assert.Null(await runtime.SessionConfigurationAsync());
        await runtime.ReplaceAsync(State(Path.GetTempPath()));
        Assert.Null(await runtime.SessionConfigurationAsync());
        await Assert.ThrowsAsync<RelayRequestException>(async () =>
        {
            await runtime.ReplaceAsync(null);
            await runtime.GetAsync(CancellationToken.None);
        });
    }

    [Fact]
    public async Task TrustRefreshIsPersistedWithoutRestartingActiveConnectionIdentity()
    {
        var store = new MemoryStateStore();
        var runtime = new ConnectorRuntimeContext(store, new MemoryTokenStore("token"));
        await runtime.InitializeAsync();
        var state = State(Path.GetTempPath());
        await runtime.ReplaceAsync(state);
        var before = runtime.Snapshot;
        var connectionChanged = runtime.WaitForChangeAsync(
            before.ConnectionRevision,
            TimeSpan.FromMilliseconds(80),
            CancellationToken.None);

        var updated = await runtime.UpdateRemoteControlTrustAsync(
            state.GatewayBaseUri,
            state.DeviceId,
            new RemoteControlTrust(
                true,
                120,
                new Dictionary<string, string> { ["relay-key-2"] = "ed25519:new" }));

        Assert.True(updated);
        Assert.True(runtime.Snapshot.Revision > before.Revision);
        Assert.Equal(before.ConnectionRevision, runtime.Snapshot.ConnectionRevision);
        await Task.Delay(20);
        Assert.False(connectionChanged.IsCompleted);
        Assert.Equal(before.ConnectionRevision, await connectionChanged);
        Assert.Equal(120, store.Value?.RemoteControlTrust.SignatureMaxSkewSeconds);
    }

    private static ConnectorPersistentState State(string root) => new(
        new Uri("https://gateway.example"),
        new ConnectorUser("owner-1", "owner", "Owner", "user"),
        "device-1",
        "Windows PC",
        [new ConnectorWorkspace("workspace-1", "Workspace", root, "fingerprint")],
        new RemoteControlTrust(
            true,
            300,
            new Dictionary<string, string> { ["relay-key-1"] = "ed25519:key" }));

    private sealed class MemoryStateStore : IConnectorPersistentStateStore
    {
        public ConnectorPersistentState? Value { get; private set; }

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
}
