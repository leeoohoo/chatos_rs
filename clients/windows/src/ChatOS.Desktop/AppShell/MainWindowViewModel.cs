using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Remote;

namespace ChatOS.Desktop.AppShell;

public sealed partial class MainWindowViewModel : ObservableObject
{
    private readonly IAuthenticationService _authenticationService;
    private readonly IWorkspaceService _workspaceService;
    private readonly IWorkspaceResourceCreationService _resourceCreationService;
    private readonly ILocalConnectorControlService _localConnectorControl;
    private WorkspaceSnapshot _workspaceSnapshot = WorkspaceSnapshot.Empty;
    private CancellationTokenSource? _selectionCancellation;
    private readonly SemaphoreSlim _conversationPreparationGate = new(1, 1);
    private bool _suppressSelectionActivation;

    public MainWindowViewModel(
        IAuthenticationService authenticationService,
        IWorkspaceService workspaceService,
        IWorkspaceResourceCreationService resourceCreationService,
        ILocalConnectorControlService localConnectorControl,
        ConversationSessionViewModel conversation,
        ProjectFilesViewModel projectFiles,
        ProjectGitViewModel projectGit,
        ProjectPlanViewModel projectPlan,
        ProjectRunViewModel projectRun,
        RemoteConnectionsViewModel remoteConnections,
        LocalizationViewModel localization)
    {
        _authenticationService = authenticationService;
        _workspaceService = workspaceService;
        _resourceCreationService = resourceCreationService;
        _localConnectorControl = localConnectorControl;
        Conversation = conversation;
        ProjectFiles = projectFiles;
        ProjectGit = projectGit;
        ProjectPlan = projectPlan;
        ProjectRun = projectRun;
        RemoteConnections = remoteConnections;
        Localization = localization;
        RemoteConnections.Connections.CollectionChanged += (_, _) => RebuildRemoteResources();
        Localization.PropertyChanged += (_, _) => RelocalizeResources();
    }

    public ConversationSessionViewModel Conversation { get; }

    public ProjectFilesViewModel ProjectFiles { get; }

    public ProjectGitViewModel ProjectGit { get; }

    public ProjectPlanViewModel ProjectPlan { get; }

    public ProjectRunViewModel ProjectRun { get; }

    public RemoteConnectionsViewModel RemoteConnections { get; }

    public LocalizationViewModel Localization { get; }

    public ObservableCollection<ShellResourceViewModel> Contacts { get; } = [];

    public ObservableCollection<ShellResourceViewModel> Projects { get; } = [];

    public ObservableCollection<ShellResourceViewModel> LocalResources { get; } = [];

    public ObservableCollection<ShellResourceViewModel> RemoteResources { get; } = [];

    public LocalConnectorStatus? LocalConnectorStatus { get; private set; }

    public ShellResourceViewModel CreateLocalTerminalResource(LocalConnectorWorkspaceStatus workspace)
    {
        ArgumentNullException.ThrowIfNull(workspace);
        var resource = new ShellResourceViewModel(
            $"local-terminal:{Guid.NewGuid():N}",
            WorkspaceResourceKind.LocalTerminal,
            $"{workspace.Alias} · {Localization.Terminal}",
            workspace.AbsoluteRoot,
            "\uE756",
            WorkspaceId: workspace.Id,
            AbsoluteRoot: workspace.AbsoluteRoot);
        LocalResources.Add(resource);
        SelectedResource = resource;
        return resource;
    }

