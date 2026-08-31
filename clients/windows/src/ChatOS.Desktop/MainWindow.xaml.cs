using System.ComponentModel;
using ChatOS.Desktop.AppShell;
using ChatOS.Desktop.Features.Chat;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Desktop.Features.Notepad;
using ChatOS.Desktop.Features.Remote;
using ChatOS.Desktop.Features.Pet;
using ChatOS.Desktop.Features.Plugins;
using ChatOS.Desktop.Features.Terminal;
using ChatOS.Connector.Approval;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;

namespace ChatOS.Desktop;

public sealed partial class MainWindow : Window
{
    private ShellResourceViewModel? _lastWorkspaceResource;

    public MainWindow(
        MainWindowViewModel viewModel,
        WorkspaceHostPage workspaceHostPage,
        SettingsPage settingsPage,
        NotepadPage notepadPage,
        RemoteConnectionsPage remoteConnectionsPage,
        RemoteSftpPage remoteSftpPage,
        RemoteTerminalPage remoteTerminalPage,
        LocalTerminalPage localTerminalPage,
        CommandApprovalCoordinator approvals,
        AppPreferencesManager preferences,
        PetWindowController petWindowController,
        PluginVisualSessionController visualSessionController,
        PluginArtifactsWindow artifactsWindow)
    {
        ViewModel = viewModel;
        WorkspaceHost = workspaceHostPage;
        SettingsPage = settingsPage;
        NotepadPage = notepadPage;
        RemoteConnectionsPage = remoteConnectionsPage;
        RemoteSftpPage = remoteSftpPage;
        RemoteTerminalPage = remoteTerminalPage;
        LocalTerminalPage = localTerminalPage;
        Approvals = approvals;
        Preferences = preferences;
        PetWindowController = petWindowController;
        VisualSessionController = visualSessionController;
        ArtifactsWindow = artifactsWindow;
        InitializeComponent();

        ExtendsContentIntoTitleBar = true;
        SetTitleBar(AppTitleBar);
        AppWindow.Resize(new SizeInt32(1440, 900));
        AppWindow.Title = "ChatOS";
        WorkspaceContent.Content = WorkspaceHost;
        SettingsPage.CloseRequested += OnSettingsCloseRequested;
        SettingsPage.Connector.PropertyChanged += OnConnectorSettingsPropertyChanged;
        RemoteConnectionsPage.OpenSftpRequested += OnOpenSftpRequested;
        RemoteConnectionsPage.OpenTerminalRequested += OnOpenTerminalRequested;
        RemoteSftpPage.CloseRequested += OnRemoteSftpCloseRequested;
        RemoteTerminalPage.CloseRequested += OnRemoteTerminalCloseRequested;
        LocalTerminalPage.CloseRequested += OnLocalTerminalCloseRequested;
        NotepadPage.CloseRequested += OnNotepadCloseRequested;
        Preferences.Changed += OnPreferencesChanged;
        ViewModel.PropertyChanged += OnViewModelPropertyChanged;
        Approvals.PendingChanged += OnPendingApprovalsChanged;
        Activated += OnActivated;
    }

    public MainWindowViewModel ViewModel { get; }

    public WorkspaceHostPage WorkspaceHost { get; }

    public SettingsPage SettingsPage { get; }

    public NotepadPage NotepadPage { get; }

    public RemoteConnectionsPage RemoteConnectionsPage { get; }

    public RemoteSftpPage RemoteSftpPage { get; }

    public RemoteTerminalPage RemoteTerminalPage { get; }

    public LocalTerminalPage LocalTerminalPage { get; }

    public CommandApprovalCoordinator Approvals { get; }

    public AppPreferencesManager Preferences { get; }

    public PetWindowController PetWindowController { get; }

    public PluginVisualSessionController VisualSessionController { get; }

    public PluginArtifactsWindow ArtifactsWindow { get; }

    private ConnectorPendingApproval? ActiveApproval { get; set; }

