using ChatOS.Core.Domain;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Sandbox;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Windows.Storage.Pickers;

namespace ChatOS.Desktop.Features.Settings;

public sealed partial class SettingsPage : UserControl
{
    private bool _syncing;

    public SettingsPage(
        AppSettingsViewModel viewModel,
        ConnectorSettingsViewModel connector,
        PluginSettingsViewModel pluginSettings,
        ApprovalSettingsViewModel approvalSettings,
        ModelSettingsViewModel modelSettings,
        SandboxSettingsViewModel sandboxSettings,
        LocalizationViewModel localization)
    {
        ViewModel = viewModel;
        Connector = connector;
        PluginSettings = pluginSettings;
        ApprovalSettings = approvalSettings;
        ModelSettings = modelSettings;
        SandboxSettings = sandboxSettings;
        Localization = localization;
        InitializeComponent();
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
        ViewModel.PropertyChanged += (_, _) => SyncControls();
        ApprovalSettings.PropertyChanged += (_, _) => SyncControls();
        SandboxSettings.PropertyChanged += (_, _) => SyncControls();
    }

    public AppSettingsViewModel ViewModel { get; }

    public LocalizationViewModel Localization { get; }

    public ConnectorSettingsViewModel Connector { get; }

    public PluginSettingsViewModel PluginSettings { get; }

    public ApprovalSettingsViewModel ApprovalSettings { get; }

    public ModelSettingsViewModel ModelSettings { get; }

    public SandboxSettingsViewModel SandboxSettings { get; }

