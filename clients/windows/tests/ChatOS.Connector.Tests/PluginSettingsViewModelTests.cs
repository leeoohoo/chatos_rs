using System.Text.Json;
using ChatOS.Connector.Plugins;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed class PluginSettingsViewModelTests
{
    [Fact]
    public async Task Install_refreshes_the_catalog_and_preserves_success_feedback()
    {
        var management = new FakeManagementService();
        var configuration = new FakeConfigurationService();
        var viewModel = CreateViewModel(management, configuration);

        await viewModel.OpenAsync();
        var plugin = Assert.Single(viewModel.Plugins);
        Assert.False(plugin.Installed);

        await viewModel.InstallOrUpdateAsync(plugin);

        Assert.Equal(["plugin.one"], management.InstalledPluginIds);
        Assert.True(Assert.Single(viewModel.Plugins).Installed);
        Assert.Contains("完整性校验", viewModel.ActionMessage);
        Assert.Null(viewModel.ErrorMessage);
        Assert.False(viewModel.IsBusy);
    }

    [Fact]
    public async Task Uninstall_refreshes_the_catalog()
    {
        var management = new FakeManagementService { Installed = true };
        var viewModel = CreateViewModel(management, new FakeConfigurationService());
        await viewModel.OpenAsync();

        await viewModel.UninstallAsync(Assert.Single(viewModel.Plugins));

        Assert.Equal(["plugin.one"], management.UninstalledPluginIds);
        Assert.False(Assert.Single(viewModel.Plugins).Installed);
        Assert.Contains("已卸载", viewModel.ActionMessage);
    }

    [Fact]
    public async Task Enable_failure_keeps_the_confirmed_state_and_surfaces_the_error()
    {
        var management = new FakeManagementService
        {
            Installed = true,
            Enabled = true,
            SetEnabledError = new InvalidOperationException("preference rejected"),
        };
        var viewModel = CreateViewModel(management, new FakeConfigurationService());
        await viewModel.OpenAsync();
        var plugin = Assert.Single(viewModel.Plugins);

        await viewModel.SetEnabledAsync(plugin, false);

        Assert.True(plugin.Enabled);
        Assert.Equal("preference rejected", viewModel.ErrorMessage);
        Assert.False(plugin.IsBusy);
        Assert.False(viewModel.IsBusy);
    }

    [Fact]
    public async Task Save_secret_clears_the_draft_and_never_serializes_it()
    {
        var configuration = new FakeConfigurationService { CredentialConfigured = false };
        var management = new FakeManagementService { Installed = true };
        var viewModel = CreateViewModel(management, configuration);
        await viewModel.OpenAsync();
        var credential = Assert.Single(Assert.Single(viewModel.Plugins).Credentials);
        credential.DraftSecret = "top-secret-value";

        var jsonBeforeSave = JsonSerializer.Serialize(credential);
        await viewModel.SaveCredentialAsync(credential);

        Assert.DoesNotContain("top-secret-value", jsonBeforeSave, StringComparison.Ordinal);
        Assert.Equal("top-secret-value", configuration.SavedSecret);
        Assert.Equal(string.Empty, credential.DraftSecret);
        Assert.True(Assert.Single(Assert.Single(viewModel.Plugins).Credentials).Configured);
    }

    [Fact]
    public async Task OAuth_authorize_and_disconnect_use_the_selected_connected_app()
    {
        var configuration = new FakeConfigurationService();
        var management = new FakeManagementService { Installed = true };
        var viewModel = CreateViewModel(management, configuration);
        await viewModel.OpenAsync();
        var app = Assert.Single(Assert.Single(viewModel.Plugins).OAuthApps);

        await viewModel.BeginOAuthAsync(app);

        Assert.Equal(("plugin.one", "mcp"), configuration.OAuthStart);
        Assert.Contains("默认浏览器", viewModel.ActionMessage);

        configuration.Connected = true;
        await viewModel.RefreshAsync();
        app = Assert.Single(Assert.Single(viewModel.Plugins).OAuthApps);
        await viewModel.DisconnectOAuthAsync(app);

        Assert.Equal("connection-one", configuration.DisconnectedConnectionId);
        Assert.False(Assert.Single(Assert.Single(viewModel.Plugins).OAuthApps).Connected);
    }

    [Fact]
    public async Task Refresh_keeps_catalog_visible_when_one_plugin_configuration_is_invalid()
    {
        var configuration = new FakeConfigurationService
        {
            GetError = new InvalidOperationException("manifest invalid"),
        };
        var viewModel = CreateViewModel(
            new FakeManagementService { Installed = true },
            configuration);

        await viewModel.RefreshAsync();

        var plugin = Assert.Single(viewModel.Plugins);
        Assert.Equal("manifest invalid", plugin.ConfigurationError);
        Assert.Null(viewModel.ErrorMessage);
        Assert.Empty(plugin.Credentials);
    }

    private static PluginSettingsViewModel CreateViewModel(
        ILocalPluginManagementService management,
        IPluginConfigurationService configuration)
    {
        var dispatcher = new ImmediateUiDispatcher();
        var preferences = new AppPreferencesManager(new MemoryPreferencesStore());
        var localization = new LocalizationViewModel(preferences, dispatcher);
        return new PluginSettingsViewModel(management, configuration, localization, dispatcher);
    }

    private sealed class MemoryPreferencesStore : IAppPreferencesStore
    {
        public Task<AppPreferences?> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult<AppPreferences?>(AppPreferences.Default);

        public Task SaveAsync(
            AppPreferences preferences,
            CancellationToken cancellationToken = default) => Task.CompletedTask;
    }

    private sealed class FakeManagementService : ILocalPluginManagementService
    {
        public bool Installed { get; set; }
        public bool Enabled { get; set; } = true;
        public Exception? SetEnabledError { get; set; }
        public List<string> InstalledPluginIds { get; } = [];
        public List<string> UninstalledPluginIds { get; } = [];

        public Task<IReadOnlyList<LocalConnectorPlugin>> ListAsync(
            CancellationToken cancellationToken = default) => Task.FromResult<IReadOnlyList<LocalConnectorPlugin>>(
            [new LocalConnectorPlugin(
                "plugin.one",
                "Plugin One",
                "Test plugin",
                "Tools",
                "ChatOS",
                "1.2.3",
                Installed,
                false,
                true,
                Enabled,
                ["network.domain:api.example.com"])]);

        public Task<InstalledPluginRecord> InstallAsync(
            string pluginId,
            CancellationToken cancellationToken = default)
        {
            InstalledPluginIds.Add(pluginId);
            Installed = true;
            return Task.FromResult(new InstalledPluginRecord(
                pluginId,
                "release-one",
                "1.2.3",
                new string('a', 64),
                "C:\\Plugins\\plugin.one",
                DateTimeOffset.UtcNow,
                [],
                new Dictionary<string, string>()));
        }

        public Task UninstallAsync(
            string pluginId,
            CancellationToken cancellationToken = default)
        {
            UninstalledPluginIds.Add(pluginId);
            Installed = false;
            return Task.CompletedTask;
        }

        public Task SetEnabledAsync(
            string pluginId,
            bool enabled,
            CancellationToken cancellationToken = default)
        {
            if (SetEnabledError is not null) return Task.FromException(SetEnabledError);
            Enabled = enabled;
            return Task.CompletedTask;
        }
    }

    private sealed class FakeConfigurationService : IPluginConfigurationService
    {
        public bool CredentialConfigured { get; set; }
        public bool Connected { get; set; }
        public Exception? GetError { get; set; }
        public string? SavedSecret { get; private set; }
        public (string PluginId, string ComponentKey)? OAuthStart { get; private set; }
        public string? DisconnectedConnectionId { get; private set; }

        public Task<PluginConfigurationSnapshot> GetAsync(
            string pluginId,
            CancellationToken cancellationToken = default)
        {
            if (GetError is not null) return Task.FromException<PluginConfigurationSnapshot>(GetError);
            var connection = Connected
                ? new PluginOAuthConnection(
                    "connection-one",
                    "user-one",
                    "device-one",
                    pluginId,
                    "release-one",
                    "mcp",
                    "example",
                    "https://api.example.com/",
                    ["read"],
                    true,
                    false,
                    DateTimeOffset.UtcNow.AddHours(1),
                    "Example User",
                    DateTimeOffset.UtcNow)
                : null;
            return Task.FromResult(new PluginConfigurationSnapshot(
                pluginId,
                "release-one",
                "1.2.3",
                [new PluginMcpComponentConfiguration(
                    "mcp",
                    "http",
                    "https://api.example.com/",
                    [new PluginPermissionConfiguration(
                        "network.domain:api.example.com",
                        true,
                        "Connect to the service")],
                    [new PluginCredentialConfiguration(
                        "mcp",
                        "API_TOKEN",
                        CredentialConfigured,
                        CredentialConfigured ? DateTimeOffset.UtcNow : null)])],
                [new PluginOAuthAppConfiguration(
                    "mcp",
                    "example",
                    "https://api.example.com/",
                    ["read"],
                    connection)]));
        }

        public Task SetCredentialAsync(
            string pluginId,
            string componentKey,
            string secretName,
            string value,
            CancellationToken cancellationToken = default)
        {
            SavedSecret = value;
            CredentialConfigured = true;
            return Task.CompletedTask;
        }

        public Task DeleteCredentialAsync(
            string pluginId,
            string componentKey,
            string secretName,
            CancellationToken cancellationToken = default)
        {
            CredentialConfigured = false;
            return Task.CompletedTask;
        }

        public Task<PluginOAuthAuthorizationStart> BeginOAuthAsync(
            string pluginId,
            string componentKey,
            CancellationToken cancellationToken = default)
        {
            OAuthStart = (pluginId, componentKey);
            return Task.FromResult(new PluginOAuthAuthorizationStart(
                "transaction-one",
                new Uri("https://accounts.example.com/authorize"),
                DateTimeOffset.UtcNow.AddMinutes(5),
                true,
                null));
        }

        public Task DisconnectOAuthAsync(
            string connectionId,
            CancellationToken cancellationToken = default)
        {
            DisconnectedConnectionId = connectionId;
            Connected = false;
            return Task.CompletedTask;
        }
    }
}
