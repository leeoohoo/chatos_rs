using System.Collections.ObjectModel;
using System.Net;
using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.AgentTeams;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Remote;
using ChatOS.Connector.Remote;
using ChatOS.Connector.Terminal;
using ChatOS.Connector.LocalAgent;
using ChatOS.Connector.Gateway;
using ChatOS.Api.Http;

namespace ChatOS.Desktop.AppShell;

public sealed partial class MainWindowViewModel : ObservableObject
{
    private const string BuiltInLocalTerminalId = "terminal-local";
    private readonly IAuthenticationService _authenticationService;
    private readonly IWorkspaceRelationsService _workspaceRelations;
    private readonly IProjectRegistry _projectRegistry;
    private readonly ILocalProjectsService _localProjects;
    private readonly ILocalConnectorPairingTicketService? _localConnectorPairingTickets;
    private string? _ownerUserId;
    private long _accountGeneration;
    private long _refreshGeneration;
    private CancellationTokenSource _accountCancellation = new();
    private readonly IProjectConversationService _projectConversations;
    private readonly Dictionary<string, string> _verifiedProjectConversations =
        new(StringComparer.Ordinal);
    private readonly ILocalConnectorControlService _localConnectorControl;
    private WorkspaceSnapshot _workspaceSnapshot = WorkspaceSnapshot.Empty;
    private CancellationTokenSource? _selectionCancellation;
    private readonly SemaphoreSlim _conversationPreparationGate = new(1, 1);
    private bool _suppressSelectionActivation;
    private readonly TerminalSessionManager? _terminalSessions;
    private readonly RemoteTerminalSessionManager? _remoteTerminalSessions;
    private readonly ILocalAgentHostLifecycle? _localAgentHost;
    private readonly WindowsLocalAgentBootstrapService? _localAgentBootstrap;
    private readonly PetFavoriteProjectsManager? _petFavorites;

    public MainWindowViewModel(
        IAuthenticationService authenticationService,
        IWorkspaceRelationsService workspaceRelations,
        IProjectRegistry projectRegistry,
        ILocalProjectsService localProjects,
        IProjectConversationService projectConversations,
        ILocalConnectorControlService localConnectorControl,
        ConversationSessionViewModel conversation,
        ProjectFilesViewModel projectFiles,
        ProjectGitViewModel projectGit,
        ProjectRunViewModel projectRun,
        RemoteConnectionsViewModel remoteConnections,
        LocalizationViewModel localization,
        AgentTeamWorkspaceViewModel? agentTeam = null,
        ProjectRequirementSurveysViewModel? requirementSurveys = null,
        TerminalSessionManager? terminalSessions = null,
        RemoteTerminalSessionManager? remoteTerminalSessions = null,
        ILocalAgentHostLifecycle? localAgentHost = null,
        WindowsLocalAgentBootstrapService? localAgentBootstrap = null,
        PetFavoriteProjectsManager? petFavorites = null,
        ILocalConnectorPairingTicketService? localConnectorPairingTickets = null)
    {
        _authenticationService = authenticationService;
        _workspaceRelations = workspaceRelations;
        _projectRegistry = projectRegistry;
        _localProjects = localProjects;
        _projectConversations = projectConversations;
        _localConnectorControl = localConnectorControl;
        _localConnectorPairingTickets = localConnectorPairingTickets;
        Conversation = conversation;
        ProjectFiles = projectFiles;
        ProjectGit = projectGit;
        ProjectRun = projectRun;
        AgentTeam = agentTeam;
        RequirementSurveys = requirementSurveys;
        RemoteConnections = remoteConnections;
        Localization = localization;
        _terminalSessions = terminalSessions;
        _remoteTerminalSessions = remoteTerminalSessions;
        _localAgentHost = localAgentHost;
        _localAgentBootstrap = localAgentBootstrap;
        _petFavorites = petFavorites;
        RemoteConnections.Connections.CollectionChanged += (_, _) => RebuildRemoteResources();
        Localization.PropertyChanged += (_, _) => RelocalizeResources();
        AddWorkspaceTools();
    }

    public ConversationSessionViewModel Conversation { get; }

    public ProjectFilesViewModel ProjectFiles { get; }

    public ProjectGitViewModel ProjectGit { get; }

    public ProjectRunViewModel ProjectRun { get; }

