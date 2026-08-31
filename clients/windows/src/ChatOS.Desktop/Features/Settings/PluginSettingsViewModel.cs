using System.Collections.ObjectModel;
using System.Text.Json.Serialization;
using ChatOS.Connector.Plugins;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;

namespace ChatOS.Desktop.Features.Settings;

public sealed partial class PluginSettingsViewModel : ObservableObject
{
    private readonly ILocalPluginManagementService _management;
    private readonly IPluginConfigurationService _configuration;
    private readonly LocalizationViewModel _localization;
    private readonly IUiDispatcher _dispatcher;
    private readonly SemaphoreSlim _operationGate = new(1, 1);

    public PluginSettingsViewModel(
        ILocalPluginManagementService management,
        IPluginConfigurationService configuration,
        LocalizationViewModel localization,
        IUiDispatcher dispatcher)
    {
        _management = management;
        _configuration = configuration;
        _localization = localization;
        _dispatcher = dispatcher;
        _localization.PropertyChanged += (_, _) =>
        {
            foreach (var plugin in Plugins)
            {
                plugin.ApplyLabels(_localization);
            }

            OnPropertyChanged(string.Empty);
        };
    }

    public ObservableCollection<PluginSettingsItem> Plugins { get; } = [];

    public string SectionDescription => _localization.Text(
        "管理这台 Windows 设备上的插件、权限、凭据和账号授权。Secret 只保存到 Windows 凭据管理器。",
        "Manage plugins, permissions, credentials, and account authorization on this Windows device. Secrets are stored only in Windows Credential Manager.");