    public event EventHandler? CloseRequested;

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        SyncControls();
        await Task.WhenAll(
            Connector.OpenAsync(),
            PluginSettings.OpenAsync(),
            ApprovalSettings.OpenAsync(),
            ModelSettings.OpenAsync(),
            SandboxSettings.OpenAsync());
    }

    private void OnUnloaded(object sender, RoutedEventArgs e) => Connector.Close();

    private void SyncControls()
    {
        if (LanguagePicker is null)
        {
            return;
        }

        _syncing = true;
        try
        {
            LanguagePicker.SelectedIndex = ViewModel.Language == InterfaceLanguage.English ? 1 : 0;
            ThemePicker.SelectedIndex = ViewModel.Theme switch
            {
                InterfaceTheme.Light => 1,
                InterfaceTheme.Dark => 2,
                _ => 0,
            };
            FontScaleSlider.Value = ViewModel.FontScale;
            FontScaleLabel.Text = $"{ViewModel.FontScale:P0}";
            PetEnabledSwitch.IsOn = ViewModel.PetEnabled;
            ApprovalModePicker.SelectedIndex = ApprovalSettings.Mode switch
            {
                ConnectorApprovalMode.AutoApproval => 1,
                ConnectorApprovalMode.FullControl => 2,
                _ => 0,
            };
            SandboxEnabledSwitch.IsOn = SandboxSettings.IsEnabled;
            SandboxProfilePicker.SelectedIndex = SandboxSettings.PermissionProfile switch
            {
                ConnectorSandboxPermissionProfile.ReadOnly => 0,
                ConnectorSandboxPermissionProfile.FullAccess => 2,
                _ => 1,
            };
            SandboxNetworkPicker.SelectedIndex = SandboxSettings.NetworkAccess switch
            {
                ConnectorSandboxNetworkAccess.Controlled when SandboxSettings.IsControlledNetworkAvailable => 1,
                ConnectorSandboxNetworkAccess.Host => 2,
                _ => 0,
            };
        }
        finally
        {
            _syncing = false;
        }
    }

    private void OnBackClicked(object sender, RoutedEventArgs e) =>
        CloseRequested?.Invoke(this, EventArgs.Empty);

    private async void OnLanguageChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_syncing || LanguagePicker.SelectedItem is not ComboBoxItem item ||
            !Enum.TryParse<InterfaceLanguage>(item.Tag?.ToString(), out var language))
        {
            return;
        }

        await ViewModel.SetLanguageAsync(language);
    }

    private async void OnThemeChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_syncing || ThemePicker.SelectedItem is not ComboBoxItem item ||
            !Enum.TryParse<InterfaceTheme>(item.Tag?.ToString(), out var theme))
        {
            return;
        }

        await ViewModel.SetThemeAsync(theme);
    }

    private async void OnFontScaleChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        FontScaleLabel.Text = $"{e.NewValue:P0}";
        if (!_syncing)
        {
            await ViewModel.SetFontScaleAsync(e.NewValue);
        }
    }

    private async void OnPetEnabledToggled(object sender, RoutedEventArgs e)
    {
        if (!_syncing)
        {
            await ViewModel.SetPetEnabledAsync(PetEnabledSwitch.IsOn);
        }
    }

    private async void OnApprovalModeChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_syncing || ApprovalModePicker.SelectedItem is not ComboBoxItem item ||
            !Enum.TryParse<ConnectorApprovalMode>(item.Tag?.ToString(), out var mode) ||
            mode == ApprovalSettings.Mode)
        {
            return;
        }

        if (mode == ConnectorApprovalMode.RequestApproval)
        {
            await ApprovalSettings.SetModeAsync(mode, riskConfirmed: false);
            SyncApprovalMode();
            return;
        }

        var isFullControl = mode == ConnectorApprovalMode.FullControl;
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = isFullControl
                ? Localization.Text("确认授予完全控制？", "Grant full control?")
                : Localization.Text("确认启用自动审批？", "Enable automatic approval?"),
            Content = isFullControl
                ? Localization.Text(
                    "任务将可以直接执行高风险命令。请只在完全信任当前设备、项目与任务时使用。",
                    "Tasks will be able to execute high-risk commands. Use this only when you fully trust the device, project, and task.")
                : Localization.Text(
                    "审批模型会自动决定部分敏感操作，无法判断或模型不可用时仍会请求你的确认。",
                    "The approval model will decide some sensitive operations and ask you when uncertain or unavailable."),
            PrimaryButtonText = isFullControl
                ? Localization.Text("授予完全控制", "Grant full control")
                : Localization.Text("启用自动审批", "Enable automatic approval"),
            CloseButtonText = Localization.Text("取消", "Cancel"),
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await ApprovalSettings.SetModeAsync(mode, riskConfirmed: true);
        }

        SyncApprovalMode();
    }

    private async void OnRefreshModelsClicked(object sender, RoutedEventArgs e) =>
        await ModelSettings.LoadAsync();

    private async void OnSaveModelsClicked(object sender, RoutedEventArgs e) =>
        await ModelSettings.SaveAsync();

    private void OnClearApprovalModelClicked(object sender, RoutedEventArgs e) =>
        ModelSettings.SelectedApprovalModel = null;

    private void OnModelRetriesChanged(NumberBox sender, NumberBoxValueChangedEventArgs args)
    {
        if (!double.IsNaN(args.NewValue))
        {
            ModelSettings.ModelRequestMaxRetries = Math.Clamp((int)args.NewValue, 0, 10);
        }
    }

    private void OnSandboxEnabledToggled(object sender, RoutedEventArgs e)
    {
        if (!_syncing)
        {
            SandboxSettings.IsEnabled = SandboxEnabledSwitch.IsOn;
            if (!SandboxEnabledSwitch.IsOn)
            {
                SandboxSettings.PermissionProfile = ConnectorSandboxPermissionProfile.FullAccess;
                SandboxSettings.NetworkAccess = ConnectorSandboxNetworkAccess.Host;
            }
        }
    }

    private void OnSandboxProfileChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_syncing || SandboxProfilePicker.SelectedItem is not ComboBoxItem item ||
            !Enum.TryParse<ConnectorSandboxPermissionProfile>(item.Tag?.ToString(), out var profile))
        {
            return;
        }
        SandboxSettings.PermissionProfile = profile;
        SandboxSettings.IsEnabled = profile is not ConnectorSandboxPermissionProfile.FullAccess;
        if (profile is ConnectorSandboxPermissionProfile.FullAccess)
        {
            SandboxSettings.NetworkAccess = ConnectorSandboxNetworkAccess.Host;
        }
        SyncControls();
    }

    private void OnSandboxNetworkChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_syncing || SandboxNetworkPicker.SelectedItem is not ComboBoxItem item ||
            !Enum.TryParse<ConnectorSandboxNetworkAccess>(item.Tag?.ToString(), out var network))
        {
            return;
        }
        SandboxSettings.NetworkAccess = network;
    }

    private async void OnSaveSandboxClicked(object sender, RoutedEventArgs e)
    {
        var fullAccess = !SandboxSettings.IsEnabled ||
            SandboxSettings.PermissionProfile is ConnectorSandboxPermissionProfile.FullAccess;
        if (fullAccess)
        {
            var dialog = new ContentDialog
            {
                XamlRoot = XamlRoot,
                Title = Localization.Text("确认关闭命令沙箱？", "Disable command sandbox?"),
                Content = Localization.Text(
                    "命令将继承当前 Windows 用户的完整文件和网络权限。",
                    "Commands will inherit the current Windows user's full file and network permissions."),
                PrimaryButtonText = Localization.Text("确认完全访问", "Confirm full access"),
                CloseButtonText = Localization.Text("取消", "Cancel"),
                DefaultButton = ContentDialogButton.Close,
            };
            if (await dialog.ShowAsync() != ContentDialogResult.Primary)
            {
                return;
            }
        }

        await SandboxSettings.SaveAsync(fullAccessConfirmed: fullAccess);
        SyncControls();
    }

    private async void OnRefreshSandboxReadinessClicked(object sender, RoutedEventArgs e)
    {
        await SandboxSettings.RefreshReadinessAsync();
        SyncControls();
    }

    private void SyncApprovalMode()
    {
        _syncing = true;
        try
        {
            ApprovalModePicker.SelectedIndex = ApprovalSettings.Mode switch
            {
                ConnectorApprovalMode.AutoApproval => 1,
                ConnectorApprovalMode.FullControl => 2,
                _ => 0,
            };
        }
        finally
        {
            _syncing = false;
        }
    }

    private async void OnDeclinePendingApproval(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ApprovalPendingItemViewModel item })
        {
            await ApprovalSettings.ResolveAsync(item, ConnectorApprovalAction.Decline);
        }
    }

    private async void OnAcceptPendingApproval(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ApprovalPendingItemViewModel item })
        {
            await ApprovalSettings.ResolveAsync(item, ConnectorApprovalAction.Accept);
        }
    }

    private async void OnAcceptSessionPendingApproval(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: ApprovalPendingItemViewModel item })
        {
            await ApprovalSettings.ResolveAsync(item, ConnectorApprovalAction.AcceptForSession);
        }
    }

    private async void OnAddConnectorWorkspaceClicked(object sender, RoutedEventArgs e)
    {
        var picker = new FolderPicker { SuggestedStartLocation = PickerLocationId.ComputerFolder };
        picker.FileTypeFilter.Add("*");
        var window = (Application.Current as App)?.MainWindow;
        if (window is null) return;
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker,
            WinRT.Interop.WindowNative.GetWindowHandle(window));
        var folder = await picker.PickSingleFolderAsync();
        if (folder is null || string.IsNullOrWhiteSpace(folder.Path)) return;
        var alias = Path.GetFileName(Path.TrimEndingDirectorySeparator(folder.Path));
        Connector.AddWorkspaceCommand.Execute(new LocalConnectorWorkspaceDraft(
            folder.Path,
            string.IsNullOrWhiteSpace(alias) ? folder.Path : alias));
    }

    private void OnRemoveConnectorWorkspaceClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: LocalConnectorWorkspaceDraft workspace })
        {
            Connector.RemoveWorkspaceCommand.Execute(workspace);
        }
    }

    private async void OnDisconnectConnectorClicked(object sender, RoutedEventArgs e)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = "断开本机 Connector？",
            Content = "这会清除当前 Windows 设备的远程连接凭据和工作区配对，不会退出 ChatOS 登录。",
            PrimaryButtonText = "断开",
            CloseButtonText = "取消",
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await Connector.DisconnectCommand.ExecuteAsync(null);
        }
    }

    private async void OnRefreshPluginsClicked(object sender, RoutedEventArgs e) =>
        await PluginSettings.RefreshAsync();

    private async void OnPluginPrimaryActionClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: PluginSettingsItem plugin })
        {
            await PluginSettings.InstallOrUpdateAsync(plugin);
        }
    }

    private async void OnPluginUninstallClicked(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: PluginSettingsItem plugin }) return;
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = Localization.Text("卸载插件？", "Uninstall plugin?"),
            Content = Localization.Text(
                $"将卸载 {plugin.DisplayName}，并清除它在本机保存的 Secret、OAuth 授权和运行会话。",
                $"This will uninstall {plugin.DisplayName} and clear its local secrets, OAuth authorization, and runtime sessions."),
            PrimaryButtonText = Localization.Text("卸载", "Uninstall"),
            CloseButtonText = Localization.Text("取消", "Cancel"),
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await PluginSettings.UninstallAsync(plugin);
        }
    }

    private async void OnPluginEnabledToggled(object sender, RoutedEventArgs e)
    {
        if (sender is not ToggleSwitch { DataContext: PluginSettingsItem plugin } toggle ||
            toggle.IsOn == plugin.Enabled || plugin.IsBusy)
        {
            return;
        }

        await PluginSettings.SetEnabledAsync(plugin, toggle.IsOn);
        if (toggle.IsOn != plugin.Enabled)
        {
            toggle.IsOn = plugin.Enabled;
        }
    }

    private void OnPluginCredentialPasswordChanged(object sender, RoutedEventArgs e)
    {
        if (sender is PasswordBox { DataContext: PluginCredentialSettingsItem credential } password)
        {
            credential.DraftSecret = password.Password;
        }
    }

    private async void OnSavePluginCredentialClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: PluginCredentialSettingsItem credential })
        {
            await PluginSettings.SaveCredentialAsync(credential);
        }
    }

    private async void OnDeletePluginCredentialClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: PluginCredentialSettingsItem credential })
        {
            await PluginSettings.DeleteCredentialAsync(credential);
        }
    }

    private async void OnPluginOAuthActionClicked(object sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: PluginOAuthSettingsItem app }) return;
        if (app.Connected)
        {
            await PluginSettings.DisconnectOAuthAsync(app);
        }
        else
        {
            await PluginSettings.BeginOAuthAsync(app);
        }
    }
}
