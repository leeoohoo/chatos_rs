using ChatOS.Connector.Sandbox;
using ChatOS.Connector.NetworkGuard;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Runtime;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed class SandboxSettingsViewModelTests
{
    [Fact]
    public async Task LoadsAndSavesRealSandboxPolicy()
    {
        var store = new MemoryStore(ConnectorSandboxSettings.Default);
        var viewModel = Create(store);
        await viewModel.OpenAsync();
        viewModel.PermissionProfile = ConnectorSandboxPermissionProfile.ReadOnly;
        viewModel.NetworkAccess = ConnectorSandboxNetworkAccess.Host;

        await viewModel.SaveAsync(fullAccessConfirmed: false);

        Assert.Equal(
            new ConnectorSandboxSettings(true, ConnectorSandboxPermissionProfile.ReadOnly, ConnectorSandboxNetworkAccess.Host),
            store.Settings);
        Assert.Contains("AppContainer", viewModel.SectionDescription, StringComparison.Ordinal);
    }

    [Fact]
    public async Task FullAccessRequiresExplicitConfirmation()
    {
        var store = new MemoryStore(ConnectorSandboxSettings.Default);
        var viewModel = Create(store);
        await viewModel.OpenAsync();
        viewModel.IsEnabled = false;
        viewModel.PermissionProfile = ConnectorSandboxPermissionProfile.FullAccess;

        await viewModel.SaveAsync(fullAccessConfirmed: false);

        Assert.NotNull(viewModel.ErrorMessage);
        Assert.Equal(ConnectorSandboxSettings.Default, store.Settings);
        await viewModel.SaveAsync(fullAccessConfirmed: true);
        Assert.False(store.Settings.Enabled);
        Assert.Equal(ConnectorSandboxNetworkAccess.Host, store.Settings.NetworkAccess);
    }

    [Fact]
    public async Task ControlledNetworkIsAvailableAndSavableOnlyWhenGuardIsReady()
    {
        var store = new MemoryStore(ConnectorSandboxSettings.Default);
        var viewModel = Create(store, new ReadyGuardClient());
        await viewModel.OpenAsync();
        viewModel.NetworkAccess = ConnectorSandboxNetworkAccess.Controlled;

        await viewModel.SaveAsync(fullAccessConfirmed: false);

        Assert.True(viewModel.IsControlledNetworkAvailable);
        Assert.Equal(ConnectorSandboxNetworkAccess.Controlled, store.Settings.NetworkAccess);
        Assert.Null(viewModel.ErrorMessage);
    }

    [Fact]
    public async Task ControlledNetworkCannotBeSelectedWithoutServerManagedAllowlist()
    {
        var store = new MemoryStore(ConnectorSandboxSettings.Default);
        var viewModel = Create(
            store,
            new ReadyGuardClient(),
            new FixedCloudReadiness("managed_allowlist_not_configured"));
        await viewModel.OpenAsync();
        viewModel.NetworkAccess = ConnectorSandboxNetworkAccess.Controlled;

        await viewModel.SaveAsync(fullAccessConfirmed: false);

        Assert.False(viewModel.IsControlledNetworkAvailable);
        Assert.Equal(ConnectorSandboxNetworkAccess.Disabled, store.Settings.NetworkAccess);
        Assert.Contains("白名单", viewModel.ErrorMessage, StringComparison.Ordinal);
    }

    private static SandboxSettingsViewModel Create(
        IConnectorSandboxSettingsStore store,
        IControlledNetworkGuardClient? networkGuard = null,
        IConnectorControlledNetworkReadinessService? cloudReadiness = null)
    {
        var dispatcher = new ImmediateUiDispatcher();
        var localization = new LocalizationViewModel(
            new AppPreferencesManager(new MemoryPreferencesStore()),
            dispatcher);
        return new SandboxSettingsViewModel(
            store,
            localization,
            dispatcher,
            networkGuard,
            cloudReadiness);
    }

    private sealed class MemoryStore(ConnectorSandboxSettings settings)
        : IConnectorSandboxSettingsStore
    {
        public ConnectorSandboxSettings Settings { get; private set; } = settings;

        public Task<ConnectorSandboxSettings> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(Settings);

        public Task SaveAsync(ConnectorSandboxSettings value, CancellationToken cancellationToken = default)
        {
            Settings = value.Normalize();
            return Task.CompletedTask;
        }
    }

    private sealed class MemoryPreferencesStore : IAppPreferencesStore
    {
        public Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<AppPreferences?>(AppPreferences.Default);

        public Task SaveAsync(AppPreferences preferences, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
    }

    private sealed class ReadyGuardClient : IControlledNetworkGuardClient
    {
        public Task<NetworkGuardReadiness> CheckReadinessAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new NetworkGuardReadiness(NetworkGuardReadinessState.Ready));

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

    private sealed class FixedCloudReadiness(string state)
        : IConnectorControlledNetworkReadinessService
    {
        public Task<ConnectorControlledNetworkReadiness> CheckAsync(
            CancellationToken cancellationToken = default) =>
            Task.FromResult(new ConnectorControlledNetworkReadiness(false, state, null, 0));
    }
}
