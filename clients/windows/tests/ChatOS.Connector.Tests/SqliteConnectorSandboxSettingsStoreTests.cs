using ChatOS.Connector.Persistence;
using ChatOS.Connector.NetworkGuard;
using ChatOS.Connector.Sandbox;

namespace ChatOS.Connector.Tests;

public sealed class SqliteConnectorSandboxSettingsStoreTests
{
    [Fact]
    public async Task DefaultsToWorkspaceWriteWithNetworkDisabledAndRoundTrips()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"chatos-sandbox-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        try
        {
            var database = new LocalStateDatabase(Path.Combine(directory, "state.db"));
            await database.InitializeAsync();
            var store = new SqliteConnectorSandboxSettingsStore(database);

            Assert.Equal(ConnectorSandboxSettings.Default, await store.LoadAsync());

            var settings = new ConnectorSandboxSettings(
                true,
                ConnectorSandboxPermissionProfile.ReadOnly,
                ConnectorSandboxNetworkAccess.Host);
            await store.SaveAsync(settings);

            Assert.Equal(settings, await store.LoadAsync());
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task FullAccessAlwaysNormalizesToHostAccessWithoutAppContainer()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"chatos-sandbox-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        try
        {
            var database = new LocalStateDatabase(Path.Combine(directory, "state.db"));
            await database.InitializeAsync();
            var store = new SqliteConnectorSandboxSettingsStore(database);
            await store.SaveAsync(new ConnectorSandboxSettings(
                true,
                ConnectorSandboxPermissionProfile.FullAccess,
                ConnectorSandboxNetworkAccess.Disabled));

            Assert.Equal(
                new ConnectorSandboxSettings(
                    false,
                    ConnectorSandboxPermissionProfile.FullAccess,
                    ConnectorSandboxNetworkAccess.Host),
                await store.LoadAsync());
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task ControlledNetworkCannotBePersistedWithoutEnforcementBackend()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"chatos-sandbox-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        try
        {
            var database = new LocalStateDatabase(Path.Combine(directory, "state.db"));
            await database.InitializeAsync();
            var store = new SqliteConnectorSandboxSettingsStore(database);

            await Assert.ThrowsAsync<InvalidOperationException>(() => store.SaveAsync(
                new ConnectorSandboxSettings(
                    true,
                    ConnectorSandboxPermissionProfile.WorkspaceWrite,
                    ConnectorSandboxNetworkAccess.Controlled)));
            Assert.Equal(ConnectorSandboxSettings.Default, await store.LoadAsync());
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    [Fact]
    public async Task ControlledNetworkPersistsOnlyAfterNetworkGuardIsReady()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"chatos-sandbox-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        try
        {
            var database = new LocalStateDatabase(Path.Combine(directory, "state.db"));
            await database.InitializeAsync();
            var store = new SqliteConnectorSandboxSettingsStore(
                database,
                new ReadinessGuardClient(NetworkGuardReadinessState.Ready));
            var settings = new ConnectorSandboxSettings(
                true,
                ConnectorSandboxPermissionProfile.WorkspaceWrite,
                ConnectorSandboxNetworkAccess.Controlled);

            await store.SaveAsync(settings);

            Assert.Equal(settings, await store.LoadAsync());
        }
        finally
        {
            Directory.Delete(directory, recursive: true);
        }
    }

    private sealed class ReadinessGuardClient(NetworkGuardReadinessState state)
        : IControlledNetworkGuardClient
    {
        public Task<NetworkGuardReadiness> CheckReadinessAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new NetworkGuardReadiness(state));

        public Task<NetworkGuardLease> AcquireLeaseAsync(
            ChatOS.NetworkGuard.Contracts.ControlledNetworkPolicyEnvelope policy,
            string appContainerSid,
            int processId,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();

        public Task<NetworkGuardLease> RenewLeaseAsync(
            NetworkGuardLease lease,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();

        public Task ReleaseLeaseAsync(
            NetworkGuardLease lease,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();
    }
}
