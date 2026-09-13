using System.Reflection;
using ChatOS.Connector.Persistence;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Workspaces;
using ChatOS.Connector.LocalAgent;
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
    private readonly LocalAgentSession _localAgent = new();
    private MainWindowViewModel _shell = null!;
    private LocalProjectRecord _aliceProject = null!;

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
        _shell = new(_auth, _relations, _registry, _projects, _localAgent,
            _conversations, localControl,
            new ConversationSessionViewModel(new EmptyMainChatService(), null!, null!, dispatcher),
            new ProjectFilesViewModel(null!, dispatcher), new ProjectGitViewModel(null!, dispatcher),
            new ProjectRunViewModel(null!, dispatcher),
            new RemoteConnectionsViewModel(remote, localControl, dispatcher), localization);
    }

    private sealed class LocalAgentSession : IWindowsLocalAgentClientRuntime
    {
        public List<string> ActivatedAccounts { get; } = [];
        public Exception? StartError { get; set; }
        public Task ActivateAsync(string accountId, CancellationToken cancellationToken = default) =>
            StartError is null ? Record(accountId) : Task.FromException(StartError);
        public Task UpdateAccessTokenAsync(string accountId, CancellationToken cancellationToken = default) =>
            Task.CompletedTask;
        public Task LogoutAsync() => Task.CompletedTask;
        private Task Record(string accountId)
        {
            ActivatedAccounts.Add(accountId);
            return Task.CompletedTask;
        }
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
        Assert.Equal("alice", Assert.Single(_localAgent.ActivatedAccounts));
        Assert.Equal("Alice project", Assert.Single(_shell.Projects).Title);
        Assert.Equal("offline", _shell.ErrorMessage);
        Assert.Equal(0, _conversations.Calls);
    }

    [Fact]
    public async Task HostStartupFailureDoesNotPublishAHalfActiveAccount()
    {
        _localAgent.StartError = new InvalidOperationException("Local Agent Host failed to start.");

        await _shell.InitializeAsync();

        Assert.False(_shell.IsAuthenticated);
        Assert.Empty(_shell.Projects);
        Assert.Equal("Local Agent Host failed to start.", _shell.ErrorMessage);
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
    public async Task ExplicitChatPreparationCachesConversationWithoutReplacingSelectedResource()
    {
        await _shell.InitializeAsync();
        _shell.SelectedResource = Assert.Single(_shell.Projects);
        var selected = _shell.SelectedResource;
        var first = await _shell.EnsureProjectConversationScopeAsync(_aliceProject.Id);
        var second = await _shell.EnsureProjectConversationScopeAsync(_aliceProject.Id);
        Assert.Equal(first, second);
        Assert.Equal("alice", first.AccountId);
        Assert.Equal("conversation", first.ThreadId);
        Assert.Equal(_aliceProject.Id, first.ProjectId);
        Assert.Equal("jiguli", first.ContactAgentId);
        Assert.Equal(1, _conversations.Calls);
        Assert.Same(selected, _shell.SelectedResource);
    }

    [Fact]
    public async Task RemovedProjectCannotPrepareNewConversation()
    {
        await _shell.InitializeAsync();
        await _shell.RemoveProjectAsync(_shell.AccountGeneration, _aliceProject);
        await Assert.ThrowsAsync<InvalidOperationException>(() => _shell.EnsureProjectConversationAsync(_aliceProject.Id));
        Assert.Empty(_shell.Projects);
        Assert.Equal(0, _conversations.Calls);
    }

    private sealed class Auth : IAuthenticationService
    {
        public string Owner = "alice";
        public Task<AuthSession?> RestoreSessionAsync(CancellationToken cancellationToken = default) => Task.FromResult<AuthSession?>(new(new(Owner, Owner, null, "user")));
        public Task<AuthSession> LoginAsync(string username, string password, CancellationToken cancellationToken = default) => Task.FromResult(new AuthSession(new(Owner, Owner, null, "user")));
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
        public TaskCompletionSource Entered = new();
        public Func<Task<string>> Load = () => Task.FromResult("conversation");
        public Task<string> EnsureConversationAsync(WorkspaceProject project, WorkspaceContact contact, CancellationToken cancellationToken = default)
        {
            Calls++;
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

    private sealed class EmptyMainChatService : ILocalAgentMainChatService
    {
        public event EventHandler? ProjectionChanged { add { } remove { } }
        public event EventHandler? ProjectionCleared { add { } remove { } }
        public Task<LocalAgentConversationSnapshot> GetConversationAsync(
            string threadId, CancellationToken cancellationToken = default) =>
            Task.FromResult(new LocalAgentConversationSnapshot("alice", threadId, []));
        public Task<LocalAgentRunCreatedResponse> CreateTurnAsync(
            LocalAgentCreateConversationTurn command, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
        public Task CancelTurnAsync(string threadId, string turnId, string runId,
            ulong expectedVersion, CancellationToken cancellationToken = default) =>
            throw new NotSupportedException();
    }

    public class ServiceStub : DispatchProxy
    {
        public Func<MethodInfo, object?[]?, object?> Call = null!;
        protected override object? Invoke(MethodInfo? targetMethod, object?[]? args) => Call(targetMethod!, args);
    }
}