    public async Task<string> EnsureProjectConversationAsync(
        string projectId,
        CancellationToken cancellationToken = default)
    {
        await _conversationPreparationGate.WaitAsync(cancellationToken);
        try
        {
            var resource = Projects.FirstOrDefault(value =>
                value.Kind == WorkspaceResourceKind.Project &&
                string.Equals(value.Id, projectId, StringComparison.Ordinal));
            if (resource?.ConversationId is { Length: > 0 } existing) return existing;
            var project = _workspaceSnapshot.Projects.FirstOrDefault(value =>
                string.Equals(value.Id, projectId, StringComparison.Ordinal))
                ?? throw new KeyNotFoundException(Localization.Text(
                    "所选项目已不在当前工作区中。",
                    "The selected project is no longer in the current workspace."));
            var contact = _workspaceSnapshot.Contacts.FirstOrDefault(value =>
                    string.Equals(value.AgentId, "jiguli", StringComparison.OrdinalIgnoreCase))
                ?? _workspaceSnapshot.Contacts.FirstOrDefault()
                ?? throw new InvalidOperationException(Localization.Text(
                    "没有找到可用于项目会话的联系人。",
                    "No contact is available for the project conversation."));
            var conversationId = await _resourceCreationService.EnsureConversationAsync(
                project,
                contact,
                cancellationToken);
            var resourceIndex = Projects.ToList().FindIndex(value =>
                string.Equals(value.Id, projectId, StringComparison.Ordinal));
            if (resourceIndex >= 0)
            {
                Projects[resourceIndex] = Projects[resourceIndex] with { ConversationId = conversationId };
            }

            _workspaceSnapshot = _workspaceSnapshot with
            {
                Projects = _workspaceSnapshot.Projects.Select(value =>
                    string.Equals(value.Id, projectId, StringComparison.Ordinal)
                        ? value with { LatestConversationId = conversationId }
                        : value).ToArray(),
            };
            return conversationId;
        }
        finally
        {
            _conversationPreparationGate.Release();
        }
    }

    [ObservableProperty]
    private ShellResourceViewModel? _selectedResource;

    [ObservableProperty]
    private string _workspaceTitle = "ChatOS";

    [ObservableProperty]
    private string _username = string.Empty;

    [ObservableProperty]
    private string _password = string.Empty;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string _currentUserLabel = string.Empty;

    [ObservableProperty]
    private bool _isAuthenticated;

    [ObservableProperty]
    private bool _isBusy;

    [ObservableProperty]
    private bool _isInitialized;

    [ObservableProperty]
    private bool _isPreparingConversation;

