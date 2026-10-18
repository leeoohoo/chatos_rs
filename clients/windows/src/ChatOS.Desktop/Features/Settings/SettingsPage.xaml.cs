using ChatOS.Core.Domain;
using ChatOS.Connector.Approval;
using ChatOS.Connector.Sandbox;
using ChatOS.Desktop.Features.Pet;
using ChatOS.Desktop.Features.QuickSearch;
using Microsoft.UI.Xaml.Media.Imaging;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Windows.Storage.Pickers;
using Windows.Storage.Streams;

namespace ChatOS.Desktop.Features.Settings;

public sealed partial class SettingsPage : UserControl
{
    private bool _controlsInitialized;
    private bool _syncing;
    private bool _syncingSettingsNavigation;
    private readonly PetWindowController _petWindowController;
    private readonly WindowsGlobalHotKeyService _globalHotKeys;

    public SettingsPage(
        AppSettingsViewModel viewModel,
        ConnectorSettingsViewModel connector,
        PluginSettingsViewModel pluginSettings,
        CustomPluginInstallerViewModel customPluginInstaller,
        ApprovalSettingsViewModel approvalSettings,
        ModelSettingsViewModel modelSettings,
        SandboxSettingsViewModel sandboxSettings,
        WeChatCompanionSettingsViewModel weChatSettings,
        LocalizationViewModel localization,
        PetWindowController petWindowController,
        WindowsGlobalHotKeyService globalHotKeys)
    {
        ViewModel = viewModel;
        Connector = connector;
        PluginSettings = pluginSettings;
        CustomPluginInstaller = customPluginInstaller;
        ApprovalSettings = approvalSettings;
        ModelSettings = modelSettings;
        SandboxSettings = sandboxSettings;
        WeChatSettings = weChatSettings;
        Localization = localization;
        _petWindowController = petWindowController;
        _globalHotKeys = globalHotKeys;
        InitializeComponent();
        _controlsInitialized = true;
        ShowSettingsSection("general");
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
        ViewModel.PropertyChanged += (_, args) =>
        {
            if (args.PropertyName is not (nameof(AppSettingsViewModel.IsSaving) or
                nameof(AppSettingsViewModel.ErrorMessage) or nameof(AppSettingsViewModel.HasError)))
                SyncControls();
        };
        ApprovalSettings.PropertyChanged += (_, _) => SyncControls();
        SandboxSettings.PropertyChanged += (_, _) => SyncControls();
        WeChatSettings.PropertyChanged += OnWeChatSettingsPropertyChanged;
        _globalHotKeys.ConfigurationChanged += (_, _) => SyncGlobalUtilityStatus();
    }

    public AppSettingsViewModel ViewModel { get; }

    public LocalizationViewModel Localization { get; }

    public ConnectorSettingsViewModel Connector { get; }

    public PluginSettingsViewModel PluginSettings { get; }

    public CustomPluginInstallerViewModel CustomPluginInstaller { get; }

    public ApprovalSettingsViewModel ApprovalSettings { get; }

    public ModelSettingsViewModel ModelSettings { get; }

    public SandboxSettingsViewModel SandboxSettings { get; }

    public WeChatCompanionSettingsViewModel WeChatSettings { get; }

    public event EventHandler? CloseRequested;