    private async void OnActivated(object sender, WindowActivatedEventArgs args)
    {
        Activated -= OnActivated;
        ApplyAppearance(Preferences.Current);
        UpdateVisualState();
        await ViewModel.InitializeAsync();
        await PetWindowController.SetAuthenticatedAsync(ViewModel.IsAuthenticated);
        await VisualSessionController.SetAuthenticatedAsync(ViewModel.IsAuthenticated);
        try
        {
            await Approvals.InitializeAsync();
        }
        catch (Exception exception)
        {
            ViewModel.ErrorMessage = ViewModel.Localization.Text(
                $"加载本机审批状态失败：{exception.Message}",
                $"Unable to load local approval state: {exception.Message}");
        }
        UpdateVisualState();
    }

    private void OnPasswordChanged(object sender, RoutedEventArgs e)
    {
        ViewModel.Password = ((PasswordBox)sender).Password;
    }

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName is nameof(MainWindowViewModel.IsAuthenticated) or
            nameof(MainWindowViewModel.IsBusy) or
            nameof(MainWindowViewModel.ErrorMessage) or
            nameof(MainWindowViewModel.Password) or
            nameof(MainWindowViewModel.SelectedResource))
        {
            if (e.PropertyName == nameof(MainWindowViewModel.SelectedResource))
            {
                if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.LocalConnector)
                {
                    ShowSettings();
                }
                else if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.RemoteConnection)
                {
                    WorkspaceContent.Content = RemoteConnectionsPage;
                }
                else if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.LocalTerminal)
                {
                    _ = OpenLocalTerminalAsync(ViewModel.SelectedResource);
                }
                else
                {
                    _lastWorkspaceResource = ViewModel.SelectedResource;
                    ShowWorkspace();
                }
            }
            if (e.PropertyName == nameof(MainWindowViewModel.IsAuthenticated))
            {
                _ = PetWindowController.SetAuthenticatedAsync(ViewModel.IsAuthenticated);
                _ = VisualSessionController.SetAuthenticatedAsync(ViewModel.IsAuthenticated);
                if (!ViewModel.IsAuthenticated)
                {
                    _ = LocalTerminalPage.CloseSessionAsync();
                }
            }
            UpdateVisualState();
        }
    }

    private void OnSettingsClicked(object sender, RoutedEventArgs e)
    {
        ShowSettings();
    }

    private async void OnNotepadClicked(object sender, RoutedEventArgs e)
    {
        WorkspaceContent.Content = NotepadPage;
        await NotepadPage.ViewModel.OpenAsync();
    }

    private async void OnArtifactsClicked(object sender, RoutedEventArgs e) =>
        await ArtifactsWindow.ShowAsync();

    private void OnNotepadCloseRequested(object? sender, EventArgs e)
    {
        _ = NotepadPage.ViewModel.CloseAsync();
        ShowWorkspace();
    }

    private async void OnSettingsCloseRequested(object? sender, EventArgs e)
    {
        await ViewModel.RefreshLocalConnectorAsync();
        if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.LocalConnector)
        {
            ViewModel.SelectedResource = _lastWorkspaceResource;
        }
        ShowWorkspace();
    }

    private void ShowSettings() => WorkspaceContent.Content = SettingsPage;

    private async void OnOpenSftpRequested(object? sender, RemoteConnection connection)
    {
        WorkspaceContent.Content = RemoteSftpPage;
        await RemoteSftpPage.OpenAsync(connection);
    }

    private void OnRemoteSftpCloseRequested(object? sender, EventArgs e) =>
        WorkspaceContent.Content = RemoteConnectionsPage;

    private void OnOpenTerminalRequested(object? sender, RemoteConnection connection)
    {
        WorkspaceContent.Content = RemoteTerminalPage;
        RemoteTerminalPage.Open(connection);
    }

    private void OnRemoteTerminalCloseRequested(object? sender, EventArgs e) =>
        WorkspaceContent.Content = RemoteConnectionsPage;

    private async Task OpenLocalTerminalAsync(ShellResourceViewModel resource)
    {
        try
        {
            WorkspaceContent.Content = LocalTerminalPage;
            await LocalTerminalPage.OpenAsync(resource);
        }
        catch (Exception exception)
        {
            ViewModel.ErrorMessage = exception.Message;
            ShowWorkspace();
        }
    }

    private void OnLocalTerminalCloseRequested(object? sender, EventArgs e)
    {
        ViewModel.SelectedResource = _lastWorkspaceResource;
        ShowWorkspace();
    }

    private async void OnCreateLocalTerminalClicked(object sender, RoutedEventArgs e)
    {
        await ViewModel.RefreshLocalConnectorAsync();
        var workspaces = ViewModel.LocalConnectorStatus?.Workspaces ?? [];
        if (workspaces.Count == 0)
        {
            ViewModel.ErrorMessage = ViewModel.Localization.Text(
                "请先在设置中配对本机 Connector 并添加工作区。",
                "Pair the local Connector and add a workspace in Settings first.");
            return;
        }

        LocalConnectorWorkspaceStatus? selected = workspaces[0];
        if (workspaces.Count > 1)
        {
            var picker = new ComboBox
            {
                ItemsSource = workspaces,
                DisplayMemberPath = nameof(LocalConnectorWorkspaceStatus.Alias),
                SelectedIndex = 0,
                MinWidth = 360,
            };
            var dialog = new ContentDialog
            {
                XamlRoot = RootGrid.XamlRoot,
                Title = ViewModel.Localization.Text("选择终端工作区", "Select terminal workspace"),
                Content = picker,
                PrimaryButtonText = ViewModel.Localization.Text("创建终端", "Create terminal"),
                CloseButtonText = ViewModel.Localization.Text("取消", "Cancel"),
                DefaultButton = ContentDialogButton.Primary,
            };
            if (await dialog.ShowAsync() != ContentDialogResult.Primary)
            {
                return;
            }
            selected = picker.SelectedItem as LocalConnectorWorkspaceStatus;
        }

        if (selected is not null)
        {
            ViewModel.CreateLocalTerminalResource(selected);
        }
    }

    private void OnConnectorSettingsPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(ConnectorSettingsViewModel.Status) && SettingsPage.Connector.Status is { } status)
        {
            ViewModel.ApplyLocalConnectorStatus(status);
        }
    }

    private void ShowWorkspace()
    {
        WorkspaceContent.Content = WorkspaceHost;
        WorkspaceHost.Configure(ViewModel.SelectedResource);
    }

    private void OnPreferencesChanged(object? sender, AppPreferences preferences)
    {
        _ = DispatcherQueue.TryEnqueue(() => ApplyAppearance(preferences));
    }

    private void ApplyAppearance(AppPreferences preferences)
    {
        RootGrid.RequestedTheme = preferences.Theme switch
        {
            InterfaceTheme.Light => ElementTheme.Light,
            InterfaceTheme.Dark => ElementTheme.Dark,
            _ => ElementTheme.Default,
        };
        Application.Current.Resources["ChatOSFontSizeCaption"] = 11d * preferences.FontScale;
        Application.Current.Resources["ChatOSFontSizeBody"] = 13d * preferences.FontScale;
        Application.Current.Resources["ChatOSFontSizeHeadline"] = 14d * preferences.FontScale;
        Application.Current.Resources["ChatOSFontSizePageTitle"] = 24d * preferences.FontScale;
    }

    private void UpdateVisualState()
    {
        LoginRoot.Visibility = ViewModel.IsAuthenticated ? Visibility.Collapsed : Visibility.Visible;
        ShellRoot.Visibility = ViewModel.IsAuthenticated ? Visibility.Visible : Visibility.Collapsed;
        SetTitleBar(ViewModel.IsAuthenticated ? AppTitleBar : LoginTitleBar);
        LoginButton.IsEnabled = !ViewModel.IsBusy;
        LoginErrorText.Visibility = string.IsNullOrWhiteSpace(ViewModel.ErrorMessage)
            ? Visibility.Collapsed
            : Visibility.Visible;
        if (ViewModel.Password.Length == 0 && LoginPasswordBox.Password.Length != 0)
        {
            LoginPasswordBox.Password = string.Empty;
        }

        WorkspaceHost.Configure(ViewModel.SelectedResource);
        RefreshApprovalOverlay();
    }

    private void OnPendingApprovalsChanged(object? sender, EventArgs e)
    {
        _ = DispatcherQueue.TryEnqueue(RefreshApprovalOverlay);
    }

    private void RefreshApprovalOverlay()
    {
        var pending = Approvals.Snapshot();
        ActiveApproval = pending.FirstOrDefault();
        ApprovalOverlay.Visibility = ActiveApproval is null
            ? Visibility.Collapsed
            : Visibility.Visible;
        if (ActiveApproval is not { } approval)
        {
            return;
        }

        ApprovalSourceText.Text = $"{approval.Source} · {approval.CreatedAt.ToLocalTime():HH:mm:ss}";
        ApprovalCommandText.Text = approval.Command;
        ApprovalWorkingDirectoryText.Text = approval.WorkingDirectory;
        ApprovalReasonText.Text = approval.Reason ?? approval.Risk.Reason ?? ViewModel.Localization.DefaultApprovalReason;
        ApprovalQueueText.Text = pending.Count > 1
            ? ViewModel.Localization.QueuedApprovals(pending.Count - 1)
            : ViewModel.Localization.ApprovalContinuesTask;
        ApprovalRiskText.Text = approval.Risk.Level switch
        {
            ConnectorApprovalRiskLevel.High => ViewModel.Localization.HighRisk,
            ConnectorApprovalRiskLevel.Medium => ViewModel.Localization.MediumRisk,
            _ => ViewModel.Localization.LowRisk,
        };
        ApprovalRiskText.Foreground = approval.Risk.Level switch
        {
            ConnectorApprovalRiskLevel.High => (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["ChatOSFailureBrush"],
            ConnectorApprovalRiskLevel.Medium => (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["ChatOSWarningBrush"],
            _ => (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["ChatOSSuccessBrush"],
        };
    }

    private async void OnDeclineApproval(object sender, RoutedEventArgs e) =>
        await ResolveActiveApprovalAsync(ConnectorApprovalAction.Decline);

    private async void OnAcceptApproval(object sender, RoutedEventArgs e) =>
        await ResolveActiveApprovalAsync(ConnectorApprovalAction.Accept);

    private async void OnAcceptSessionApproval(object sender, RoutedEventArgs e) =>
        await ResolveActiveApprovalAsync(ConnectorApprovalAction.AcceptForSession);

    private async Task ResolveActiveApprovalAsync(ConnectorApprovalAction action)
    {
        if (ActiveApproval is not { } approval)
        {
            return;
        }

        SetApprovalButtonsEnabled(false);
        ApprovalProgressRing.IsActive = true;
        try
        {
            await Approvals.ResolveAsync(approval.Id, action);
        }
        catch (Exception exception)
        {
            ViewModel.ErrorMessage = ViewModel.Localization.Text(
                $"处理本机审批失败：{exception.Message}",
                $"Unable to process the local approval: {exception.Message}");
        }
        finally
        {
            ApprovalProgressRing.IsActive = false;
            SetApprovalButtonsEnabled(true);
            RefreshApprovalOverlay();
        }
    }

    private void SetApprovalButtonsEnabled(bool enabled)
    {
        DeclineApprovalButton.IsEnabled = enabled;
        AcceptApprovalButton.IsEnabled = enabled;
        AcceptSessionApprovalButton.IsEnabled = enabled;
    }
}