    public string RefreshLabel => _localization.Text("刷新插件", "Refresh plugins");

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanRefresh))]
    private bool _isBusy;

    public bool CanRefresh => !IsBusy;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string? _actionMessage;

    public Task OpenAsync(CancellationToken cancellationToken = default) => RefreshAsync(cancellationToken);

    public Task RefreshAsync(CancellationToken cancellationToken = default) =>
        RunAsync(null, async token =>
        {
            var plugins = await _management.ListAsync(token).ConfigureAwait(false);
            var items = new List<PluginSettingsItem>(plugins.Count);
            foreach (var plugin in plugins)
            {
                PluginConfigurationSnapshot? configuration = null;
                string? configurationError = null;
                if (plugin.Installed)
                {
                    try
                    {
                        configuration = await _configuration.GetAsync(plugin.PluginId, token)
                            .ConfigureAwait(false);
                    }
                    catch (Exception exception) when (exception is not OperationCanceledException)
                    {
                        configurationError = exception.Message;
                    }
                }

                items.Add(new PluginSettingsItem(
                    plugin,
                    configuration,
                    configurationError,
                    _localization));
            }

            await _dispatcher.InvokeAsync(() =>
            {
                Plugins.Clear();
                foreach (var item in items)
                {
                    Plugins.Add(item);
                }
            }, token).ConfigureAwait(false);
        }, cancellationToken);

    public Task InstallOrUpdateAsync(PluginSettingsItem item, CancellationToken cancellationToken = default) =>
        RunAsync(item, async token =>
        {
            await _management.InstallAsync(item.PluginId, token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => ActionMessage = _localization.Text(
                $"{item.DisplayName} 已安装并完成完整性校验。",
                $"{item.DisplayName} was installed and passed integrity validation."), token).ConfigureAwait(false);
        }, cancellationToken, refreshAfter: true);

    public Task UninstallAsync(PluginSettingsItem item, CancellationToken cancellationToken = default) =>
        RunAsync(item, async token =>
        {
            await _management.UninstallAsync(item.PluginId, token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => ActionMessage = _localization.Text(
                $"{item.DisplayName} 已卸载，本机 Secret 和 OAuth 授权也已清除。",
                $"{item.DisplayName} was uninstalled; local secrets and OAuth authorization were also removed."), token).ConfigureAwait(false);
        }, cancellationToken, refreshAfter: true);

    public Task SetEnabledAsync(
        PluginSettingsItem item,
        bool enabled,
        CancellationToken cancellationToken = default) => RunAsync(item, async token =>
    {
        await _management.SetEnabledAsync(item.PluginId, enabled, token).ConfigureAwait(false);
        await _dispatcher.InvokeAsync(() => item.Enabled = enabled, token).ConfigureAwait(false);
    }, cancellationToken);

    public Task SaveCredentialAsync(
        PluginCredentialSettingsItem credential,
        CancellationToken cancellationToken = default) => RunAsync(
        Plugins.FirstOrDefault(value => value.PluginId == credential.PluginId),
        async token =>
        {
            var value = credential.DraftSecret;
            if (string.IsNullOrEmpty(value))
            {
                throw new InvalidOperationException(_localization.Text(
                    "请输入 Secret 后再保存。",
                    "Enter a secret before saving."));
            }

            await _configuration.SetCredentialAsync(
                credential.PluginId,
                credential.ComponentKey,
                credential.SecretName,
                value,
                token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => credential.DraftSecret = string.Empty, token)
                .ConfigureAwait(false);
        }, cancellationToken, refreshAfter: true);

    public Task DeleteCredentialAsync(
        PluginCredentialSettingsItem credential,
        CancellationToken cancellationToken = default) => RunAsync(
        Plugins.FirstOrDefault(value => value.PluginId == credential.PluginId),
        token => _configuration.DeleteCredentialAsync(
            credential.PluginId,
            credential.ComponentKey,
            credential.SecretName,
            token),
        cancellationToken,
        refreshAfter: true);

    public Task BeginOAuthAsync(
        PluginOAuthSettingsItem app,
        CancellationToken cancellationToken = default) => RunAsync(
        Plugins.FirstOrDefault(value => value.PluginId == app.PluginId),
        async token =>
        {
            var started = await _configuration.BeginOAuthAsync(
                app.PluginId,
                app.ComponentKey,
                token).ConfigureAwait(false);
            await _dispatcher.InvokeAsync(() => ActionMessage = started.BrowserOpened
                ? _localization.Text(
                    "已在默认浏览器打开授权页面；完成后点击刷新查看连接状态。",
                    "The authorization page opened in your default browser. Refresh after completing it.")
                : _localization.Text(
                    $"无法自动打开浏览器，请手动访问：{started.AuthorizationUrl}",
                    $"The browser could not be opened. Visit this URL manually: {started.AuthorizationUrl}"), token)
                .ConfigureAwait(false);
        }, cancellationToken);

    public Task DisconnectOAuthAsync(
        PluginOAuthSettingsItem app,
        CancellationToken cancellationToken = default) => RunAsync(
        Plugins.FirstOrDefault(value => value.PluginId == app.PluginId),
        token => app.ConnectionId is null
            ? Task.CompletedTask
            : _configuration.DisconnectOAuthAsync(app.ConnectionId, token),
        cancellationToken,
        refreshAfter: true);

    private async Task RunAsync(
        PluginSettingsItem? item,
        Func<CancellationToken, Task> operation,
        CancellationToken cancellationToken,
        bool refreshAfter = false)
    {
        var succeeded = false;
        await _operationGate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await _dispatcher.InvokeAsync(() =>
            {
                IsBusy = true;
                if (item is not null) item.IsBusy = true;
                ErrorMessage = null;
                if (!refreshAfter) ActionMessage = null;
            }, cancellationToken).ConfigureAwait(false);
            await operation(cancellationToken).ConfigureAwait(false);
            succeeded = true;
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            await _dispatcher.InvokeAsync(() => ErrorMessage = exception.Message).ConfigureAwait(false);
        }
        finally
        {
            await _dispatcher.InvokeAsync(() =>
            {
                IsBusy = false;
                if (item is not null) item.IsBusy = false;
            }).ConfigureAwait(false);
            _operationGate.Release();
        }

        if (refreshAfter && succeeded && !cancellationToken.IsCancellationRequested)
        {
            var successMessage = ActionMessage;
            await RefreshAsync(cancellationToken).ConfigureAwait(false);
            if (successMessage is not null)
            {
                await _dispatcher.InvokeAsync(() => ActionMessage = successMessage, cancellationToken)
                    .ConfigureAwait(false);
            }
        }
    }
}

