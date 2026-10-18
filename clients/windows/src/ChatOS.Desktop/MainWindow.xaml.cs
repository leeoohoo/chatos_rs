using System.ComponentModel;
using ChatOS.Desktop.AppShell;
using ChatOS.Desktop.Features.Chat;
using ChatOS.Desktop.Features.Settings;
using ChatOS.Desktop.Features.Notepad;
using ChatOS.Desktop.Features.Remote;
using ChatOS.Desktop.Features.Pet;
using ChatOS.Desktop.Features.Plugins;
using ChatOS.Desktop.Features.Terminal;
using ChatOS.Desktop.Features.Clipboard;
using ChatOS.Desktop.Features.AgentTeams;
using ChatOS.Desktop.Features.MediaStudio;
using ChatOS.Desktop.Features.Projects;
using ChatOS.Connector.Approval;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Settings;
using Microsoft.UI.Windowing;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;

namespace ChatOS.Desktop;

public sealed partial class MainWindow : Window
{
    private ShellResourceViewModel? _lastWorkspaceResource;
    private bool _postActivationInitializationComplete;
    private bool? _renderedAuthenticationState;
    private bool _sidebarSelectionPending;
    private bool _restoringSidebarSelection;

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
        PluginArtifactsWindow artifactsWindow,
        ClipboardHistoryWindow clipboardHistoryWindow,
        PluginApplicationsPage pluginApplicationsPage,
        ProjectFeatureHubPage projectFeatureHubPage,
        MediaStudioPage mediaStudioPage,
        AgentWorkspacePage agentWorkspacePage,
        IProjectFilesystemService projectFilesystem)
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
        ClipboardHistoryWindow = clipboardHistoryWindow;
        PluginApplicationsPage = pluginApplicationsPage;
        ProjectFeatureHubPage = projectFeatureHubPage;
        MediaStudioPage = mediaStudioPage;
        AgentWorkspacePage = agentWorkspacePage;
        ProjectFilesystem = projectFilesystem;
        InitializeComponent();

        ExtendsContentIntoTitleBar = true;
        // The login view is the first visible root. Registering a title-bar element from the
        // collapsed workspace can leave WinUI's non-client input source pointing at a hidden
        // visual and has caused CoreMessagingXP failures during the first sign-in transition.
        SetTitleBar(LoginTitleBar);
        AppWindow.Resize(new SizeInt32(1440, 900));
        AppWindow.Title = "ChatOS";
        ShowContent(WorkspaceHost);
        SettingsPage.CloseRequested += OnSettingsCloseRequested;
        SettingsPage.Connector.PropertyChanged += OnConnectorSettingsPropertyChanged;
        RemoteConnectionsPage.OpenSftpRequested += OnOpenSftpRequested;
        RemoteConnectionsPage.OpenTerminalRequested += OnOpenTerminalRequested;
        RemoteSftpPage.CloseRequested += OnRemoteSftpCloseRequested;
        RemoteTerminalPage.CloseRequested += OnRemoteTerminalCloseRequested;
        LocalTerminalPage.CloseRequested += OnLocalTerminalCloseRequested;
        LocalTerminalPage.NewTerminalRequested += OnLocalTerminalNewRequested;
        NotepadPage.CloseRequested += OnNotepadCloseRequested;
        Preferences.Changed += OnPreferencesChanged;
        ViewModel.PropertyChanged += OnViewModelPropertyChanged;
        ChatOSApiClient.AuthenticationExpired += OnAuthenticationExpired;
        ViewModel.Projects.CollectionChanged += (_, _) => RefreshSidebarState();
        ViewModel.RemoteResources.CollectionChanged += (_, _) => RefreshSidebarState();
        WorkspaceHost.ProjectTabRequested += async (_, tab) => await ViewModel.OpenProjectTabAsync(tab);
        WorkspaceHost.ProjectRemoveRequested += OnProjectRemoveRequested;
        PluginApplicationsPage.OpenPluginManagementRequested += (_, _) =>
        {
            SettingsPage.OpenSection("plugins");
            ShowSettings();
        };
        ProjectFeatureHubPage.FeatureRequested += OnProjectFeatureRequested;
        AgentWorkspacePage.FeatureRequested += OnProjectFeatureRequested;
        AgentWorkspacePage.OpenSkillManagementRequested += (_, _) =>
        {
            SettingsPage.OpenSection("plugins");
            ShowSettings();
        };
        Approvals.PendingChanged += OnPendingApprovalsChanged;
        Activated += OnActivated;
        // Defer the maximize until after the Activated handler is subscribed;
        // resizing the overlapped presenter synchronously can otherwise consume
        // the first activation and skip session restoration/pet loading.
        _ = DispatcherQueue.TryEnqueue(() =>
        {
            if (AppWindow.Presenter is OverlappedPresenter presenter)
            {
                presenter.Maximize();
            }
        });
    }

    private PluginApplicationsPage PluginApplicationsPage { get; }

    private ProjectFeatureHubPage ProjectFeatureHubPage { get; }

    private MediaStudioPage MediaStudioPage { get; }

    private AgentWorkspacePage AgentWorkspacePage { get; }

    private IProjectFilesystemService ProjectFilesystem { get; }

    public MainWindowViewModel ViewModel { get; }

    private void OnAuthenticationExpired(object? sender, EventArgs e)
    {
        // API requests complete on background threads. All account-scoped observable
        // collections and WinUI controls must be reset on the window's UI queue.
        _ = DispatcherQueue.TryEnqueue(() =>
            _ = ViewModel.HandleAuthenticationExpiredAsync());
    }

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

    public ClipboardHistoryWindow ClipboardHistoryWindow { get; }

    private ConnectorPendingApproval? ActiveApproval { get; set; }

    private async void OnActivated(object sender, WindowActivatedEventArgs args)
    {
        Activated -= OnActivated;
        try
        {
            StartupDiagnostics.RecordStage("main window activation handler entered");
            ApplyAppearance(Preferences.Current);
            UpdateVisualState();
            StartupDiagnostics.RecordStage("initial visual state applied");
            await LoadLoginPetSpriteAsync();
            StartupDiagnostics.RecordStage("login pet sprite synchronized");
            await ViewModel.InitializeAsync();
            StartupDiagnostics.RecordStage("account session restoration completed");
            await PetWindowController.SetAuthenticatedAsync(ViewModel.IsAuthenticated);
            StartupDiagnostics.RecordStage("pet window state synchronized");
            await VisualSessionController.SetAuthenticatedAsync(ViewModel.IsAuthenticated);
            StartupDiagnostics.RecordStage("plugin visual session state synchronized");
            try
            {
                await Approvals.InitializeAsync();
                StartupDiagnostics.RecordStage("approval coordinator initialized");
            }
            catch (Exception exception)
            {
                ViewModel.ErrorMessage = ViewModel.Localization.Text(
                    $"加载本机审批状态失败：{exception.Message}",
                    $"Unable to load local approval state: {exception.Message}");
            }
            UpdateVisualState();
            _postActivationInitializationComplete = true;
            StartupDiagnostics.RecordStage("main window post-activation initialization complete");
        }
        catch (Exception exception)
        {
            // Keep the authenticated shell alive when an optional secondary window,
            // Connector, approval store, or local-agent integration fails. macOS keeps the
            // login/workspace phase usable and surfaces the failure in-app; exiting here made
            // Windows look like a crash and prevented the user from retrying the affected tool.
            StartupDiagnostics.ReportFatal("main window post-activation initialization", exception);
            ViewModel.ErrorMessage = ViewModel.Localization.Text(
                $"加载桌面功能失败：{exception.Message}",
                $"A desktop feature failed to initialize: {exception.Message}");
            _postActivationInitializationComplete = true;
            UpdateVisualState();
        }
    }

    private async Task LoadLoginPetSpriteAsync()
    {
        var frames = await PetSpriteAtlas.LoadAsync();
        if (frames is not { Count: > 0 })
        {
            LoginPetSpriteImage.Visibility = Visibility.Collapsed;
            LoginPetFallback.Visibility = Visibility.Collapsed;
            StartupDiagnostics.RecordLifecycle("login pet sprite atlas unavailable; no legacy fallback image shown");
            return;
        }

        LoginPetSpriteImage.Source = frames[0];
        LoginPetSpriteImage.Visibility = Visibility.Visible;
        LoginPetFallback.Visibility = Visibility.Collapsed;
        StartupDiagnostics.RecordLifecycle("login pet sprite loaded from fengtuan idle frame");
    }

    private void OnPasswordChanged(object sender, RoutedEventArgs e)
    {
        ViewModel.Password = ((PasswordBox)sender).Password;
    }

    private async void OnLoginClicked(object sender, RoutedEventArgs e)
    {
        // PasswordBox does not expose a bindable Password property.  Native
        // UI automation can update the control without raising PasswordChanged
        // before the button command runs, so take the authoritative value from
        // the control at submit time.
        await Task.Yield();
        ViewModel.Password = LoginPasswordBox.Password;
        await ViewModel.LoginCommand.ExecuteAsync(null);
    }

    private void OnConfirmPasswordChanged(object sender, RoutedEventArgs e)
    {
        ViewModel.ConfirmPassword = ((PasswordBox)sender).Password;
    }

    private async void OnSidebarSelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_restoringSidebarSelection)
        {
            return;
        }

        if (e.AddedItems.FirstOrDefault() is not ShellResourceViewModel selected) return;
        if (ViewModel.SelectedResource is { } current &&
            current.Kind == selected.Kind && current.Id == selected.Id) return;

        var previous = ViewModel.SelectedResource;
        _sidebarSelectionPending = true;
        try
        {
            if (!await WorkspaceHost.ConfirmCanChangeResourceAsync())
            {
                if (sender is ListView list)
                {
                    _restoringSidebarSelection = true;
                    try
                    {
                        list.SelectedItem = previous is not null && list.Items.Contains(previous)
                            ? previous
                            : null;
                    }
                    finally
                    {
                        _restoringSidebarSelection = false;
                    }
                }
                return;
            }

            ViewModel.SelectedResource = selected;
        }
        finally
        {
            _sidebarSelectionPending = false;
        }
    }

    private void OnSidebarItemClicked(object sender, ItemClickEventArgs e)
    {
        if (_sidebarSelectionPending) return;
        if (e.ClickedItem is not ShellResourceViewModel selected) return;
        if (ViewModel.SelectedResource is not { } current ||
            current.Kind != selected.Kind || current.Id != selected.Id)
        {
            ViewModel.SelectedResource = selected;
            return;
        }

        // Separate sidebar ListViews can retain their own selected item after another section
        // opens. ItemClick still fires for that retained row, so explicitly reopen a project and
        // restore its default directory/files view even when SelectionChanged does not fire.
        if (selected.Kind == WorkspaceResourceKind.Project)
        {
            _lastWorkspaceResource = selected;
            ShowWorkspace();
        }
    }

    private void OnViewModelPropertyChanged(object? sender, PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(MainWindowViewModel.SelectedResource) && ViewModel.IsPublishingWorkspace) return;
        var authenticationChanged = e.PropertyName == nameof(MainWindowViewModel.IsAuthenticated);
        if (e.PropertyName is nameof(MainWindowViewModel.IsAuthenticated) or
            nameof(MainWindowViewModel.IsBusy) or
            nameof(MainWindowViewModel.ErrorMessage) or
            nameof(MainWindowViewModel.Password) or
            nameof(MainWindowViewModel.ConfirmPassword) or
            nameof(MainWindowViewModel.IsRegistrationMode) or
            nameof(MainWindowViewModel.RegistrationMessage) or
            nameof(MainWindowViewModel.SelectedResource))
        {
            if (e.PropertyName == nameof(MainWindowViewModel.SelectedResource))
            {
                var preservesActiveProjectTab =
                    ViewModel.IsPreparingConversation &&
                    ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.Project &&
                    _lastWorkspaceResource?.Kind == WorkspaceResourceKind.Project &&
                    string.Equals(
                        ViewModel.SelectedResource.Id,
                        _lastWorkspaceResource.Id,
                        StringComparison.Ordinal);
                if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.LocalConnector)
                {
                    ShowSettings();
                }
                else if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.Applications)
                {
                    ShowContent(PluginApplicationsPage);
                    _ = PluginApplicationsPage.OpenAsync();
                }
                else if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.AgentTeams)
                {
                    ShowContent(AgentWorkspacePage);
                }
                else if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.RequirementSurveys)
                {
                    ProjectFeatureHubPage.Configure(ViewModel.SelectedResource.Kind);
                    ShowContent(ProjectFeatureHubPage);
                }
                else if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.MediaStudio)
                {
                    StartupDiagnostics.RecordLifecycle("media studio selection received");
                    ShowContent(MediaStudioPage);
                    StartupDiagnostics.RecordLifecycle("media studio content attached");
                }
                else if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.RemoteConnection)
                {
                    ShowContent(RemoteConnectionsPage);
                }
                else if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.LocalTerminal)
                {
                    _ = OpenLocalTerminalAsync(ViewModel.SelectedResource);
                }
                else
                {
                    _lastWorkspaceResource = ViewModel.SelectedResource;
                    if (!preservesActiveProjectTab)
                    {
                        ShowWorkspace();
                    }
                }
            }
            if (authenticationChanged && !ViewModel.IsAuthenticated)
            {
                _ = LocalTerminalPage.CloseSessionAsync();
                _ = PluginApplicationsPage.ResetAsync();
                MediaStudioPage.ResetSession();
            }
            UpdateVisualState();
            if (authenticationChanged && _postActivationInitializationComplete)
            {
                // Defer secondary-window activation until the main window has completed its
                // title-bar/root transition. Activating the pet window re-entrantly from the
                // property notification can destabilize WinUI's CoreMessaging input source.
                _ = DispatcherQueue.TryEnqueue(() =>
                    _ = SynchronizeAuthenticatedWindowsAsync(ViewModel.IsAuthenticated));
            }
        }
    }

    private async Task SynchronizeAuthenticatedWindowsAsync(bool authenticated)
    {
        try
        {
            await PetWindowController.SetAuthenticatedAsync(authenticated);
        }
        catch (Exception exception)
        {
            StartupDiagnostics.RecordUnhandled("pet window authentication synchronization", exception);
        }

        try
        {
            await VisualSessionController.SetAuthenticatedAsync(authenticated);
        }
        catch (Exception exception)
        {
            StartupDiagnostics.RecordUnhandled("visual session authentication synchronization", exception);
        }
    }

    private void OnSidebarRetryClicked(object sender, RoutedEventArgs e) =>
        ViewModel.RefreshWorkspaceCommand.Execute(null);

    private void OnProjectFeatureRequested(object? sender, ProjectFeatureRequestedEventArgs e)
    {
        ViewModel.SelectedResource = e.Project;
        ShowWorkspace();
        WorkspaceHost.OpenProjectFeature(e.Tab);
    }

    private void OnSettingsClicked(object sender, RoutedEventArgs e)
    {
        ShowSettings();
    }

    private async void OnNotepadClicked(object sender, RoutedEventArgs e)
    {
        ShowAuxiliaryContent(NotepadPage);
        await NotepadPage.ViewModel.OpenAsync();
    }

    private async void OnArtifactsClicked(object sender, RoutedEventArgs e) =>
        await ArtifactsWindow.ShowAsync();

    private async void OnClipboardHistoryClicked(object sender, RoutedEventArgs e) =>
        await ClipboardHistoryWindow.ShowAsync();

    private void OnNotepadCloseRequested(object? sender, EventArgs e)
    {
        _ = NotepadPage.ViewModel.CloseAsync();
        HideAuxiliaryContent();
    }

    private async void OnSettingsCloseRequested(object? sender, EventArgs e)
    {
        await ViewModel.RefreshLocalConnectorAsync();
        if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.LocalConnector)
        {
            ViewModel.SelectedResource = _lastWorkspaceResource;
        }
        HideAuxiliaryContent();
    }

    public void OpenSettings() => ShowSettings();

    private void ShowSettings() => ShowAuxiliaryContent(SettingsPage);

    private async void OnOpenSftpRequested(object? sender, RemoteConnection connection)
    {
        ShowContent(RemoteSftpPage);
        await RemoteSftpPage.OpenAsync(connection);
    }

    private void OnRemoteSftpCloseRequested(object? sender, EventArgs e) =>
        ShowContent(RemoteConnectionsPage);

    private void OnOpenTerminalRequested(object? sender, RemoteConnection connection)
    {
        ShowContent(RemoteTerminalPage);
        RemoteTerminalPage.Open(connection);
    }

    private void OnRemoteTerminalCloseRequested(object? sender, EventArgs e) =>
        ShowContent(RemoteConnectionsPage);

    private async Task OpenLocalTerminalAsync(ShellResourceViewModel resource)
    {
        try
        {
            ShowContent(LocalTerminalPage);
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

    private async void OnLocalTerminalNewRequested(object? sender, EventArgs e) =>
        await CreateLocalTerminalAsync();

    private async void OnCreateLocalProjectClicked(object sender, RoutedEventArgs e)
    {
        var account = ViewModel.AccountGeneration;
        try
        {
            await ViewModel.RefreshLocalConnectorAsync();
            if (account != ViewModel.AccountGeneration || !ViewModel.IsAuthenticated) return;
            var workspaces = ViewModel.LocalConnectorStatus?.Workspaces ?? [];
            if (workspaces.Count == 0)
            {
                await ShowConnectorSetupRequiredAsync(forProject: true);
                return;
            }
            using var createViewModel = new CreateProjectViewModel(
                ViewModel.LocalConnectorStatus,
                ProjectFilesystem,
                ViewModel.Localization);
            var content = new CreateProjectDialogContent(createViewModel);
            var dialog = new AutomationContentDialog
            {
                XamlRoot = RootGrid.XamlRoot, Title = ViewModel.Localization.CreateLocalProject,
                Content = content, PrimaryButtonText = ViewModel.Localization.Text("创建", "Create"),
                CloseButtonText = ViewModel.Localization.Text("取消", "Cancel"),
                DefaultButton = ContentDialogButton.Primary,
                IsPrimaryButtonEnabled = false,
                MinWidth = 760,
                MaxWidth = 780,
            };
            content.StateChanged += (_, _) =>
                dialog.IsPrimaryButtonEnabled = createViewModel.CanCreate;
            dialog.Opened += (_, _) =>
            {
                dialog.SetButtonAutomationProperties(
                    "PrimaryButton",
                    "ChatOS.CreateProject.Submit",
                    ViewModel.Localization.Text("创建项目", "Create project"));
                dialog.SetButtonAutomationProperties(
                    "CloseButton",
                    "ChatOS.CreateProject.Cancel",
                    ViewModel.Localization.Text("取消创建项目", "Cancel project creation"));
            };
            dialog.PrimaryButtonClick += async (_, args) =>
            {
                var deferral = args.GetDeferral();
                args.Cancel = true;
                try
                {
                    dialog.IsPrimaryButtonEnabled = false;
                    var created = await createViewModel.SaveAsync(
                        (draft, root, _) => ViewModel.CreateProjectAsync(account, draft, root));
                    args.Cancel = !created;
                }
                finally
                {
                    dialog.IsPrimaryButtonEnabled = createViewModel.CanCreate;
                    deferral.Complete();
                }
            };
            await content.LoadAsync();
            dialog.IsPrimaryButtonEnabled = createViewModel.CanCreate;
            _ = await dialog.ShowAsync();
        }
        catch (OperationCanceledException) { }
        catch (Exception exception)
        {
            if (account == ViewModel.AccountGeneration) ViewModel.ErrorMessage = exception.Message;
        }
    }

    private async void OnRenameLocalProjectClicked(object sender, RoutedEventArgs e)
    {
        var selected = ResolveProjectResource(sender);
        if (selected is null) return;
        var account = ViewModel.AccountGeneration;
        try
        {
            var project = await ViewModel.GetLocalProjectAsync(account, selected.Id);
            var name = new TextBox
            {
                Header = ViewModel.Localization.Text("项目名称", "Project name"),
                Text = project.Draft.Name,
                MinWidth = 380,
                SelectionStart = 0,
                SelectionLength = project.Draft.Name.Length,
            };
            var error = new TextBlock
            {
                Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["ChatOSFailureBrush"],
                TextWrapping = TextWrapping.Wrap,
                Visibility = Visibility.Collapsed,
            };
            var content = new StackPanel { Spacing = 10 };
            content.Children.Add(name);
            content.Children.Add(error);
            var dialog = new ContentDialog
            {
                XamlRoot = RootGrid.XamlRoot,
                Title = ViewModel.Localization.Text("重命名本地项目", "Rename local project"),
                Content = content, PrimaryButtonText = ViewModel.Localization.Text("保存", "Save"),
                CloseButtonText = ViewModel.Localization.Text("取消", "Cancel"),
                DefaultButton = ContentDialogButton.Primary,
                IsPrimaryButtonEnabled = project.Draft.Name.Trim().Length > 0,
            };
            name.TextChanged += (_, _) =>
                dialog.IsPrimaryButtonEnabled = name.Text.Trim().Length > 0;
            dialog.PrimaryButtonClick += async (_, args) =>
            {
                var deferral = args.GetDeferral();
                args.Cancel = true;
                dialog.IsPrimaryButtonEnabled = false;
                error.Visibility = Visibility.Collapsed;
                try
                {
                    await ViewModel.RenameProjectAsync(account, project, name.Text.Trim());
                    args.Cancel = false;
                }
                catch (Exception exception)
                {
                    error.Text = exception.Message;
                    error.Visibility = Visibility.Visible;
                }
                finally
                {
                    dialog.IsPrimaryButtonEnabled = name.Text.Trim().Length > 0;
                    deferral.Complete();
                }
            };
            _ = await dialog.ShowAsync();
        }
        catch (OperationCanceledException) { }
        catch (Exception exception)
        {
            if (account == ViewModel.AccountGeneration) ViewModel.ErrorMessage = exception.Message;
        }
    }

    private async void OnRemoveLocalProjectClicked(object sender, RoutedEventArgs e)
    {
        var selected = ResolveProjectResource(sender);
        if (selected is null) return;
        await RemoveLocalProjectAsync(selected);
    }

    private async void OnProjectRemoveRequested(object? sender, EventArgs e)
    {
        if (ViewModel.SelectedResource is not { Kind: WorkspaceResourceKind.Project } selected) return;
        await RemoveLocalProjectAsync(selected);
    }

    private async Task RemoveLocalProjectAsync(ShellResourceViewModel selected)
    {
        var account = ViewModel.AccountGeneration;
        try
        {
            var project = await ViewModel.GetLocalProjectAsync(account, selected.Id);
            var dialog = new ContentDialog
            {
                XamlRoot = RootGrid.XamlRoot,
                Title = ViewModel.Localization.Text("删除项目？", "Delete project?"),
                Content = ViewModel.Localization.Text(
                    $"“{project.Draft.Name}”会从本机项目列表移除，不会删除文件夹、聊天历史或插件数据。",
                    $"'{project.Draft.Name}' will be removed from this device's project list. Files, conversations and plugin data will be kept."),
                PrimaryButtonText = ViewModel.Localization.Delete,
                CloseButtonText = ViewModel.Localization.Text("取消", "Cancel"),
                DefaultButton = ContentDialogButton.Close,
            };
            if (await dialog.ShowAsync() == ContentDialogResult.Primary)
                await ViewModel.RemoveProjectAsync(account, project);
        }
        catch (OperationCanceledException) { }
        catch (Exception exception)
        {
            if (account != ViewModel.AccountGeneration) return;
            ViewModel.ErrorMessage = exception.Message;
            var failure = new ContentDialog
            {
                XamlRoot = RootGrid.XamlRoot,
                Title = ViewModel.Localization.Text("项目删除失败", "Project deletion failed"),
                Content = exception.Message,
                CloseButtonText = ViewModel.Localization.Text("好", "OK"),
                DefaultButton = ContentDialogButton.Close,
            };
            _ = await failure.ShowAsync();
        }
    }

    private ShellResourceViewModel? ResolveProjectResource(object sender)
    {
        if (sender is FrameworkElement { Tag: string projectId })
        {
            return ViewModel.Projects.FirstOrDefault(project =>
                string.Equals(project.Id, projectId, StringComparison.Ordinal));
        }

        return ViewModel.SelectedResource is { Kind: WorkspaceResourceKind.Project } selected
            ? selected
            : null;
    }

    private async void OnCreateLocalTerminalClicked(object sender, RoutedEventArgs e)
    {
        await CreateLocalTerminalAsync();
    }

    private async Task CreateLocalTerminalAsync()
    {
        await ViewModel.RefreshLocalConnectorAsync();
        var workspaces = ViewModel.LocalConnectorStatus?.Workspaces ?? [];
        if (workspaces.Count == 0)
        {
            await ShowConnectorSetupRequiredAsync(forProject: false);
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

    private async Task ShowConnectorSetupRequiredAsync(bool forProject)
    {
        var paired = ViewModel.LocalConnectorStatus?.IsPaired == true;
        var action = forProject
            ? ViewModel.Localization.Text("创建项目", "create a project")
            : ViewModel.Localization.Text("新建终端", "create a terminal");
        var dialog = new ContentDialog
        {
            XamlRoot = RootGrid.XamlRoot,
            Title = paired
                ? ViewModel.Localization.Text("需要授权工作区", "An authorized workspace is required")
                : ViewModel.Localization.Text("本机 Connector 尚未就绪", "Local Connector is not ready"),
            Content = paired
                ? ViewModel.Localization.Text(
                    $"要{action}，请先为本机 Connector 添加一个授权工作区。",
                    $"To {action}, add an authorized workspace to the Local Connector first.")
                : ViewModel.Localization.Text(
                    $"要{action}，请先配对本机 Connector 并授权一个工作区。",
                    $"To {action}, pair the Local Connector and authorize a workspace first."),
            PrimaryButtonText = ViewModel.Localization.Text("打开 Connector 设置", "Open Connector settings"),
            CloseButtonText = ViewModel.Localization.Text("取消", "Cancel"),
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await dialog.ShowAsync() == ContentDialogResult.Primary)
        {
            ShowSettings();
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
        ShowContent(WorkspaceHost);
        WorkspaceHost.Configure(ViewModel.SelectedResource);
    }

    private void RestoreSelectedContent()
    {
        if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.Applications)
        {
            ShowContent(PluginApplicationsPage);
            _ = PluginApplicationsPage.OpenAsync();
            return;
        }
        if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.AgentTeams)
        {
            ShowContent(AgentWorkspacePage);
            return;
        }
        if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.RequirementSurveys)
        {
            ProjectFeatureHubPage.Configure(ViewModel.SelectedResource.Kind);
            ShowContent(ProjectFeatureHubPage);
            return;
        }
        if (ViewModel.SelectedResource?.Kind == WorkspaceResourceKind.MediaStudio)
        {
            ShowContent(MediaStudioPage);
            return;
        }
        ShowWorkspace();
    }

    private void ShowContent(object? content)
    {
        WorkspaceContent.Content = content;
        RefreshWorkspaceEmptyState();
    }

    private void ShowAuxiliaryContent(object content)
    {
        var isSettings = ReferenceEquals(content, SettingsPage);
        AuxiliaryOverlayCard.MaxWidth = isSettings ? 840 : 1300;
        AuxiliaryOverlayCard.MaxHeight = isSettings ? 640 : 840;
        AuxiliaryOverlayContent.Content = content;
        AuxiliaryOverlay.Visibility = Visibility.Visible;
    }

    private void HideAuxiliaryContent()
    {
        AuxiliaryOverlayCard.MaxWidth = 1300;
        AuxiliaryOverlayCard.MaxHeight = 840;
        AuxiliaryOverlay.Visibility = Visibility.Collapsed;
        AuxiliaryOverlayContent.Content = null;
    }

    private void RefreshSidebarState()
    {
        if (ProjectsEmptyText is null || RemoteEmptyText is null) return;
        ProjectsEmptyText.Visibility = !ViewModel.IsBusy && ViewModel.Projects.Count == 0
            ? Visibility.Visible
            : Visibility.Collapsed;
        RemoteEmptyText.Visibility = !ViewModel.IsBusy && ViewModel.RemoteResources.Count == 0
            ? Visibility.Visible
            : Visibility.Collapsed;
    }

    private void RefreshWorkspaceEmptyState()
    {
        if (EmptyWorkspaceState is null) return;
        var isEmpty = ReferenceEquals(WorkspaceContent.Content, WorkspaceHost) &&
            ViewModel.SelectedResource is null;
        EmptyWorkspaceState.Visibility = isEmpty ? Visibility.Visible : Visibility.Collapsed;
        WorkspaceContent.Visibility = isEmpty ? Visibility.Collapsed : Visibility.Visible;
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
        Application.Current.Resources["ChatOSFontSizePageTitle"] = 26d * preferences.FontScale;
    }

    private void UpdateVisualState()
    {
        ApplyAuthenticationVisualState(ViewModel.IsAuthenticated);
        LoginForm.Visibility = ViewModel.IsRegistrationMode ? Visibility.Collapsed : Visibility.Visible;
        RegistrationForm.Visibility = ViewModel.IsRegistrationMode ? Visibility.Visible : Visibility.Collapsed;
        // Keep the native control state aligned with the bindable validation
        // property; this method also runs during startup before x:Bind has
        // delivered the first property notification.
        LoginButton.IsEnabled = ViewModel.CanLogin;
        LoginErrorText.Visibility = string.IsNullOrWhiteSpace(ViewModel.ErrorMessage)
            ? Visibility.Collapsed
            : Visibility.Visible;
        RegistrationErrorText.Visibility = LoginErrorText.Visibility;
        RegistrationStatusText.Visibility = string.IsNullOrWhiteSpace(ViewModel.RegistrationMessage)
            ? Visibility.Collapsed
            : Visibility.Visible;
        SidebarErrorCard.Visibility = string.IsNullOrWhiteSpace(ViewModel.ErrorMessage)
            ? Visibility.Collapsed
            : Visibility.Visible;
        if (ViewModel.Password.Length == 0 && LoginPasswordBox.Password.Length != 0)
        {
            LoginPasswordBox.Password = string.Empty;
        }
        if (ViewModel.Password.Length == 0 && RegistrationPasswordBox.Password.Length != 0)
        {
            RegistrationPasswordBox.Password = string.Empty;
        }
        if (ViewModel.ConfirmPassword.Length == 0 && RegistrationConfirmPasswordBox.Password.Length != 0)
        {
            RegistrationConfirmPasswordBox.Password = string.Empty;
        }

        WorkspaceHost.Configure(ViewModel.SelectedResource);
        RefreshSidebarState();
        RefreshWorkspaceEmptyState();
        RefreshApprovalOverlay();
    }

    private void ApplyAuthenticationVisualState(bool authenticated)
    {
        if (_renderedAuthenticationState == authenticated)
        {
            return;
        }

        StartupDiagnostics.RecordLifecycle(
            $"applying {(authenticated ? "authenticated" : "signed-out")} window state");

        // A custom title-bar element must stay visible until its replacement is registered.
        // Make the incoming root visible, switch the title bar, and only then collapse the
        // outgoing root. The previous order temporarily hid the active title bar and could
        // terminate the process inside CoreMessagingXP when login completed.
        if (authenticated)
        {
            ShellRoot.Visibility = Visibility.Visible;
            SetTitleBar(AppTitleBar);
            LoginRoot.Visibility = Visibility.Collapsed;
        }
        else
        {
            HideAuxiliaryContent();
            LoginRoot.Visibility = Visibility.Visible;
            SetTitleBar(LoginTitleBar);
            ShellRoot.Visibility = Visibility.Collapsed;
        }

        _renderedAuthenticationState = authenticated;
        StartupDiagnostics.RecordLifecycle("window authentication state applied");
    }

    private void OnPendingApprovalsChanged(object? sender, EventArgs e)
    {
        _ = DispatcherQueue.TryEnqueue(RefreshApprovalOverlay);
    }

    private void RefreshApprovalOverlay()
    {
        var previousApprovalId = ActiveApproval?.Id;
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
        if (!string.Equals(previousApprovalId, approval.Id, StringComparison.Ordinal))
        {
            _ = DeclineApprovalButton.Focus(FocusState.Programmatic);
        }
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

    private sealed class AutomationContentDialog : ContentDialog
    {
        public void SetButtonAutomationProperties(string templatePart, string automationId, string name)
        {
            if (GetTemplateChild(templatePart) is not Button button) return;
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetAutomationId(button, automationId);
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(button, name);
        }
    }
}