    public async Task InitializeAsync(CancellationToken cancellationToken = default)
    {
        if (IsInitialized)
        {
            return;
        }

        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var session = await _authenticationService.RestoreSessionAsync(cancellationToken);
            if (session is not null)
            {
                ApplySession(session);
                await ReloadWorkspaceCoreAsync(cancellationToken);
            }
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsBusy = false;
            IsInitialized = true;
        }
    }

    [RelayCommand]
    private async Task LoginAsync()
    {
        if (IsBusy)
        {
            return;
        }

        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var session = await _authenticationService.LoginAsync(Username, Password);
            Password = string.Empty;
            ApplySession(session);
            await ReloadWorkspaceCoreAsync();
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsBusy = false;
        }
    }

    [RelayCommand]
    private async Task RefreshWorkspaceAsync()
    {
        if (!IsAuthenticated || IsBusy)
        {
            return;
        }

        IsBusy = true;
        ErrorMessage = null;
        try
        {
            await ReloadWorkspaceCoreAsync();
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
        }
        finally
        {
            IsBusy = false;
        }
    }

    [RelayCommand]
    private async Task LogoutAsync()
    {
        CancelSelectionActivation();
        await _authenticationService.LogoutAsync();
        Contacts.Clear();
        Projects.Clear();
        LocalResources.Clear();
        RemoteResources.Clear();
        SelectedResource = null;
        await ProjectRun.CloseAsync();
        await ProjectGit.CloseAsync();
        await Conversation.OpenAsync(null, "ChatOS");
        CurrentUserLabel = string.Empty;
        IsAuthenticated = false;
        ErrorMessage = null;
    }

    partial void OnSelectedResourceChanged(ShellResourceViewModel? value)
    {
        WorkspaceTitle = value?.Title ?? "ChatOS";
        if (_suppressSelectionActivation) return;
        CancelSelectionActivation();
        _selectionCancellation = new CancellationTokenSource();
        _ = ActivateResourceSafelyAsync(value, _selectionCancellation.Token);
    }

    private void ApplySession(AuthSession session)
    {
        CurrentUserLabel = session.User.EffectiveDisplayName;
        IsAuthenticated = true;
    }

    private async Task ReloadWorkspaceCoreAsync(CancellationToken cancellationToken = default)
    {
        var snapshot = await _workspaceService.FetchWorkspaceAsync(cancellationToken);
        _workspaceSnapshot = snapshot;
        var activeConversations = snapshot.Conversations
            .Where(static conversation => !conversation.IsArchived)
            .OrderByDescending(static conversation => conversation.UpdatedAt)
            .ToArray();

        Contacts.Clear();
        foreach (var contact in snapshot.Contacts)
        {
            var conversation = activeConversations.FirstOrDefault(value =>
                value.ProjectId is null &&
                (string.Equals(value.ContactId, contact.Id, StringComparison.Ordinal) ||
                 string.Equals(value.ContactAgentId, contact.AgentId, StringComparison.Ordinal)));
            Contacts.Add(new ShellResourceViewModel(
                contact.Id,
                WorkspaceResourceKind.Contact,
                contact.Name,
                ContactSubtitle(contact.Status),
                "\uE77B",
                conversation?.Id));
        }

        if (!snapshot.Contacts.Any(static contact =>
                string.Equals(contact.AgentId, "jiguli", StringComparison.OrdinalIgnoreCase)))
        {
            var conversation = activeConversations.FirstOrDefault(value =>
                string.Equals(value.ContactAgentId, "jiguli", StringComparison.OrdinalIgnoreCase));
            Contacts.Insert(0, new ShellResourceViewModel(
                "jiguli",
                WorkspaceResourceKind.Contact,
                Localization.Text("叽咕狸", "Jiguli"),
                Localization.Text("和叽咕狸开始对话", "Start a conversation with Jiguli"),
                "\uE77B",
                conversation?.Id));
        }

        Projects.Clear();
        foreach (var project in snapshot.Projects)
        {
            var conversationId = project.LatestConversationId
                ?? activeConversations.FirstOrDefault(value =>
                    string.Equals(value.ProjectId, project.Id, StringComparison.Ordinal))?.Id;
            Projects.Add(new ShellResourceViewModel(
                project.Id,
                WorkspaceResourceKind.Project,
                project.Name,
                project.DisplayRootPath ?? project.RootPath ?? Localization.Projects,
                "\uE8B7",
                conversationId));
        }

        await RefreshLocalConnectorAsync(cancellationToken);
        await RemoteConnections.OpenAsync(cancellationToken);
        RebuildRemoteResources();

        if (SelectedResource is not null)
        {
            SelectedResource = Contacts.Concat(Projects).Concat(LocalResources).Concat(RemoteResources)
                .FirstOrDefault(value =>
                    value.Kind == SelectedResource.Kind && value.Id == SelectedResource.Id);
        }
    }

    private async Task ActivateResourceAsync(
        ShellResourceViewModel? resource,
        CancellationToken cancellationToken)
    {
        if (resource is null)
        {
            await ProjectRun.CloseAsync(cancellationToken);
            await ProjectGit.CloseAsync(cancellationToken);
            await Conversation.OpenAsync(null, "ChatOS", cancellationToken);
            return;
        }

        if (resource.Kind == WorkspaceResourceKind.LocalConnector)
        {
            return;
        }
        if (resource.Kind == WorkspaceResourceKind.LocalTerminal)
        {
            return;
        }
        if (resource.Kind == WorkspaceResourceKind.RemoteConnection)
        {
            RemoteConnections.EditCommand.Execute(
                RemoteConnections.Connections.FirstOrDefault(value => value.Id == resource.Id));
            return;
        }

        if (resource.Kind != WorkspaceResourceKind.Project)
        {
            await ProjectRun.CloseAsync(cancellationToken);
            await ProjectGit.CloseAsync(cancellationToken);
            await Conversation.OpenAsync(resource.ConversationId, resource.Title, cancellationToken);
            return;
        }

        var project = _workspaceSnapshot.Projects.FirstOrDefault(value =>
            string.Equals(value.Id, resource.Id, StringComparison.Ordinal));
        if (project is null)
        {
            ErrorMessage = Localization.Text(
                "所选项目已不在当前工作区中，请刷新后重试。",
                "The selected project is no longer in the current workspace. Refresh and try again.");
            await Conversation.OpenAsync(null, resource.Title, cancellationToken);
            return;
        }

        var filesTask = ProjectFiles.OpenAsync(project, cancellationToken);
        var gitTask = ProjectGit.OpenAsync(project, cancellationToken);
        var planTask = ProjectPlan.OpenAsync(project, cancellationToken);
        var runTask = ProjectRun.OpenAsync(project, cancellationToken);
        if (!string.IsNullOrWhiteSpace(resource.ConversationId))
        {
            await Task.WhenAll(
                filesTask,
                gitTask,
                planTask,
                runTask,
                Conversation.OpenAsync(resource.ConversationId, resource.Title, cancellationToken));
            return;
        }

        var contact = _workspaceSnapshot.Contacts.FirstOrDefault(value =>
                string.Equals(value.AgentId, "jiguli", StringComparison.OrdinalIgnoreCase))
            ?? _workspaceSnapshot.Contacts.FirstOrDefault();
        if (contact is null)
        {
            ErrorMessage = Localization.Text(
                "没有找到可用于项目会话的联系人，请刷新工作区。",
                "No contact is available for the project conversation. Refresh the workspace.");
            await Conversation.OpenAsync(null, resource.Title, cancellationToken);
            await Task.WhenAll(filesTask, gitTask, planTask, runTask);
            return;
        }

        IsPreparingConversation = true;
        ErrorMessage = null;
        await Conversation.OpenAsync(null, resource.Title, cancellationToken);
        try
        {
            var conversationTask = _resourceCreationService.EnsureConversationAsync(
                project,
                contact,
                cancellationToken);
            await Task.WhenAll(filesTask, gitTask, planTask, runTask, conversationTask);
            var conversationId = conversationTask.Result;
            cancellationToken.ThrowIfCancellationRequested();
            if (SelectedResource is not { } selected ||
                selected.Kind != resource.Kind ||
                !string.Equals(selected.Id, resource.Id, StringComparison.Ordinal))
            {
                return;
            }

            var updated = resource with { ConversationId = conversationId };
            var index = Projects.IndexOf(resource);
            if (index >= 0)
            {
                Projects[index] = updated;
            }

            SetSelectedResourceWithoutActivation(updated);
            await Conversation.OpenAsync(conversationId, resource.Title, cancellationToken);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            if (!cancellationToken.IsCancellationRequested)
            {
                ErrorMessage = Localization.Text(
                    $"准备项目会话失败：{exception.Message}",
                    $"Unable to prepare the project conversation: {exception.Message}");
            }
        }
        finally
        {
            if (!cancellationToken.IsCancellationRequested)
            {
                IsPreparingConversation = false;
            }
        }
    }

    private async Task ActivateResourceSafelyAsync(
        ShellResourceViewModel? resource,
        CancellationToken cancellationToken)
    {
        try
        {
            await ActivateResourceAsync(resource, cancellationToken);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
        }
        catch (Exception exception)
        {
            if (!cancellationToken.IsCancellationRequested)
            {
                ErrorMessage = exception.Message;
            }
        }
    }

    private void CancelSelectionActivation()
    {
        _selectionCancellation?.Cancel();
        _selectionCancellation?.Dispose();
        _selectionCancellation = null;
        IsPreparingConversation = false;
    }

    private string ContactSubtitle(string? status) => status?.ToLowerInvariant() switch
    {
        "active" or "online" => Localization.Text("在线", "Online"),
        "busy" or "working" => Localization.Text("处理中", "Working"),
        "offline" => Localization.Text("离线", "Offline"),
        _ => Localization.Text("开始对话", "Start conversation"),
    };

    public async Task RefreshLocalConnectorAsync(CancellationToken cancellationToken = default)
    {
        try
        {
            ApplyLocalConnectorStatus(await _localConnectorControl.GetStatusAsync(cancellationToken));
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            var resource = new ShellResourceViewModel(
                "local-connector",
                WorkspaceResourceKind.LocalConnector,
                Localization.Text("本机 Connector", "Local Connector"),
                Localization.Text(
                    $"状态读取失败：{exception.Message}",
                    $"Unable to read status: {exception.Message}"),
                "\uE968");
            var connectorIndex = LocalResources.ToList().FindIndex(value =>
                value.Kind == WorkspaceResourceKind.LocalConnector);
            if (connectorIndex < 0)
            {
                LocalResources.Insert(0, resource);
            }
            else
            {
                LocalResources[connectorIndex] = resource;
            }
        }
    }

    public void ApplyLocalConnectorStatus(LocalConnectorStatus status)
    {
        LocalConnectorStatus = status;
        var subtitle = status.ConnectionPhase switch
        {
            "Connected" => Localization.Text(
                $"已连接 · {status.Workspaces.Count} 个工作区",
                $"Connected · {status.Workspaces.Count} workspace{(status.Workspaces.Count == 1 ? string.Empty : "s")}"),
            "Connecting" => Localization.Text("正在连接", "Connecting"),
            "WaitingToReconnect" => Localization.Text("连接中断，等待重连", "Connection interrupted; waiting to reconnect"),
            "Suspended" => Localization.Text("系统睡眠，连接已暂停", "Connection paused while the system sleeps"),
            _ when status.IsPaired => Localization.Text(
                $"已配对 · {status.Workspaces.Count} 个工作区",
                $"Paired · {status.Workspaces.Count} workspace{(status.Workspaces.Count == 1 ? string.Empty : "s")}"),
            _ => Localization.Text("尚未配对，点击设置", "Not paired; open Settings"),
        };
        var resource = new ShellResourceViewModel(
            "local-connector",
            WorkspaceResourceKind.LocalConnector,
            status.DeviceName ?? Localization.Text("本机 Connector", "Local Connector"),
            subtitle,
            "\uE968");
        var connectorIndex = LocalResources.ToList().FindIndex(value =>
            value.Kind == WorkspaceResourceKind.LocalConnector);
        if (connectorIndex < 0)
        {
            LocalResources.Insert(0, resource);
        }
        else
        {
            LocalResources[connectorIndex] = resource;
        }

        if (SelectedResource?.Kind == WorkspaceResourceKind.LocalConnector)
        {
            SetSelectedResourceWithoutActivation(resource);
        }
    }

    private void RebuildRemoteResources()
    {
        var selectedId = SelectedResource?.Kind == WorkspaceResourceKind.RemoteConnection
            ? SelectedResource.Id
            : null;
        RemoteResources.Clear();
        foreach (var connection in RemoteConnections.Connections)
        {
            RemoteResources.Add(new ShellResourceViewModel(
                connection.Id,
                WorkspaceResourceKind.RemoteConnection,
                connection.Name,
                $"{connection.Username}@{connection.Host}:{connection.Port}",
                "\uE968"));
        }
        if (selectedId is not null)
        {
            SetSelectedResourceWithoutActivation(
                RemoteResources.FirstOrDefault(value => value.Id == selectedId));
        }
    }

    private void SetSelectedResourceWithoutActivation(ShellResourceViewModel? resource)
    {
        _suppressSelectionActivation = true;
        try
        {
            SelectedResource = resource;
        }
        finally
        {
            _suppressSelectionActivation = false;
        }
    }

    private void RelocalizeResources()
    {
        for (var index = 0; index < Contacts.Count; index++)
        {
            var current = Contacts[index];
            var contact = _workspaceSnapshot.Contacts.FirstOrDefault(value => value.Id == current.Id);
            Contacts[index] = contact is null
                ? current with
                {
                    Title = Localization.Text("叽咕狸", "Jiguli"),
                    Subtitle = Localization.Text("和叽咕狸开始对话", "Start a conversation with Jiguli"),
                }
                : current with { Subtitle = ContactSubtitle(contact.Status) };
        }

        for (var index = 0; index < Projects.Count; index++)
        {
            var current = Projects[index];
            var project = _workspaceSnapshot.Projects.FirstOrDefault(value => value.Id == current.Id);
            if (project is not null && string.IsNullOrWhiteSpace(project.DisplayRootPath ?? project.RootPath))
            {
                Projects[index] = current with { Subtitle = Localization.Projects };
            }
        }

        for (var index = 0; index < LocalResources.Count; index++)
        {
            var current = LocalResources[index];
            if (current.Kind != WorkspaceResourceKind.LocalTerminal) continue;
            var workspace = LocalConnectorStatus?.Workspaces.FirstOrDefault(value => value.Id == current.WorkspaceId);
            var alias = workspace?.Alias ?? current.Title.Split('·', 2)[0].Trim();
            LocalResources[index] = current with { Title = $"{alias} · {Localization.Terminal}" };
        }

        if (LocalConnectorStatus is not null)
        {
            ApplyLocalConnectorStatus(LocalConnectorStatus);
        }

        if (SelectedResource is { } selected)
        {
            SetSelectedResourceWithoutActivation(Contacts.Concat(Projects).Concat(LocalResources).Concat(RemoteResources)
                .FirstOrDefault(value => value.Kind == selected.Kind && value.Id == selected.Id));
        }
    }
}