    public AgentTeamWorkspaceViewModel? AgentTeam { get; }

    public ProjectRequirementSurveysViewModel? RequirementSurveys { get; }

    public RemoteConnectionsViewModel RemoteConnections { get; }

    public LocalizationViewModel Localization { get; }

    public ObservableCollection<ShellResourceViewModel> Contacts { get; } = [];

    public ObservableCollection<ShellResourceViewModel> Projects { get; } = [];

    public ObservableCollection<ShellResourceViewModel> ApplicationResources { get; } = [];

    public ObservableCollection<ShellResourceViewModel> LocalResources { get; } = [];

    public ObservableCollection<ShellResourceViewModel> RemoteResources { get; } = [];

    public LocalConnectorStatus? LocalConnectorStatus { get; private set; }

    // Keep the sign-in affordance in sync with the macOS form: the request is
    // not sent until both credentials have usable values.  This also prevents
    // a blank-submit server round trip when PasswordBox has just been cleared.
    public bool CanLogin => !IsBusy &&
        !string.IsNullOrWhiteSpace(Username) &&
        Password.Length > 0;

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

    private ShellResourceViewModel CreateBuiltInLocalTerminalResource(
        LocalConnectorStatus? status,
        string? statusMessage = null)
    {
        var workspace = status?.Workspaces.FirstOrDefault();
        var root = workspace?.AbsoluteRoot;
        if (string.IsNullOrWhiteSpace(root) || !Path.IsPathFullyQualified(root))
        {
            root = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
        }

        return new ShellResourceViewModel(
            BuiltInLocalTerminalId,
            WorkspaceResourceKind.LocalTerminal,
            Localization.Text("本机终端", "Local Terminal"),
            statusMessage ?? Localization.Text("可用", "Available"),
            "\uE756",
            WorkspaceId: workspace?.Id ?? BuiltInLocalTerminalId,
            AbsoluteRoot: root);
    }