public sealed partial class PluginSettingsItem : ObservableObject
{
    public PluginSettingsItem(
        LocalConnectorPlugin plugin,
        PluginConfigurationSnapshot? configuration,
        string? configurationError,
        LocalizationViewModel localization)
    {
        PluginId = plugin.PluginId;
        DisplayName = plugin.DisplayName;
        Description = plugin.Description;
        Category = plugin.Category;
        Publisher = plugin.Publisher;
        LatestVersion = plugin.LatestVersion;
        Installed = plugin.Installed;
        UpdateAvailable = plugin.UpdateAvailable;
        InstallAvailable = plugin.InstallAvailable;
        Enabled = plugin.Enabled;
        ConfigurationError = configurationError;
        if (configuration is not null)
        {
            foreach (var component in configuration.Components)
            {
                foreach (var permission in component.Permissions)
                {
                    Permissions.Add(new PluginPermissionSettingsItem(
                        component.ComponentKey,
                        permission.Permission,
                        permission.Required,
                        permission.Reason,
                        localization));
                }

                foreach (var credential in component.Credentials)
                {
                    Credentials.Add(new PluginCredentialSettingsItem(
                        PluginId,
                        credential,
                        localization));
                }
            }

            foreach (var app in configuration.OAuthApps)
            {
                OAuthApps.Add(new PluginOAuthSettingsItem(PluginId, app, localization));
            }
        }

        ApplyLabels(localization);
    }

    public string PluginId { get; }
    public string DisplayName { get; }
    public string Description { get; }
    public string Category { get; }
    public string Publisher { get; }
    public string LatestVersion { get; }
    public bool Installed { get; }
    public bool UpdateAvailable { get; }
    public bool InstallAvailable { get; }
    public string? ConfigurationError { get; }
    public ObservableCollection<PluginPermissionSettingsItem> Permissions { get; } = [];
    public ObservableCollection<PluginCredentialSettingsItem> Credentials { get; } = [];
    public ObservableCollection<PluginOAuthSettingsItem> OAuthApps { get; } = [];
    public bool HasPermissions => Permissions.Count > 0;
    public bool HasCredentials => Credentials.Count > 0;
    public bool HasOAuthApps => OAuthApps.Count > 0;

    [ObservableProperty]
    private bool _enabled;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsAvailable))]
    private bool _isBusy;

    public bool IsAvailable => !IsBusy;

    [ObservableProperty]
    private string _installationStatus = string.Empty;

    [ObservableProperty]
    private string _primaryActionLabel = string.Empty;

    [ObservableProperty]
    private string _uninstallLabel = string.Empty;

    [ObservableProperty]
    private string _permissionsLabel = string.Empty;

    [ObservableProperty]
    private string _credentialsLabel = string.Empty;

    [ObservableProperty]
    private string _oAuthLabel = string.Empty;

    public void ApplyLabels(LocalizationViewModel localization)
    {
        InstallationStatus = Installed
            ? UpdateAvailable
                ? localization.Text($"已安装 · 可更新到 {LatestVersion}", $"Installed · Update available: {LatestVersion}")
                : localization.Text($"已安装 · {LatestVersion}", $"Installed · {LatestVersion}")
            : localization.Text("未安装", "Not installed");
        PrimaryActionLabel = UpdateAvailable
            ? localization.Text("更新", "Update")
            : localization.Text("安装", "Install");
        UninstallLabel = localization.Text("卸载", "Uninstall");
        PermissionsLabel = localization.Text("权限", "Permissions");
        CredentialsLabel = localization.Text("Secret 配置", "Secret configuration");
        OAuthLabel = localization.Text("账号授权", "Account authorization");
        foreach (var credential in Credentials) credential.ApplyLabels(localization);
        foreach (var app in OAuthApps) app.ApplyLabels(localization);
        foreach (var permission in Permissions) permission.ApplyLabels(localization);
    }
}

