using System.Net;
using System.Reflection;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;
using ChatOS.Desktop.AppShell;
using ChatOS.Presentation.Chat;
using ChatOS.Presentation.Projects;
using ChatOS.Presentation.Remote;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Connector.Tests;

public sealed class ClientOwnedShellTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(Path.GetTempPath(), "chatos-shell-projects-" + Guid.NewGuid().ToString("N"));
    private SqliteProjectRegistry _registry = null!;
    private LocalProjectsService _projects = null!;
    private readonly Auth _auth = new();
    private readonly Relations _relations = new();
    private readonly Conversations _conversations = new();
    private MainWindowViewModel _shell = null!;
    private LocalProjectRecord _aliceProject = null!;
    private PetFavoriteProjectsManager _favorites = null!;
    private Filesystem _filesystem = null!;
    private ProjectFilesViewModel _projectFiles = null!;

    public async Task InitializeAsync()
    {
        var database = new LocalStateDatabase(Path.Combine(_directory, "state.db"));
        await database.InitializeAsync();
        _registry = new(database);
        _aliceProject = await _registry.CreateAsync("alice", new("Alice project", "workspace"));
        await _registry.CreateAsync("bob", new("Bob project", "workspace"));
        var stateStore = new SqliteConnectorPersistentStateStore(database);
        await stateStore.SaveAsync(new ConnectorPersistentState(
            new Uri("https://gateway.example"),
            new ConnectorUser("alice", "alice", "Alice", "user"),
            "device",
            "Windows PC",
            [new ConnectorWorkspace("workspace", "Workspace", _directory, "fingerprint")],
            new RemoteControlTrust(false, 120, new Dictionary<string, string>())));
        var runtime = new ConnectorRuntimeContext(stateStore,
            Stub<IConnectorAccessTokenStore>((_, _) => ValueTask.FromResult<string?>(null)));
        _projects = new(_registry, runtime);
        var dispatcher = new ImmediateUiDispatcher();
        var localControl = Stub<ILocalConnectorControlService>((method, _) => method.Name == "GetStatusAsync"
            ? Task.FromResult(new LocalConnectorStatus(false, "Disconnected", null, null, null, null, null, null, null, []))
            : throw new NotSupportedException(method.Name));
        var remote = Stub<IRemoteConnectionService>((method, _) => method.Name == "ListAsync"
            ? Task.FromResult<IReadOnlyList<RemoteConnection>>([]) : throw new NotSupportedException(method.Name));
        var localization = new LocalizationViewModel(new AppPreferencesManager(
            Stub<IAppPreferencesStore>((_, _) => throw new NotSupportedException())), dispatcher);
        _favorites = new PetFavoriteProjectsManager(new MemoryFavoriteStore([_aliceProject.Id]));
        await _favorites.InitializeAsync();
        _filesystem = new Filesystem();
        _projectFiles = new ProjectFilesViewModel(_filesystem, dispatcher);
        _shell = new(_auth, _relations, _registry, _projects, _conversations, localControl,
            new ConversationSessionViewModel(null!, null!, null!, null!, null!, null!, new(), dispatcher),
            _projectFiles, new ProjectGitViewModel(null!, dispatcher),
            new ProjectRunViewModel(null!, dispatcher),
            new RemoteConnectionsViewModel(remote, localControl, dispatcher), localization,
            petFavorites: _favorites);
    }

    public async Task DisposeAsync()
    {
        await _shell.LogoutCommand.ExecuteAsync(null);
        Directory.Delete(_directory, recursive: true);
    }

    [Fact]
    public async Task PublishesLocalProjectsBeforeRemoteResponseAndRetainsThemOffline()
    {
        var pending = new TaskCompletionSource<WorkspaceRelationsSnapshot>();
        _relations.Load = () => pending.Task;
        var loading = _shell.InitializeAsync();
        await _relations.Entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
        Assert.Equal(_aliceProject.Id, Assert.Single(_shell.Projects).Id);
        Assert.False(loading.IsCompleted);
        pending.SetException(new IOException("offline"));
        await loading;
        Assert.Equal("Alice project", Assert.Single(_shell.Projects).Title);
        Assert.Equal("offline", _shell.ErrorMessage);
        Assert.Equal(0, _conversations.Calls);
    }

    [Fact]
    public async Task LogoutDiscardsLateProjectAndContactResults()
    {
        var pending = new TaskCompletionSource<WorkspaceRelationsSnapshot>();
        _relations.Load = () => pending.Task;
        var loading = _shell.InitializeAsync();
        await _relations.Entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
        await _shell.LogoutCommand.ExecuteAsync(null);
        pending.SetResult(Relations.Default);
        await loading;
        Assert.False(_shell.IsAuthenticated);
        Assert.Empty(_shell.Projects);
        Assert.Empty(_shell.Contacts);
        Assert.Null(_shell.ErrorMessage);
    }

    [Fact]
    public async Task AccountSwitchDoesNotMixProjectsOrAcceptOldDialogMutation()
    {
        await _shell.InitializeAsync();
        var aliceGeneration = _shell.AccountGeneration;
        await _shell.LogoutCommand.ExecuteAsync(null);
        _auth.Owner = "bob";
        _shell.Username = "bob";
        _shell.Password = "secret";
        await _shell.LoginCommand.ExecuteAsync(null);
        Assert.Equal("Bob project", Assert.Single(_shell.Projects).Title);
        await Assert.ThrowsAsync<OperationCanceledException>(() => _shell.RenameProjectAsync(aliceGeneration, _aliceProject, "Wrong account"));
        Assert.Equal("Alice project", (await _registry.GetAsync("alice", _aliceProject.Id))!.Draft.Name);
    }

    [Fact]
    public async Task RenameDuringRemoteRefreshWinsAndDoesNotCreateConversation()
    {
        var pending = new TaskCompletionSource<WorkspaceRelationsSnapshot>();
        _relations.Load = () => pending.Task;
        var loading = _shell.InitializeAsync();
        await _relations.Entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
        await _shell.RenameProjectAsync(_shell.AccountGeneration, _aliceProject, "Current name");
        pending.SetResult(Relations.Default);
        await loading;
        Assert.Equal("Current name", Assert.Single(_shell.Projects).Title);
        Assert.Equal(0, _conversations.Calls);
    }

    [Fact]
    public async Task FailedRemoteRefreshPreservesSameAccountConversationLinks()
    {
        _relations.Load = () => Task.FromResult(new WorkspaceRelationsSnapshot(Relations.Default.Contacts,
            [new("conversation", "Chat", _aliceProject.Id, "contact", "jiguli", 1, DateTimeOffset.UtcNow, false)]));
        await _shell.InitializeAsync();
        Assert.Equal("conversation", Assert.Single(_shell.Projects).ConversationId);
        _relations.Load = () => Task.FromException<WorkspaceRelationsSnapshot>(new IOException("offline"));
        await _shell.RefreshWorkspaceCommand.ExecuteAsync(null);
        Assert.Equal("conversation", Assert.Single(_shell.Projects).ConversationId);
        Assert.Contains(_shell.Contacts, contact => contact.Id == "contact");
    }

    [Fact]
    public async Task ExpiredConnectorCredentialIsRenewedAndLocalAgentBootstrapRetriesOnce()
    {
        await _shell.LogoutCommand.ExecuteAsync(null);
        var control = new CredentialControl();
        var tickets = new PairingTickets();
        var host = new LocalAgentHost(
            new ConnectorGatewayException(HttpStatusCode.Unauthorized, "invalid or expired token"),
            null);
        ReplaceShell(control, host, tickets);

        await _shell.InitializeAsync();

        Assert.Equal(2, host.StartCount);
        Assert.Equal(1, tickets.IssueCount);
        Assert.Equal(1, control.RefreshCount);
        Assert.Equal("ticket-1", control.LastTicket);
        Assert.Null(_shell.ErrorMessage);
        Assert.True(_shell.IsAuthenticated);
    }

    [Fact]
    public async Task RefreshResourcesRetriesAPreviouslyUnavailableLocalAgent()
    {
        await _shell.LogoutCommand.ExecuteAsync(null);
        var control = new CredentialControl();
        var host = new LocalAgentHost(new IOException("host unavailable"), null);
        ReplaceShell(control, host, null);

        await _shell.InitializeAsync();
        Assert.Contains("本地智能体暂时不可用", _shell.ErrorMessage);
        Assert.Equal(1, host.StartCount);

        await _shell.RefreshWorkspaceCommand.ExecuteAsync(null);

        Assert.Equal(2, host.StartCount);
        Assert.Null(_shell.ErrorMessage);
    }

    [Fact]
    public async Task ContactSubtitleUsesTheLatestConversationTitle()
    {
        _relations.Load = () => Task.FromResult(new WorkspaceRelationsSnapshot(Relations.Default.Contacts,
            [new("contact-conversation", "Jiguli chat", null, "contact", "jiguli", 2,
                DateTimeOffset.UtcNow, false)]));

        await _shell.InitializeAsync();

        var contact = Assert.Single(_shell.Contacts);
        Assert.Equal("Jiguli chat", contact.Subtitle);
        Assert.Equal("contact-conversation", contact.ConversationId);
    }

    [Fact]
    public async Task RefreshReopensSelectedContactWhenItsConversationBecomesAvailable()
    {
        await _shell.InitializeAsync();
        _shell.SelectedResource = Assert.Single(_shell.Contacts);
        Assert.Null(_shell.SelectedResource.ConversationId);

        _relations.Load = () => Task.FromResult(new WorkspaceRelationsSnapshot(Relations.Default.Contacts,
            [new("contact-conversation", "Jiguli chat", null, "contact", "jiguli", 1,
                DateTimeOffset.UtcNow, false)]));
        await _shell.RefreshWorkspaceCommand.ExecuteAsync(null);

        await WaitUntilAsync(() => _shell.Conversation.ConversationId == "contact-conversation");
        Assert.Equal("contact-conversation", _shell.SelectedResource?.ConversationId);
        Assert.Equal("Jiguli", _shell.Conversation.Title);
    }

    [Fact]
    public async Task PluginApplicationCountUpdatesTheSidebarAndSelectedResource()
    {
        await _shell.InitializeAsync();
        _shell.SelectedResource = Assert.Single(_shell.ApplicationResources,
            value => value.Kind == WorkspaceResourceKind.Applications);

        _shell.UpdatePluginApplicationCount(3);

        var applications = Assert.Single(_shell.ApplicationResources,
            value => value.Kind == WorkspaceResourceKind.Applications);
        Assert.Equal("3 个可用", applications.Subtitle);
        Assert.Same(applications, _shell.SelectedResource);

        _shell.UpdatePluginApplicationCount(null);
        applications = Assert.Single(_shell.ApplicationResources,
            value => value.Kind == WorkspaceResourceKind.Applications);
        Assert.Equal(_shell.Localization.InstalledPluginApplications, applications.Subtitle);
    }

    [Fact]
    public async Task ConversationResultAfterLogoutCannotMutateWorkspace()
    {
        await _shell.InitializeAsync();
        var pending = new TaskCompletionSource<string>();
        _conversations.Load = () => pending.Task;
        var preparing = _shell.EnsureProjectConversationAsync(_aliceProject.Id);
        await _conversations.Entered.Task.WaitAsync(TimeSpan.FromSeconds(5));
        await _shell.LogoutCommand.ExecuteAsync(null);
        pending.SetResult("old-account-conversation");
        await Assert.ThrowsAsync<OperationCanceledException>(() => preparing);
        Assert.Empty(_shell.Projects);
        Assert.Null(_shell.SelectedResource);
    }

    [Fact]
    public async Task SelectingAndRefreshingProjectDoesNotPrepareChatOrLoseSelection()
    {
        await _shell.InitializeAsync();
        _shell.SelectedResource = Assert.Single(_shell.Projects);
        // A WinUI ListView clears SelectedItem when its ItemsSource is reset.
        _shell.Projects.CollectionChanged += (_, change) =>
        {
            if (change.Action == System.Collections.Specialized.NotifyCollectionChangedAction.Reset)
                _shell.SelectedResource = null;
        };
        await _shell.RefreshWorkspaceCommand.ExecuteAsync(null);
        Assert.Equal(_aliceProject.Id, _shell.SelectedResource?.Id);
        Assert.Equal(0, _conversations.Calls);
        Assert.False(_shell.IsPublishingWorkspace);
    }

    [Fact]
    public async Task ExplicitChatPreparationCachesConversationAndUpdatesSelectedProjectResource()
    {
        await _shell.InitializeAsync();
        _shell.SelectedResource = Assert.Single(_shell.Projects);
        var selected = _shell.SelectedResource;
        var first = await _shell.EnsureProjectConversationAsync(_aliceProject.Id);
        var second = await _shell.EnsureProjectConversationAsync(_aliceProject.Id);
        Assert.Equal(first, second);
        Assert.Equal(1, _conversations.Calls);
        Assert.NotSame(selected, _shell.SelectedResource);
        Assert.Equal(first, Assert.Single(_shell.Projects).ConversationId);
        Assert.Equal(first, _shell.SelectedResource?.ConversationId);
    }

    [Fact]
    public async Task FirstProjectChatEntryVerifiesAStaleWorkspaceConversationWithTheHost()
    {
        _relations.Load = () => Task.FromResult(new WorkspaceRelationsSnapshot(
            Relations.Default.Contacts,
            [new("stale-conversation", "Stale chat", _aliceProject.Id, "contact", "jiguli", 1,
                DateTimeOffset.UtcNow, false)]));
        _conversations.Load = () => Task.FromResult("bound-conversation");

        await _shell.InitializeAsync();
        _shell.SelectedResource = Assert.Single(_shell.Projects);
        Assert.Equal("stale-conversation", _shell.SelectedResource.ConversationId);

        await _shell.OpenProjectTabAsync("chat");

        Assert.Equal(1, _conversations.Calls);
        Assert.Equal("bound-conversation", _shell.Conversation.ConversationId);
        Assert.Equal("bound-conversation", Assert.Single(_shell.Projects).ConversationId);
        Assert.Equal(_aliceProject.Id, _conversations.LastProject?.ProjectContext?.ProjectId);

        await _shell.OpenProjectTabAsync("files");
        await _shell.OpenProjectTabAsync("chat");

        Assert.Equal(1, _conversations.Calls);
        Assert.Equal("bound-conversation", _shell.Conversation.ConversationId);
    }

    [Fact]
    public async Task EmptyHostConversationIdentityFailsClosedWithoutCachingWorkspaceIdentity()
    {
        _relations.Load = () => Task.FromResult(new WorkspaceRelationsSnapshot(
            Relations.Default.Contacts,
            [new("stale-conversation", "Stale chat", _aliceProject.Id, "contact", "jiguli", 1,
                DateTimeOffset.UtcNow, false)]));
        _conversations.Load = () => Task.FromResult("   ");

        await _shell.InitializeAsync();
        _shell.SelectedResource = Assert.Single(_shell.Projects);

        await _shell.OpenProjectTabAsync("chat");

        Assert.True(_shell.HasProjectConversationError);
        Assert.Contains("无效的项目会话标识", _shell.ProjectConversationErrorMessage);
        Assert.Null(_shell.Conversation.ConversationId);
        Assert.Equal("stale-conversation", Assert.Single(_shell.Projects).ConversationId);

        _conversations.Load = () => Task.FromResult("recovered-conversation");
        await _shell.OpenProjectTabAsync("chat");

        Assert.Equal(2, _conversations.Calls);
        Assert.Equal("recovered-conversation", _shell.Conversation.ConversationId);
    }

    [Fact]
    public async Task CreatedProjectOpensOneBoundChatAndCanReenterFilesAndChat()
    {
        await _shell.InitializeAsync();
        var folder = Path.Combine(_directory, "created-chat-project");
        Directory.CreateDirectory(folder);
        var draft = new LocalProjectDraft(
            "Created chat project",
            "workspace",
            "created-chat-project",
            "Created for the project chat path",
            "desktop_application");

        await _shell.CreateProjectAsync(_shell.AccountGeneration, draft, _directory);
        var created = Assert.Single(_shell.Projects, value => value.Title == "Created chat project");
        await _shell.OpenProjectTabAsync("chat");

        Assert.Equal("conversation", _shell.Conversation.ConversationId);
        Assert.Equal("conversation", Assert.Single(_shell.Projects, value => value.Id == created.Id).ConversationId);
        Assert.Equal("conversation", _shell.SelectedResource?.ConversationId);
        Assert.Equal(created.Id, _conversations.LastProject?.Id);
        Assert.Equal(created.Id, _conversations.LastProject?.ProjectContext?.ProjectId);
        Assert.Equal(1, _conversations.Calls);
        Assert.False(_shell.IsPreparingConversation);
        Assert.Null(_shell.ProjectConversationErrorMessage);

        await _shell.OpenProjectTabAsync("files");
        Assert.True(_projectFiles.IsOpen);
        Assert.Equal(created.Id, _projectFiles.ProjectId);
        await _shell.OpenProjectTabAsync("chat");

        Assert.Equal("conversation", _shell.Conversation.ConversationId);
        Assert.Equal(1, _conversations.Calls);
    }

    [Fact]
    public async Task FailedProjectChatPreparationShowsRetryStateAndDoesNotCacheConversation()
    {
        await _shell.InitializeAsync();
        _shell.SelectedResource = Assert.Single(_shell.Projects);
        _conversations.Load = () => Task.FromException<string>(new IOException("host unavailable"));

        await _shell.OpenProjectTabAsync("chat");

        Assert.False(_shell.IsPreparingConversation);
        Assert.True(_shell.HasProjectConversationError);
        Assert.Contains("host unavailable", _shell.ProjectConversationErrorMessage);
        Assert.Null(Assert.Single(_shell.Projects).ConversationId);
        Assert.Null(_shell.SelectedResource?.ConversationId);

        _conversations.Load = () => Task.FromResult("conversation-after-retry");
        await _shell.OpenProjectTabAsync("chat");

        Assert.False(_shell.HasProjectConversationError);
        Assert.Equal("conversation-after-retry", _shell.Conversation.ConversationId);
        Assert.Equal("conversation-after-retry", Assert.Single(_shell.Projects).ConversationId);
        Assert.Equal(2, _conversations.Calls);
    }

    [Fact]
    public async Task CreatingProjectSelectsItAndOpensFilesWithoutPreparingChat()
    {
        await _shell.InitializeAsync();
        var folder = Path.Combine(_directory, "created-project");
        Directory.CreateDirectory(folder);
        var draft = new LocalProjectDraft(
            "Created project",
            "workspace",
            "created-project",
            "Created from the Windows dialog",
            "desktop_application");

        await _shell.CreateProjectAsync(_shell.AccountGeneration, draft, _directory);
        var openedPath = await _filesystem.Opened.Task.WaitAsync(TimeSpan.FromSeconds(5));

        var selected = Assert.Single(_shell.Projects, value => value.Title == "Created project");
        Assert.Same(selected, _shell.SelectedResource);
        Assert.Equal(WorkspaceResourceKind.Project, selected.Kind);
        Assert.Equal(selected.Id, _projectFiles.ProjectId);
        Assert.True(_projectFiles.IsOpen);
        Assert.Equal("local://connector/device/workspace/created-project", openedPath);
        Assert.Equal("desktop_application",
            (await _registry.GetAsync("alice", selected.Id))?.Draft.ProjectTypeKey);
        Assert.Equal(0, _conversations.Calls);
    }

    [Fact]
    public async Task ReturningToFilesAfterAnotherProjectFeatureReopensTheFileSession()
    {
        await _shell.InitializeAsync();
        _shell.SelectedResource = Assert.Single(_shell.Projects);
        await WaitUntilAsync(() => _projectFiles.IsOpen);
        await _projectFiles.CloseAsync();
        Assert.False(_projectFiles.IsOpen);

        await _shell.OpenProjectTabAsync("files");

        Assert.True(_projectFiles.IsOpen);
        Assert.Equal(_aliceProject.Id, _projectFiles.ProjectId);
        Assert.Equal("local://connector/device/workspace", _projectFiles.ProjectRoot);
    }

    [Fact]
    public async Task RemovedProjectCannotPrepareNewConversation()
    {
        await _shell.InitializeAsync();
        await _shell.RemoveProjectAsync(_shell.AccountGeneration, _aliceProject);
        await Assert.ThrowsAsync<InvalidOperationException>(() => _shell.EnsureProjectConversationAsync(_aliceProject.Id));
        Assert.Empty(_shell.Projects);
        Assert.Equal(0, _conversations.Calls);
        Assert.False(_favorites.IsFavorite(_aliceProject.Id));
    }

    private sealed class MemoryFavoriteStore(IReadOnlyList<string> values) : IPetFavoriteProjectsStore
    {
        private IReadOnlyList<string> _values = values;

        public Task<IReadOnlyList<string>> LoadAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(_values);

        public Task SaveAsync(
            IReadOnlyCollection<string> projectIds,
            CancellationToken cancellationToken = default)
        {
            _values = projectIds.ToArray();
            return Task.CompletedTask;
        }
    }

    private static async Task WaitUntilAsync(Func<bool> condition)
    {
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        while (!condition())
        {
            await Task.Delay(10, timeout.Token);
        }
    }

    private void ReplaceShell(
        ILocalConnectorControlService localControl,
        ILocalAgentHostLifecycle localAgentHost,
        ILocalConnectorPairingTicketService? pairingTickets)
    {
        var dispatcher = new ImmediateUiDispatcher();
        var remote = Stub<IRemoteConnectionService>((method, _) => method.Name == "ListAsync"
            ? Task.FromResult<IReadOnlyList<RemoteConnection>>([])
            : throw new NotSupportedException(method.Name));
        var localization = new LocalizationViewModel(new AppPreferencesManager(
            Stub<IAppPreferencesStore>((_, _) => throw new NotSupportedException())), dispatcher);
        _projectFiles = new ProjectFilesViewModel(_filesystem, dispatcher);
        _shell = new MainWindowViewModel(
            _auth,
            _relations,
            _registry,
            _projects,
            _conversations,
            localControl,
            new ConversationSessionViewModel(null!, null!, null!, null!, null!, null!, new(), dispatcher),
            _projectFiles,
            new ProjectGitViewModel(null!, dispatcher),
            new ProjectRunViewModel(null!, dispatcher),
            new RemoteConnectionsViewModel(remote, localControl, dispatcher),
            localization,
            localAgentHost: localAgentHost,
            petFavorites: _favorites,
            localConnectorPairingTickets: pairingTickets);
    }

    private sealed class CredentialControl : ILocalConnectorControlService
    {
        private readonly LocalConnectorStatus _status = new(
            true,
            "Connected",
            "alice",
            "device",
            "Windows PC",
            "https://gateway.example",
            null,
            null,
            null,
            [new LocalConnectorWorkspaceStatus("workspace", "Workspace", "C:\\")]);

        public int RefreshCount { get; private set; }

        public string? LastTicket { get; private set; }

        public Task<LocalConnectorStatus> GetStatusAsync(
            CancellationToken cancellationToken = default) => Task.FromResult(_status);

        public Task<LocalConnectorStatus> PairAsync(
            LocalConnectorPairingDraft draft,
            string ticket,
            CancellationToken cancellationToken = default) => throw new NotSupportedException();

        public Task<LocalConnectorStatus> RefreshCredentialAsync(
            string ticket,
            CancellationToken cancellationToken = default)
        {
            RefreshCount++;
            LastTicket = ticket;
            return Task.FromResult(_status);
        }

        public Task DisconnectAsync(CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
    }

    private sealed class PairingTickets : ILocalConnectorPairingTicketService
    {
        public int IssueCount { get; private set; }

        public Task<string> IssueAsync(CancellationToken cancellationToken = default)
        {
            IssueCount++;
            return Task.FromResult($"ticket-{IssueCount}");
        }
    }

    private sealed class LocalAgentHost(params Exception?[] starts) : ILocalAgentHostLifecycle
    {
        private readonly Queue<Exception?> _starts = new(starts);

        public string? ActiveOwnerUserId { get; private set; }

        public int StartCount { get; private set; }

        public Task StartForOwnerAsync(
            string ownerUserId,
            CancellationToken cancellationToken = default)
        {
            StartCount++;
            if (_starts.Count > 0 && _starts.Dequeue() is { } exception)
            {
                return Task.FromException(exception);
            }

            ActiveOwnerUserId = ownerUserId;
            return Task.CompletedTask;
        }

        public Task StopAsync(CancellationToken cancellationToken = default)
        {
            ActiveOwnerUserId = null;
            return Task.CompletedTask;
        }
    }

    private sealed class Filesystem : IProjectFilesystemService
    {
        public TaskCompletionSource<string> Opened { get; } =
            new(TaskCreationOptions.RunContinuationsAsynchronously);

        public Task<ProjectDirectoryListing> ListEntriesAsync(
            string path,
            bool forceRefresh = false,
            CancellationToken cancellationToken = default)
        {
            Opened.TrySetResult(path);
            return Task.FromResult(new ProjectDirectoryListing(path, null, true, [], false));
        }

        public Task<IReadOnlyList<ProjectFileEntry>> SearchEntriesAsync(
            string path, string query, int limit = 100,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ProjectFileEntry>>([]);

        public Task<IReadOnlyList<ProjectFileContentMatch>> SearchContentAsync(
            string path, string query, int limit = 100,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<ProjectFileContentMatch>>([]);

        public Task<ProjectFileContent> ReadFileAsync(
            string path,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task WriteFileAsync(
            string path, string content,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task CreateFileAsync(
            string parentPath, string name,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task CreateDirectoryAsync(
            string parentPath, string name,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task DeleteEntryAsync(
            string path, bool recursive,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task<ProjectFileMoveResult> MoveEntryAsync(
            string sourcePath, string targetParentPath, string? targetName = null,
            bool replaceExisting = false,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();

        public Task OpenExternallyAsync(
            string path, ProjectFileExternalOpenMode mode,
            CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
    }

    [Fact]
    public async Task RegistrationRejectsMismatchedPasswordsBeforeCallingService()
    {
        await _shell.LogoutCommand.ExecuteAsync(null);
        _shell.ShowRegistrationCommand.Execute(null);
        _shell.Username = "person@example.com";
        _shell.InviteCode = "invite-123";
        _shell.VerificationCode = "123456";
        _shell.Password = "secret-value";
        _shell.ConfirmPassword = "different-value";

        await _shell.RegisterCommand.ExecuteAsync(null);

        Assert.Equal("两次输入的密码不一致。", _shell.ErrorMessage);
        Assert.Equal(0, _auth.RegisterCalls);
    }

    [Fact]
    public async Task RegistrationAuthenticatesNewAccountAndPersistsWorkspaceIdentity()
    {
        await _shell.LogoutCommand.ExecuteAsync(null);
        _shell.ShowRegistrationCommand.Execute(null);
        _shell.Username = "person@example.com";
        _shell.InviteCode = "invite-123";
        _shell.VerificationCode = "123456";
        _shell.Password = _shell.ConfirmPassword = "secret-value";
        _auth.Owner = "alice";

        await _shell.RegisterCommand.ExecuteAsync(null);

        Assert.True(_shell.IsAuthenticated);
        Assert.Equal(1, _auth.RegisterCalls);
        Assert.Equal("alice", _shell.CurrentOwnerUserId);
        Assert.Empty(_shell.Password);
        Assert.Empty(_shell.ConfirmPassword);
        Assert.Single(_shell.Projects);
    }

    private sealed class Auth : IAuthenticationService
    {
        public string Owner = "alice";
        public int CodeCalls;
        public int RegisterCalls;
        public Task<AuthSession?> RestoreSessionAsync(CancellationToken cancellationToken = default) => Task.FromResult<AuthSession?>(new(new(Owner, Owner, null, "user")));
        public Task<AuthSession> LoginAsync(string username, string password, CancellationToken cancellationToken = default) => Task.FromResult(new AuthSession(new(Owner, Owner, null, "user")));
        public Task<RegistrationCodeDelivery> SendRegistrationCodeAsync(string email, string inviteCode, CancellationToken cancellationToken = default)
        {
            CodeCalls++;
            return Task.FromResult(new RegistrationCodeDelivery(600, 60));
        }
        public Task<AuthSession> RegisterAsync(string email, string password, string inviteCode, string verificationCode, CancellationToken cancellationToken = default)
        {
            RegisterCalls++;
            return Task.FromResult(new AuthSession(new(Owner, Owner, null, "user")));
        }
        public ValueTask LogoutAsync(CancellationToken cancellationToken = default) => ValueTask.CompletedTask;
    }

    private sealed class Relations : IWorkspaceRelationsService
    {
        public static WorkspaceRelationsSnapshot Default => new([new("contact", "jiguli", "Jiguli", null)], []);
        public Func<Task<WorkspaceRelationsSnapshot>> Load = () => Task.FromResult(Default);
        public TaskCompletionSource Entered = new();
        public Task<WorkspaceRelationsSnapshot> FetchWorkspaceRelationsAsync(CancellationToken cancellationToken = default)
        {
            Entered.TrySetResult();
            return Load(); // Deliberately ignore cancellation to exercise stale-result guards.
        }
    }

    private sealed class Conversations : IProjectConversationService
    {
        public int Calls;
        public WorkspaceProject? LastProject;
        public TaskCompletionSource Entered = new();
        public Func<Task<string>> Load = () => Task.FromResult("conversation");
        public Task<string> EnsureConversationAsync(WorkspaceProject project, WorkspaceContact contact, CancellationToken cancellationToken = default)
        {
            Calls++;
            LastProject = project;
            Entered.TrySetResult();
            return Load();
        }
    }

    private static T Stub<T>(Func<MethodInfo, object?[]?, object?> call) where T : class
    {
        var instance = DispatchProxy.Create<T, ServiceStub>();
        ((ServiceStub)(object)instance).Call = call;
        return instance;
    }

    public class ServiceStub : DispatchProxy
    {
        public Func<MethodInfo, object?[]?, object?> Call = null!;
        protected override object? Invoke(MethodInfo? targetMethod, object?[]? args) => Call(targetMethod!, args);
    }
}