    public void OpenSection(string section)
    {
        _syncingSettingsNavigation = true;
        try
        {
            SettingsNavigation.SelectedItem = SettingsNavigation.Items
                .OfType<ListViewItem>()
                .FirstOrDefault(item => string.Equals(item.Tag as string, section, StringComparison.Ordinal));
            LocalSettingsNavigation.SelectedItem = LocalSettingsNavigation.Items
                .OfType<ListViewItem>()
                .FirstOrDefault(item => string.Equals(item.Tag as string, section, StringComparison.Ordinal));
        }
        finally
        {
            _syncingSettingsNavigation = false;
        }

        ShowSettingsSection(section);
    }

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        try
        {
            SyncControls();
            await Task.WhenAll(
                Connector.OpenAsync(),
                PluginSettings.OpenAsync(),
                ApprovalSettings.OpenAsync(),
                ModelSettings.OpenAsync(),
                SandboxSettings.OpenAsync(),
                WeChatSettings.OpenAsync());
        }
        catch (Exception exception)
        {
            StartupDiagnostics.RecordUnhandled("settings loading", exception);
            Connector.ErrorMessage = exception.Message;
        }
    }

    private void OnUnloaded(object sender, RoutedEventArgs e)
    {
        Connector.Close();
        CustomPluginInstaller.CancelPending();
        WeChatSettings.StopPolling();
    }

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
            PetSizeSlider.Value = ViewModel.PetSize;
            PetSizeValueLabel.Text = $"{ViewModel.PetSize:F0} px";
            PetShowProcessSwitch.IsOn = ViewModel.PetShowProcess;
            PetShowCompletionsSwitch.IsOn = ViewModel.PetShowCompletions;
            GlobalUtilitiesSwitch.IsOn = ViewModel.GlobalUtilitiesEnabled;
            GlobalScreenshotSwitch.IsOn = ViewModel.GlobalScreenshotEnabled;
            GlobalRecordingSwitch.IsOn = ViewModel.GlobalRecordingEnabled;
            GlobalClipboardSwitch.IsOn = ViewModel.GlobalClipboardEnabled;
            GlobalQuickSearchSwitch.IsOn = ViewModel.GlobalQuickSearchEnabled;
            GlobalScreenshotSwitch.IsEnabled = ViewModel.GlobalUtilitiesEnabled;
            GlobalRecordingSwitch.IsEnabled = ViewModel.GlobalUtilitiesEnabled;
            GlobalClipboardSwitch.IsEnabled = ViewModel.GlobalUtilitiesEnabled;
            GlobalQuickSearchSwitch.IsEnabled = ViewModel.GlobalUtilitiesEnabled;
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
        SyncGlobalUtilityStatus();
        SyncRuntimePermissionStatus();
    }

    private void OnBackClicked(object sender, RoutedEventArgs e) =>
        CloseRequested?.Invoke(this, EventArgs.Empty);

    private void OnSettingsNavigationChanged(object sender, SelectionChangedEventArgs e)
    {
        if (!_controlsInitialized || _syncingSettingsNavigation ||
            sender is not ListView list || list.SelectedItem is not ListViewItem item ||
            item.Tag is not string section)
        {
            return;
        }

        _syncingSettingsNavigation = true;
        try
        {
            if (ReferenceEquals(list, SettingsNavigation))
            {
                LocalSettingsNavigation.SelectedItem = null;
            }
            else
            {
                SettingsNavigation.SelectedItem = null;
            }
        }
        finally
        {
            _syncingSettingsNavigation = false;
        }

        ShowSettingsSection(section);
    }

    private void ShowSettingsSection(string section)
    {
        var panels = new FrameworkElement[]
        {
            GeneralSettingsSection,
            PetSettingsSection,
            WeChatSettingsSection,
            UtilitiesSettingsSection,
            RuntimeSettingsSection,
            ConnectorSectionHeading,
            ConnectorSectionCard,
            ConnectorBoundaryHeading,
            ConnectorBoundaryCard,
            SandboxSectionHeading,
            SandboxSectionCard,
            SandboxBackendHeading,
            SandboxBackendCard,
            ModelsSectionHeading,
            ModelsSectionCard,
            ApprovalSectionHeading,
            ApprovalSectionCard,
            PendingApprovalsSection,
            ApprovalHistorySection,
            PluginsSectionHeading,
            CustomPluginInstallerCard,
            PluginActionMessage,
            PluginErrorMessage,
            PluginEmptyMessage,
            PluginsSectionList,
        };
        foreach (var panel in panels) panel.Visibility = Visibility.Collapsed;

        switch (section)
        {
            case "general":
                SettingsSectionTitle.Text = "常规与账号";
                SettingsSectionDescription.Text = "语言、外观、字体大小、运行行为与当前账号。";
                GeneralSettingsSection.Visibility = Visibility.Visible;
                break;
            case "wechat":
                SettingsSectionTitle.Text = "微信小程序";
                SettingsSectionDescription.Text = "绑定微信账号，并查看已登录的小程序设备。";
                WeChatSettingsSection.Visibility = Visibility.Visible;
                break;
            case "pet":
                SettingsSectionTitle.Text = "宠物";
                SettingsSectionDescription.Text = "控制全局宠物的显示、任务提醒、大小与位置。";
                PetSettingsSection.Visibility = Visibility.Visible;
                break;
            case "utilities":
                SettingsSectionTitle.Text = "全局工具";
                SettingsSectionDescription.Text = "查看 Windows 上可用的截图、录屏、剪贴板与快速搜索能力。";
                UtilitiesSettingsSection.Visibility = Visibility.Visible;
                break;
            case "models":
                SettingsSectionTitle.Text = "AI 模型配置";
                SettingsSectionDescription.Text = "同步可用模型，并设置本机审批模型和请求重试。";
                ModelsSectionHeading.Visibility = Visibility.Visible;
                ModelsSectionCard.Visibility = Visibility.Visible;
                break;
            case "connector":
                SettingsSectionTitle.Text = "设备与网关";
                SettingsSectionDescription.Text = "查看本机 Connector 的连接状态、身份与工作区边界。";
                ConnectorSectionHeading.Visibility = Visibility.Visible;
                ConnectorSectionCard.Visibility = Visibility.Visible;
                ConnectorBoundaryHeading.Visibility = Connector.IsPaired
                    ? Visibility.Visible : Visibility.Collapsed;
                ConnectorBoundaryCard.Visibility = Connector.IsPaired
                    ? Visibility.Visible : Visibility.Collapsed;
                break;
            case "plugins":
                SettingsSectionTitle.Text = "插件与 Skills";
                SettingsSectionDescription.Text = "管理插件、能力权限、凭据和账号授权。";
                PluginsSectionHeading.Visibility = Visibility.Visible;
                CustomPluginInstallerCard.Visibility = Visibility.Visible;
                PluginActionMessage.Visibility = string.IsNullOrWhiteSpace(PluginSettings.ActionMessage)
                    ? Visibility.Collapsed : Visibility.Visible;
                PluginErrorMessage.Visibility = string.IsNullOrWhiteSpace(PluginSettings.ErrorMessage)
                    ? Visibility.Collapsed : Visibility.Visible;
                PluginEmptyMessage.Visibility = PluginSettings.HasFilteredPlugins
                    ? Visibility.Collapsed : Visibility.Visible;
                PluginsSectionList.Visibility = Visibility.Visible;
                break;
            case "approvals":
                SettingsSectionTitle.Text = "命令审批";
                SettingsSectionDescription.Text = "设置默认审批级别，并处理等待确认的命令与审计记录。";
                ApprovalSectionHeading.Visibility = Visibility.Visible;
                ApprovalSectionCard.Visibility = Visibility.Visible;
                PendingApprovalsSection.Visibility = Visibility.Visible;
                ApprovalHistorySection.Visibility = Visibility.Visible;
                break;
            case "runtime":
                SettingsSectionTitle.Text = "运行与系统权限";
                SettingsSectionDescription.Text = "查看 Windows 本机能力和受控执行边界。";
                RuntimeSettingsSection.Visibility = Visibility.Visible;
                break;
            case "access":
                SettingsSectionTitle.Text = "权限控制";
                SettingsSectionDescription.Text = "设置命令沙箱、文件范围和网络访问策略。";
                SandboxSectionHeading.Visibility = Visibility.Visible;
                SandboxSectionCard.Visibility = Visibility.Visible;
                SandboxBackendHeading.Visibility = Visibility.Visible;
                SandboxBackendCard.Visibility = Visibility.Visible;
                break;
        }

        SettingsContentScroller?.ChangeView(null, 0, null, true);
    }

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
        // The Slider raises ValueChanged while InitializeComponent is still creating
        // later siblings, before FontScaleLabel has been assigned.
        if (!_controlsInitialized || FontScaleLabel is null) return;

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

    private async void OnResetFontScaleClicked(object sender, RoutedEventArgs e) =>
        await ViewModel.ResetFontScaleAsync();

    private async void OnPetSizeChanged(object sender, RangeBaseValueChangedEventArgs e)
    {
        if (!_controlsInitialized || PetSizeValueLabel is null) return;

        PetSizeValueLabel.Text = $"{e.NewValue:F0} px";
        if (!_syncing)
        {
            await ViewModel.SetPetSizeAsync(e.NewValue);
        }
    }

    private async void OnPetResetPositionClicked(object sender, RoutedEventArgs e)
    {
        try
        {
            await _petWindowController.ResetPositionAsync();
        }
        catch (Exception exception)
        {
            StartupDiagnostics.RecordUnhandled("pet position reset", exception);
        }
    }

    private async void OnWeChatSettingsPropertyChanged(
        object? sender,
        System.ComponentModel.PropertyChangedEventArgs e)
    {
        if (e.PropertyName is not (nameof(WeChatCompanionSettingsViewModel.TicketImageData) or
            nameof(WeChatCompanionSettingsViewModel.Ticket) or ""))
        {
            return;
        }

        try
        {
            var data = WeChatSettings.TicketImageData;
            if (data is null || data.Length == 0)
            {
                WeChatQrImage.Source = null;
                return;
            }

            using var stream = new InMemoryRandomAccessStream();
            using (var writer = new DataWriter(stream))
            {
                writer.WriteBytes(data);
                await writer.StoreAsync();
                await writer.FlushAsync();
                writer.DetachStream();
            }
            stream.Seek(0);
            var source = new BitmapImage();
            await source.SetSourceAsync(stream);
            if (ReferenceEquals(data, WeChatSettings.TicketImageData))
            {
                WeChatQrImage.Source = source;
            }
        }
        catch (Exception exception)
        {
            StartupDiagnostics.RecordUnhandled("WeChat Mini Program code rendering", exception);
        }
    }

    private async void OnGenerateWeChatTicketClicked(object sender, RoutedEventArgs e) =>
        await WeChatSettings.IssueTicketAsync();

    private async void OnConfirmWeChatTicketClicked(object sender, RoutedEventArgs e) =>
        await WeChatSettings.ConfirmTicketAsync();

    private async void OnRevokeWeChatSessionClicked(object sender, RoutedEventArgs e)
    {
        if (sender is Button { DataContext: WeChatSessionItemViewModel session })
        {
            await WeChatSettings.RevokeAsync(session);
        }
    }

    private async void OnUnbindWeChatClicked(object sender, RoutedEventArgs e)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = Localization.Text("解除微信绑定？", "Unbind WeChat?"),
            Content = Localization.Text(
                "所有微信小程序登录会立即失效。桌面设备、项目和会话不会被删除。",
                "All Mini Program sessions will be revoked. Desktop devices, projects, and conversations will not be deleted."),
            PrimaryButtonText = Localization.Text("解除绑定", "Unbind"),
            CloseButtonText = Localization.Text("取消", "Cancel"),
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            await WeChatSettings.UnbindAsync();
        }
    }

    private async void OnPetShowProcessToggled(object sender, RoutedEventArgs e)
    {
        if (!_syncing)
        {
            await ViewModel.SetPetShowProcessAsync(PetShowProcessSwitch.IsOn);
        }
    }

    private async void OnPetShowCompletionsToggled(object sender, RoutedEventArgs e)
    {
        if (!_syncing)
        {
            await ViewModel.SetPetShowCompletionsAsync(PetShowCompletionsSwitch.IsOn);
        }
    }

    private async void OnGlobalUtilitiesToggled(object sender, RoutedEventArgs e)
    {
        if (!_syncing)
            await ViewModel.SetGlobalUtilitiesEnabledAsync(GlobalUtilitiesSwitch.IsOn);
    }

    private async void OnGlobalScreenshotToggled(object sender, RoutedEventArgs e)
    {
        if (!_syncing)
            await ViewModel.SetGlobalScreenshotEnabledAsync(GlobalScreenshotSwitch.IsOn);
    }

    private async void OnGlobalRecordingToggled(object sender, RoutedEventArgs e)
    {
        if (!_syncing)
            await ViewModel.SetGlobalRecordingEnabledAsync(GlobalRecordingSwitch.IsOn);
    }

    private async void OnGlobalClipboardToggled(object sender, RoutedEventArgs e)
    {
        if (!_syncing)
            await ViewModel.SetGlobalClipboardEnabledAsync(GlobalClipboardSwitch.IsOn);
    }

    private async void OnGlobalQuickSearchToggled(object sender, RoutedEventArgs e)
    {
        if (!_syncing)
            await ViewModel.SetGlobalQuickSearchEnabledAsync(GlobalQuickSearchSwitch.IsOn);
    }

    private async void OnRestoreGlobalUtilityDefaultsClicked(object sender, RoutedEventArgs e)
    {
        await ViewModel.ResetGlobalUtilityDefaultsAsync();
    }

    private void SyncGlobalUtilityStatus()
    {
        if (!_controlsInitialized || GlobalScreenshotStatus is null) return;
        SetGlobalUtilityStatus(
            GlobalScreenshotStatus,
            GlobalScreenshotShortcut,
            WindowsGlobalUtilityAction.Screenshot);
        SetGlobalUtilityStatus(
            GlobalRecordingStatus,
            GlobalRecordingShortcut,
            WindowsGlobalUtilityAction.ScreenRecording);
        SetGlobalUtilityStatus(
            GlobalClipboardStatus,
            GlobalClipboardShortcut,
            WindowsGlobalUtilityAction.ClipboardHistory);
        SetGlobalUtilityStatus(
            GlobalQuickSearchStatus,
            GlobalQuickSearchShortcut,
            WindowsGlobalUtilityAction.QuickSearch);
    }

    private void SetGlobalUtilityStatus(
        TextBlock statusLabel,
        TextBlock shortcutLabel,
        WindowsGlobalUtilityAction action)
    {
        var state = _globalHotKeys.GetState(action);
        statusLabel.Text = state switch
        {
            { Enabled: false } => "未注册",
            { IsRegistered: true, UsesFallback: true } => "已注册备用键",
            { IsRegistered: true } => "已注册",
            _ => "快捷键冲突",
        };
        statusLabel.Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources[
            state.IsRegistered ? "ChatOSSuccessBrush" :
            state.Enabled ? "ChatOSWarningBrush" : "ChatOSSecondaryTextBrush"];
        shortcutLabel.Text = state.ActiveShortcut ?? state.RequestedShortcut;
    }

    private void SyncRuntimePermissionStatus()
    {
        if (!_controlsInitialized || RuntimeTerminalStatus is null) return;

        var commandProcessor = Environment.GetEnvironmentVariable("ComSpec");
        var terminalReady = OperatingSystem.IsWindows() &&
            !string.IsNullOrWhiteSpace(commandProcessor) &&
            File.Exists(commandProcessor);
        SetRuntimeStatus(
            RuntimeTerminalStatus,
            terminalReady ? "已就绪" : "需要检查",
            terminalReady);

        var captureReady = false;
        try
        {
            captureReady = Windows.Graphics.Capture.GraphicsCaptureSession.IsSupported();
        }
        catch
        {
            // Older Windows builds and non-interactive sessions can reject this capability check.
        }
        SetRuntimeStatus(
            RuntimeCaptureStatus,
            captureReady ? "按需授权" : "当前不可用",
            captureReady);

        var restricted = SandboxSettings.IsEnabled &&
            SandboxSettings.PermissionProfile is not ConnectorSandboxPermissionProfile.FullAccess;
        var controlledNetworkReady = SandboxSettings.NetworkAccess is not ConnectorSandboxNetworkAccess.Controlled ||
            SandboxSettings.IsControlledNetworkAvailable;
        SetRuntimeStatus(
            RuntimeBoundaryStatus,
            restricted && controlledNetworkReady ? "已就绪" : restricted ? "网络组件待就绪" : "未启用",
            restricted && controlledNetworkReady);
    }

    private static void SetRuntimeStatus(TextBlock label, string text, bool ready)
    {
        label.Text = text;
        label.Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources[
            ready ? "ChatOSSuccessBrush" : "ChatOSWarningBrush"];
    }

    private async void OnRefreshRuntimePermissionsClicked(object sender, RoutedEventArgs e)
    {
        await SandboxSettings.RefreshReadinessAsync();
        SyncRuntimePermissionStatus();
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

    private async void OnRefreshApprovalHistoryClicked(object sender, RoutedEventArgs e)
    {
        if (ApprovalSettings.CanRefreshHistory) await ApprovalSettings.OpenAsync();
    }

    private async void OnRetryApprovalReviewerReadinessClicked(object sender, RoutedEventArgs e)
    {
        if (ApprovalSettings.CanRetryReviewerReadiness) await ApprovalSettings.RefreshReviewerReadinessAsync();
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

    private async void OnChooseCustomPluginFolderClicked(object sender, RoutedEventArgs e)
    {
        var picker = new FolderPicker { SuggestedStartLocation = PickerLocationId.Downloads };
        picker.FileTypeFilter.Add("*");
        var window = (Application.Current as App)?.MainWindow;
        if (window is null) return;
        WinRT.Interop.InitializeWithWindow.Initialize(
            picker,
            WinRT.Interop.WindowNative.GetWindowHandle(window));
        var folder = await picker.PickSingleFolderAsync();
        if (folder is not null && !string.IsNullOrWhiteSpace(folder.Path))
        {
            CustomPluginInstaller.SourcePath = folder.Path;
        }
    }

    private async void OnInspectCustomPluginClicked(object sender, RoutedEventArgs e) =>
        await CustomPluginInstaller.InspectAsync();

    private async void OnInstallCustomPluginClicked(object sender, RoutedEventArgs e)
    {
        var inspection = CustomPluginInstaller.Inspection;
        if (inspection is null) return;
        var dialog = new ContentDialog
        {
            XamlRoot = XamlRoot,
            Title = Localization.Text(
                "安装导入的 Plugin / Skill / MCP？",
                "Install imported Plugin / Skill / MCP?"),
            Content = Localization.Text(
                $"将重新读取 {inspection.DisplayName} 的来源并核对 SHA-256 指纹。只有来源和实际工具清单与预检完全一致时才会写入本机安装记录。",
                $"ChatOS will read the source for {inspection.DisplayName} again and verify its SHA-256 fingerprint. The local installation record is written only when the source and actual tool list exactly match preflight."),
            PrimaryButtonText = Localization.Text("安装", "Install"),
            CloseButtonText = Localization.Text("取消", "Cancel"),
            DefaultButton = ContentDialogButton.Close,
        };
        if (await dialog.ShowAsync() != ContentDialogResult.Primary) return;
        if (await CustomPluginInstaller.InstallAsync())
        {
            await PluginSettings.RefreshAsync();
        }
    }

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