public sealed partial class PluginPermissionSettingsItem : ObservableObject
{
    public PluginPermissionSettingsItem(
        string componentKey,
        string permission,
        bool required,
        string? reason,
        LocalizationViewModel localization)
    {
        ComponentKey = componentKey;
        Permission = permission;
        Required = required;
        Reason = reason;
        ApplyLabels(localization);
    }

    public string ComponentKey { get; }
    public string Permission { get; }
    public bool Required { get; }
    public string? Reason { get; }

    [ObservableProperty]
    private string _displayText = string.Empty;

    public void ApplyLabels(LocalizationViewModel localization) => DisplayText =
        $"{ComponentKey} · {Permission} · {(Required ? localization.Text("必需", "Required") : localization.Text("可选", "Optional"))}";
}

public sealed partial class PluginCredentialSettingsItem : ObservableObject
{
    public PluginCredentialSettingsItem(
        string pluginId,
        PluginCredentialConfiguration credential,
        LocalizationViewModel localization)
    {
        PluginId = pluginId;
        ComponentKey = credential.ComponentKey;
        SecretName = credential.SecretName;
        Configured = credential.Configured;
        UpdatedAt = credential.UpdatedAt;
        ApplyLabels(localization);
    }

    public string PluginId { get; }
    public string ComponentKey { get; }
    public string SecretName { get; }
    public bool Configured { get; }
    public DateTimeOffset? UpdatedAt { get; }

    [ObservableProperty]
    [property: JsonIgnore]
    private string _draftSecret = string.Empty;

    [ObservableProperty]
    private string _statusLabel = string.Empty;

    [ObservableProperty]
    private string _saveLabel = string.Empty;

    [ObservableProperty]
    private string _deleteLabel = string.Empty;

    public void ApplyLabels(LocalizationViewModel localization)
    {
        StatusLabel = Configured
            ? localization.Text("已配置", "Configured")
            : localization.Text("未配置", "Not configured");
        SaveLabel = localization.Text("保存", "Save");
        DeleteLabel = localization.Text("清除", "Remove");
    }
}

public sealed partial class PluginOAuthSettingsItem : ObservableObject
{
    public PluginOAuthSettingsItem(
        string pluginId,
        PluginOAuthAppConfiguration app,
        LocalizationViewModel localization)
    {
        PluginId = pluginId;
        ComponentKey = app.ComponentKey;
        Provider = app.Provider;
        Resource = app.Resource;
        Scopes = string.Join(", ", app.Scopes);
        ConnectionId = app.Connection?.Id;
        Connected = app.Connection?.Connected == true && app.Connection.NeedsAuth == false;
        NeedsAuth = app.Connection?.NeedsAuth == true;
        ApplyLabels(localization);
    }

    public string PluginId { get; }
    public string ComponentKey { get; }
    public string Provider { get; }
    public string Resource { get; }
    public string Scopes { get; }
    public string? ConnectionId { get; }
    public bool Connected { get; }
    public bool NeedsAuth { get; }

    [ObservableProperty]
    private string _statusLabel = string.Empty;

    [ObservableProperty]
    private string _actionLabel = string.Empty;

    public void ApplyLabels(LocalizationViewModel localization)
    {
        StatusLabel = Connected
            ? localization.Text("已连接", "Connected")
            : NeedsAuth
                ? localization.Text("需要重新授权", "Authorization required")
                : localization.Text("未连接", "Not connected");
        ActionLabel = Connected
            ? localization.Text("断开", "Disconnect")
            : localization.Text("授权", "Authorize");
    }
}