    public async Task<string> EnsureProjectConversationAsync(
        string projectId,
        CancellationToken cancellationToken = default)
    {
        var generation = AccountGeneration;
        var owner = RequireAccount(generation);
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, _accountCancellation.Token);
        cancellationToken = linked.Token;
        await _conversationPreparationGate.WaitAsync(cancellationToken);
        try
        {
            RequireAccount(generation);
            var localProject = await _projectRegistry.GetAsync(owner, projectId, cancellationToken);
            RequireAccount(generation);
            if (localProject?.Status != LocalProjectStatus.Active)
                throw new InvalidOperationException("The local project is unavailable.");
            if (_verifiedProjectConversations.TryGetValue(projectId, out var existing))
            {
                ApplyPreparedProjectConversation(projectId, existing);
                return existing;
            }
            var project = _workspaceSnapshot.Projects.FirstOrDefault(value =>
                string.Equals(value.Id, projectId, StringComparison.Ordinal))
                ?? throw new KeyNotFoundException(Localization.Text(
                    "所选项目已不在当前工作区中。",
                    "The selected project is no longer in the current workspace."));
            project = project with
            {
                ProjectContext = await _localProjects.ResolveContextAsync(owner, projectId, cancellationToken),
            };
            var contact = _workspaceSnapshot.Contacts.FirstOrDefault(value =>
                    string.Equals(value.AgentId, "jiguli", StringComparison.OrdinalIgnoreCase))
                ?? _workspaceSnapshot.Contacts.FirstOrDefault()
                ?? throw new InvalidOperationException(Localization.Text(
                    "没有找到可用于项目会话的联系人。",
                    "No contact is available for the project conversation."));
            var conversationId = await _projectConversations.EnsureConversationAsync(
                project,
                contact,
                cancellationToken);
            if (string.IsNullOrWhiteSpace(conversationId))
                throw new InvalidOperationException(Localization.Text(
                    "本地智能体返回了无效的项目会话标识。",
                    "The Local Agent returned an invalid project conversation identity."));
            RequireAccount(generation);
            var current = await _projectRegistry.GetAsync(owner, projectId, cancellationToken);
            RequireAccount(generation);
            if (current?.Status != LocalProjectStatus.Active)
                throw new InvalidOperationException("The local project was removed while preparing the conversation.");
            _verifiedProjectConversations[projectId] = conversationId;
            ApplyPreparedProjectConversation(projectId, conversationId);
            return conversationId;
        }
        finally
        {
            _conversationPreparationGate.Release();
        }
    }

    public long AccountGeneration => _accountGeneration;
    public string? CurrentOwnerUserId => IsAuthenticated ? _ownerUserId : null;
    public bool IsPublishingWorkspace { get; private set; }

    private string RequireAccount(long generation)
    {
        if (generation != AccountGeneration || !IsAuthenticated || _ownerUserId is null)
            throw new OperationCanceledException("The signed-in account changed.");
        return _ownerUserId;
    }

    private bool IsCurrent(long account, long refresh) =>
        IsAuthenticated && account == AccountGeneration && refresh == _refreshGeneration;

    public async Task<LocalProjectRecord> GetLocalProjectAsync(long account, string projectId)
    {
        var owner = RequireAccount(account);
        var project = await _projectRegistry.GetAsync(owner, projectId);
        RequireAccount(account);
        return project is { Status: LocalProjectStatus.Active } ? project
            : throw new InvalidOperationException("The local project is unavailable.");
    }

    public async Task CreateProjectAsync(long account, LocalProjectDraft draft, string expectedWorkspaceRoot)
    {
        var owner = RequireAccount(account);
        var project = await _localProjects.CreateAsync(owner, draft, expectedWorkspaceRoot, _accountCancellation.Token);
        RequireAccount(account);
        await ReloadLocalProjectsAsync(account);
        RequireAccount(account);
        SelectedResource = Projects.FirstOrDefault(value => value.Id == project.Id);
    }

    public async Task RenameProjectAsync(long account, LocalProjectRecord project, string name)
    {
        var owner = RequireAccount(account);
        if (project.OwnerUserId != owner) throw new InvalidOperationException("Project account mismatch.");
        await _localProjects.RenameAsync(owner, project.Id, project.Revision, name, _accountCancellation.Token);
        RequireAccount(account);
        await ReloadLocalProjectsAsync(account);
    }

    public async Task RemoveProjectAsync(long account, LocalProjectRecord project)
    {
        var owner = RequireAccount(account);
        if (project.OwnerUserId != owner) throw new InvalidOperationException("Project account mismatch.");
        await _localProjects.RemoveAsync(owner, project.Id, project.Revision, _accountCancellation.Token);
        RequireAccount(account);
        _verifiedProjectConversations.Remove(project.Id);
        if (_petFavorites is not null)
        {
            try
            {
                await _petFavorites.SetFavoriteAsync(project.Id, false, _accountCancellation.Token);
            }
            catch (OperationCanceledException) when (_accountCancellation.IsCancellationRequested)
            {
                throw;
            }
            catch
            {
                // The registry removal is authoritative. A stale favorite is harmless and can be
                // reconciled on the next favorite-project refresh; do not report a false deletion failure.
            }
        }
        await ReloadLocalProjectsAsync(account);
    }

    private async Task ReloadLocalProjectsAsync(long account)
    {
        var cancellationToken = _accountCancellation.Token;
        var owner = RequireAccount(account);
        var refresh = ++_refreshGeneration;
        var deviceId = await _localProjects.GetDeviceIdAsync(owner, cancellationToken);
        RequireAccount(account);
        var snapshot = await new ClientOwnedWorkspaceLoader(_projectRegistry, _workspaceRelations, owner)
            .LoadLocalAsync(deviceId, cancellationToken);
        if (!IsCurrent(account, refresh)) return;
        PublishWorkspace(snapshot with { Contacts = _workspaceSnapshot.Contacts, Conversations = _workspaceSnapshot.Conversations });
    }

    [ObservableProperty]
    private ShellResourceViewModel? _selectedResource;

    [ObservableProperty]
    private string _workspaceTitle = "ChatOS";

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanLogin))]
    private string _username = string.Empty;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanLogin))]
    private string _password = string.Empty;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private string _currentUserLabel = string.Empty;

    [ObservableProperty]
    private bool _isAuthenticated;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CanLogin))]
    private bool _isBusy;

    [ObservableProperty]
    private bool _isInitialized;

    [ObservableProperty]
    private bool _isPreparingConversation;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(HasProjectConversationError))]
    private string? _projectConversationErrorMessage;

    public bool HasProjectConversationError =>
        !string.IsNullOrWhiteSpace(ProjectConversationErrorMessage);

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
            var authenticationGeneration = AccountGeneration;
            var session = await _authenticationService.RestoreSessionAsync(cancellationToken);
            if (authenticationGeneration != AccountGeneration) return;
            if (session is not null)
            {
                ApplySession(session);
                await EnsureLocalConnectorReadyAsync(cancellationToken);
                await PrepareLocalAgentServicesAsync(session.User.Id, cancellationToken);
                await ReloadWorkspaceCoreAsync(cancellationToken);
                await StartLocalAgentWithoutBlockingWorkspaceAsync(session.User.Id, cancellationToken);
            }
        }
        catch (OperationCanceledException) { }
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

        if (string.IsNullOrWhiteSpace(Username) || Password.Length == 0)
        {
            ErrorMessage = Localization.Text(
                "请输入账号和密码。",
                "Enter your account and password.");
            return;
        }

        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var authenticationGeneration = AccountGeneration;
            var session = await _authenticationService.LoginAsync(Username, Password);
            if (authenticationGeneration != AccountGeneration) return;
            Password = string.Empty;
            ApplySession(session);
            await EnsureLocalConnectorReadyAsync(CancellationToken.None);
            await PrepareLocalAgentServicesAsync(session.User.Id, CancellationToken.None);
            await ReloadWorkspaceCoreAsync();
            await StartLocalAgentWithoutBlockingWorkspaceAsync(session.User.Id, CancellationToken.None);
        }
        catch (OperationCanceledException) { }
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
            if (_ownerUserId is { } ownerUserId)
            {
                await StartLocalAgentWithoutBlockingWorkspaceAsync(
                    ownerUserId,
                    CancellationToken.None);
            }
        }
        catch (OperationCanceledException) { }
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
        _accountCancellation.Cancel();
        _accountCancellation.Dispose();
        _accountCancellation = new();
        _accountGeneration++;
        _refreshGeneration++;
        _ownerUserId = null;
        _verifiedProjectConversations.Clear();
        _workspaceSnapshot = WorkspaceSnapshot.Empty;
        LocalConnectorStatus = null;
        IsRegistrationMode = false;
        ClearRegistrationFeedback(clearIdentity: true);
        IsAuthenticated = false;
        Contacts.Clear();
        Projects.Clear();
        LocalResources.Clear();
        RemoteResources.Clear();
        SelectedResource = null;
        if (_terminalSessions is not null)
        {
            try
            {
                await _terminalSessions.CloseAllAsync(CancellationToken.None);
            }
            catch
            {
            }
        }
        if (_remoteTerminalSessions is not null)
        {
            try
            {
                await _remoteTerminalSessions.CloseAllAsync(CancellationToken.None);
            }
            catch
            {
            }
        }
        if (_localAgentHost is not null)
        {
            await _localAgentHost.StopAsync(CancellationToken.None);
        }
        _localAgentBootstrap?.Reset();
        await _authenticationService.LogoutAsync();
        await ProjectRun.CloseAsync();
        await ProjectGit.CloseAsync();
        await ProjectFiles.CloseAsync();
        await Conversation.OpenAsync(null, "ChatOS");
        CurrentUserLabel = string.Empty;
        IsAuthenticated = false;
        ErrorMessage = null;
    }

    partial void OnSelectedResourceChanged(ShellResourceViewModel? value)
    {
        WorkspaceTitle = value?.Title ?? "ChatOS";
        if (_suppressSelectionActivation || IsPublishingWorkspace) return;
        CancelSelectionActivation();
        _selectionCancellation = new CancellationTokenSource();
        _ = ActivateResourceSafelyAsync(value, _selectionCancellation.Token);
    }

    private void ApplySession(AuthSession session)
    {
        if (_ownerUserId != session.User.Id)
        {
            CancelSelectionActivation();
            _accountCancellation.Cancel();
            _accountCancellation.Dispose();
            _accountCancellation = new();
            _accountGeneration++;
            _refreshGeneration++;
            _workspaceSnapshot = WorkspaceSnapshot.Empty;
            _verifiedProjectConversations.Clear();
            Contacts.Clear();
            Projects.Clear();
            LocalResources.Clear();
            RemoteResources.Clear();
            LocalConnectorStatus = null;
            SelectedResource = null;
        }
        _ownerUserId = session.User.Id;
        CurrentUserLabel = session.User.EffectiveDisplayName;
        IsAuthenticated = true;
    }

    private async Task StartLocalAgentAsync(
        string ownerUserId,
        CancellationToken cancellationToken)
    {
        using var linkedCancellation = CancellationTokenSource.CreateLinkedTokenSource(
            cancellationToken, _accountCancellation.Token);
        cancellationToken = linkedCancellation.Token;
        if (_localAgentBootstrap is not null)
        {
            _ = await _localAgentBootstrap
                .BootstrapForOwnerAsync(ownerUserId, cancellationToken);
            return;
        }
        if (_localAgentHost is not null)
        {
            await _localAgentHost.StartForOwnerAsync(ownerUserId, cancellationToken);
        }
    }

    private async Task PrepareLocalAgentServicesAsync(
        string ownerUserId,
        CancellationToken cancellationToken)
    {
        try
        {
            if (_localAgentBootstrap is not null)
            {
                await _localAgentBootstrap.PrepareLocalServicesForOwnerAsync(
                    ownerUserId,
                    cancellationToken);
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception exception)
        {
            // macOS authenticates first and treats native-agent startup as an optional
            // capability. A missing Connector/host must not prevent the authenticated
            // shell and workspace from loading.
            if (LocalConnectorStatus?.IsPaired == true)
            {
                ErrorMessage = LocalAgentFailureMessage(exception);
            }
        }
    }

    public async Task HandleAuthenticationExpiredAsync()
    {
        // Session expiry must win over any refresh/send busy state. macOS transitions to
        // signed-out immediately even when a workspace operation is still in flight.
        if (!IsAuthenticated)
        {
            return;
        }

        try
        {
            await LogoutAsync();
            ErrorMessage = Localization.Text(
                "登录状态已失效，请重新登录。",
                "Your sign-in session has expired. Please sign in again.");
        }
        catch (Exception exception)
        {
            ErrorMessage = exception.Message;
            IsAuthenticated = false;
        }
    }

    private async Task EnsureLocalConnectorReadyAsync(CancellationToken cancellationToken)
    {
        try
        {
            var status = await _localConnectorControl.GetStatusAsync(cancellationToken);
            if (!status.IsPaired && _localConnectorPairingTickets is not null)
            {
                var userProfile = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile);
                var root = Path.GetPathRoot(userProfile);
                if (string.IsNullOrWhiteSpace(root) || !Directory.Exists(root))
                {
                    ApplyLocalConnectorStatus(status);
                    return;
                }

                var gateway = Environment.GetEnvironmentVariable("CHATOS_LOCAL_CONNECTOR_CLOUD_BASE_URL")
                    ?? "https://local-connector.jgoool.com";
                var ticket = await _localConnectorPairingTickets.IssueAsync(cancellationToken);
                status = await _localConnectorControl.PairAsync(
                    new LocalConnectorPairingDraft(
                        gateway,
                        Environment.MachineName,
                        [new LocalConnectorWorkspaceDraft(root, Localization.Text("本机文件系统", "Local file system"))]),
                    ticket,
                    cancellationToken);
            }

            ApplyLocalConnectorStatus(status);
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch
        {
            // Connector bootstrap is best effort. The shell and locally registered projects
            // still load, while the create actions present an explicit setup path to the user.
        }
    }

    private async Task StartLocalAgentWithoutBlockingWorkspaceAsync(
        string ownerUserId,
        CancellationToken cancellationToken)
    {
        try
        {
            try
            {
                await StartLocalAgentAsync(ownerUserId, cancellationToken);
            }
            catch (ConnectorGatewayException exception)
                when (exception.StatusCode == HttpStatusCode.Unauthorized)
            {
                // The Connector credential has a separate lifetime from the ChatOS session.
                // Renew it with the current signed-in session and retry bootstrap once, matching
                // the macOS re-pairing path without turning a Connector failure into a logout.
                if (!await TryRefreshLocalConnectorCredentialAsync(cancellationToken))
                {
                    throw;
                }

                await StartLocalAgentAsync(ownerUserId, cancellationToken);
            }
            if (_localAgentBootstrap is not null)
            {
                // Match macOS: a conversation may be restored before the optional Local
                // Agent control plane finishes bootstrapping. Reload only its runtime
                // settings after configuration succeeds, preserving the visible history
                // and any composer draft.
                await Conversation.LocalAgentRuntimeDidBecomeReadyAsync(cancellationToken);
            }
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (OperationCanceledException)
        {
            // The account-scoped startup was superseded. The active account owns the next start.
        }
        catch (Exception exception)
        {
            // Local Agent setup is an optional authenticated capability. Missing Connector or
            // model configuration must never hide projects or break the basic desktop shell.
            if (LocalConnectorStatus?.IsPaired == true)
            {
                ErrorMessage = LocalAgentFailureMessage(exception);
            }
        }
    }

    private async Task<bool> TryRefreshLocalConnectorCredentialAsync(
        CancellationToken cancellationToken)
    {
        if (_localConnectorPairingTickets is null || LocalConnectorStatus?.IsPaired != true)
        {
            return false;
        }

        var ticket = await _localConnectorPairingTickets.IssueAsync(cancellationToken);
        var status = await _localConnectorControl.RefreshCredentialAsync(
            ticket,
            cancellationToken);
        ApplyLocalConnectorStatus(status);
        return true;
    }

    private string LocalAgentFailureMessage(Exception exception)
    {
        if (exception is ConnectorGatewayException { StatusCode: HttpStatusCode.Unauthorized })
        {
            return Localization.Text(
                "本地智能体连接凭据已失效，请刷新资源后重试。",
                "The Local Agent connection credential has expired. Refresh resources to try again.");
        }

        var detail = ReadableErrorDetail(exception.Message);
        return Localization.Text(
            $"本地智能体暂时不可用，请刷新资源后重试。{(detail is null ? string.Empty : $"（{detail}）")}",
            $"Local Agent is temporarily unavailable. Refresh resources to try again.{(detail is null ? string.Empty : $" ({detail})")}");
    }

    private static string? ReadableErrorDetail(string? message)
    {
        if (string.IsNullOrWhiteSpace(message))
        {
            return null;
        }

        var detail = message.Trim();
        if (detail.StartsWith('{'))
        {
            try
            {
                using var document = JsonDocument.Parse(detail);
                foreach (var key in new[] { "message", "detail", "error" })
                {
                    if (document.RootElement.TryGetProperty(key, out var value) &&
                        value.ValueKind == JsonValueKind.String &&
                        !string.IsNullOrWhiteSpace(value.GetString()))
                    {
                        detail = value.GetString()!.Trim();
                        break;
                    }
                }
            }
            catch (JsonException)
            {
                return null;
            }
        }

        if (detail.Length > 240)
        {
            detail = detail[..240] + "…";
        }

        return string.IsNullOrEmpty(detail) ? null : detail;
    }

    private async Task ReloadWorkspaceCoreAsync(CancellationToken cancellationToken = default)
    {
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken, _accountCancellation.Token);
        cancellationToken = linked.Token;
        var account = AccountGeneration;
        var owner = RequireAccount(account);
        var refresh = ++_refreshGeneration;
        var deviceId = await _localProjects.GetDeviceIdAsync(owner, cancellationToken);
        if (!IsCurrent(account, refresh)) return;
        var loader = new ClientOwnedWorkspaceLoader(_projectRegistry, _workspaceRelations, owner);
        var local = await loader.LoadLocalAsync(deviceId, cancellationToken);
        if (!IsCurrent(account, refresh)) return;
        PublishWorkspace(local with { Contacts = _workspaceSnapshot.Contacts, Conversations = _workspaceSnapshot.Conversations });
        var result = await loader.RefreshAsync(deviceId, cancellationToken);
        if (!IsCurrent(account, refresh)) return;
        var snapshot = result.RemoteError is null ? result.Snapshot : result.Snapshot with
        {
            Contacts = _workspaceSnapshot.Contacts,
            Conversations = _workspaceSnapshot.Conversations,
        };
        PublishWorkspace(snapshot);
        ErrorMessage = result.RemoteError;
        await RefreshLocalConnectorAsync(cancellationToken);
        if (!IsCurrent(account, refresh)) return;
        await RemoteConnections.OpenAsync(cancellationToken);
        if (!IsCurrent(account, refresh)) return;
        RebuildRemoteResources();
    }

    private void PublishWorkspace(WorkspaceSnapshot snapshot)
    {
        var previous = SelectedResource;
        IsPublishingWorkspace = true;
        try
        {
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
                    conversation?.Title ?? ContactSubtitle(contact.Status),
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
                    conversation?.Title ?? Localization.Text("和叽咕狸开始对话", "Start a conversation with Jiguli"),
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

            SelectedResource = previous is null ? null :
                Contacts.Concat(Projects).Concat(ApplicationResources).Concat(LocalResources).Concat(RemoteResources)
                    .FirstOrDefault(value => value.Kind == previous.Kind && value.Id == previous.Id);
        }
        finally
        {
            IsPublishingWorkspace = false;
        }
        OnPropertyChanged(nameof(SelectedResource));
        if (previous?.Kind != SelectedResource?.Kind ||
            previous?.Id != SelectedResource?.Id ||
            previous?.ConversationId != SelectedResource?.ConversationId)
        {
            CancelSelectionActivation();
            _selectionCancellation = new CancellationTokenSource();
            _ = ActivateResourceSafelyAsync(SelectedResource, _selectionCancellation.Token);
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
            await ProjectFiles.CloseAsync(cancellationToken);
            await Conversation.OpenAsync(null, "ChatOS", cancellationToken);
            return;
        }

        if (resource.Kind == WorkspaceResourceKind.LocalConnector)
        {
            return;
        }
        if (resource.Kind == WorkspaceResourceKind.Applications)
        {
            return;
        }
        if (resource.Kind == WorkspaceResourceKind.AgentTeams)
        {
            var owner = RequireAccount(AccountGeneration);
            if (AgentTeam is not null)
                await AgentTeam.OpenGlobalAsync(owner, cancellationToken);
            return;
        }
        if (resource.Kind is WorkspaceResourceKind.RequirementSurveys or
            WorkspaceResourceKind.MediaStudio)
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
            await ProjectFiles.CloseAsync(cancellationToken);
            await Conversation.OpenAsync(
                resource.ConversationId,
                resource.Title,
                string.Equals(resource.Id, "plugin-installer", StringComparison.Ordinal),
                cancellationToken);
            return;
        }

        var project = _workspaceSnapshot.Projects.FirstOrDefault(value =>
            string.Equals(value.Id, resource.Id, StringComparison.Ordinal));
        if (project is null)
        {
            ErrorMessage = Localization.Text(
                "所选项目已不在当前工作区中，请刷新后重试。",
                "The selected project is no longer in the current workspace. Refresh and try again.");
            await ProjectFiles.CloseAsync(cancellationToken);
            await Conversation.OpenAsync(null, resource.Title, cancellationToken);
            return;
        }

        await ProjectRun.CloseAsync(cancellationToken);
        await ProjectGit.CloseAsync(cancellationToken);
        await Conversation.OpenAsync(null, resource.Title, cancellationToken);
        await ProjectFiles.OpenAsync(project, cancellationToken);
    }

    public async Task OpenProjectTabAsync(string tab)
    {
        var resource = SelectedResource;
        if (resource?.Kind != WorkspaceResourceKind.Project) return;
        var project = _workspaceSnapshot.Projects.FirstOrDefault(value => value.Id == resource.Id);
        if (project is null) return;
        var token = _selectionCancellation?.Token ?? CancellationToken.None;
        if (!string.Equals(tab, "chat", StringComparison.Ordinal))
        {
            ProjectConversationErrorMessage = null;
        }
        try
        {
            switch (tab)
            {
                case "files":
                    await ProjectRun.CloseAsync(token);
                    await ProjectGit.CloseAsync(token);
                    await ProjectFiles.OpenAsync(project, token);
                    break;
                case "chat":
                    // Keep project feature sessions mutually exclusive.  macOS tears down
                    // the previous project workspace before entering chat; doing the same
                    // here prevents a stale Git/Run request from publishing into the page
                    // the user just selected.
                    await ProjectRun.CloseAsync(token);
                    await ProjectGit.CloseAsync(token);
                    await ProjectFiles.CloseAsync(token);
                    ProjectConversationErrorMessage = null;
                    IsPreparingConversation = true;
                    var conversationId = await EnsureProjectConversationAsync(project.Id, token);
                    token.ThrowIfCancellationRequested();
                    await Conversation.OpenAsync(conversationId, project.Name, token);
                    break;
                case "git":
                    await ProjectRun.CloseAsync(token);
                    await ProjectFiles.CloseAsync(token);
                    await ProjectGit.OpenAsync(project, token);
                    break;
                case "run":
                    await ProjectGit.CloseAsync(token);
                    await ProjectFiles.CloseAsync(token);
                    await ProjectRun.OpenAsync(project, token);
                    break;
                case "agent-team":
                    await ProjectRun.CloseAsync(token);
                    await ProjectGit.CloseAsync(token);
                    await ProjectFiles.CloseAsync(token);
                    var owner = RequireAccount(AccountGeneration);
                    if (AgentTeam is not null)
                        await AgentTeam.OpenAsync(owner, project, token);
                    break;
                case "requirement-surveys":
                    await ProjectRun.CloseAsync(token);
                    await ProjectGit.CloseAsync(token);
                    await ProjectFiles.CloseAsync(token);
                    var surveyOwner = RequireAccount(AccountGeneration);
                    if (RequirementSurveys is not null)
                        await RequirementSurveys.OpenAsync(surveyOwner, project, token);
                    break;
            }
        }
        catch (OperationCanceledException) { }
        catch (Exception exception)
        {
            if (!token.IsCancellationRequested)
            {
                if (string.Equals(tab, "chat", StringComparison.Ordinal))
                {
                    ProjectConversationErrorMessage = Localization.Text(
                        $"无法准备项目对话：{exception.Message}",
                        $"Unable to prepare the project conversation: {exception.Message}");
                }
                else
                {
                    ErrorMessage = exception.Message;
                }
            }
        }
        finally
        {
            if (!token.IsCancellationRequested) IsPreparingConversation = false;
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
        ProjectConversationErrorMessage = null;
    }

    private void ApplyPreparedProjectConversation(string projectId, string conversationId)
    {
        _workspaceSnapshot = _workspaceSnapshot with
        {
            Projects = _workspaceSnapshot.Projects.Select(value =>
                string.Equals(value.Id, projectId, StringComparison.Ordinal)
                    ? value with { LatestConversationId = conversationId }
                    : value).ToArray(),
        };

        var index = -1;
        for (var candidate = 0; candidate < Projects.Count; candidate++)
        {
            if (Projects[candidate].Kind == WorkspaceResourceKind.Project &&
                string.Equals(Projects[candidate].Id, projectId, StringComparison.Ordinal))
            {
                index = candidate;
                break;
            }
        }
        if (index < 0) return;

        var updated = Projects[index] with { ConversationId = conversationId };
        Projects[index] = updated;
        if (SelectedResource?.Kind == WorkspaceResourceKind.Project &&
            string.Equals(SelectedResource.Id, projectId, StringComparison.Ordinal))
        {
            // Replacing the selected row keeps WinUI selection aligned with the collection.
            // Suppress resource activation so the in-flight chat-open token is not cancelled.
            SetSelectedResourceWithoutActivation(updated);
        }
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
        var account = AccountGeneration;
        try
        {
            var status = await _localConnectorControl.GetStatusAsync(cancellationToken);
            if (account != AccountGeneration || !IsAuthenticated) return;
            ApplyLocalConnectorStatus(status);
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            if (account != AccountGeneration || !IsAuthenticated) return;
            var resource = CreateBuiltInLocalTerminalResource(
                LocalConnectorStatus,
                Localization.Text("可用", "Available"));
            var connectorIndex = LocalResources.ToList().FindIndex(value =>
                value.Id == BuiltInLocalTerminalId ||
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
        var resource = CreateBuiltInLocalTerminalResource(status);
        var connectorIndex = LocalResources.ToList().FindIndex(value =>
            value.Id == BuiltInLocalTerminalId ||
            value.Kind == WorkspaceResourceKind.LocalConnector);
        if (connectorIndex < 0)
        {
            LocalResources.Insert(0, resource);
        }
        else
        {
            LocalResources[connectorIndex] = resource;
        }

        if (SelectedResource is { } selected &&
            (selected.Id == BuiltInLocalTerminalId ||
             selected.Kind == WorkspaceResourceKind.LocalConnector))
        {
            SetSelectedResourceWithoutActivation(resource);
        }
    }

}
