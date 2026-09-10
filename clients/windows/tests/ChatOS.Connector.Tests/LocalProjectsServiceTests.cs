using ChatOS.Connector.Persistence;
using ChatOS.Connector.Relay;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Tests;

public sealed class LocalProjectsServiceTests : IAsyncLifetime
{
    private readonly string _root = Path.Combine(Path.GetTempPath(), "chatos-local-projects-" + Guid.NewGuid().ToString("N"));
    private SqliteProjectRegistry _registry = null!;
    private ConnectorRuntimeContext _runtime = null!;
    private LocalProjectsService _service = null!;

    public async Task InitializeAsync()
    {
        Directory.CreateDirectory(Path.Combine(_root, "repo"));
        var db = new LocalStateDatabase(Path.Combine(_root, "state.db"));
        await db.InitializeAsync();
        _registry = new(db);
        _runtime = new(new SqliteConnectorPersistentStateStore(db), new NoTokenStore());
        await _runtime.InitializeAsync();
        await _runtime.ReplaceAsync(new(new Uri("https://unreachable.invalid"),
            new("alice", "alice", null, "user"), "device", "PC",
            [new("workspace", "Workspace", _root, "fingerprint")], new(true, 300, new Dictionary<string, string>())));
        _service = new(_registry, _runtime);
    }

    public Task DisposeAsync()
    {
        Directory.Delete(_root, recursive: true);
        return Task.CompletedTask;
    }

    [Fact]
    public async Task CreateOfflineWithoutGitAndResolveFromLatestLocalRecord()
    {
        var created = await _service.CreateAsync("alice", new("Local project", "workspace", "repo"), _root);
        Assert.False(Directory.Exists(Path.Combine(_root, "repo", ".git")));
        var original = await _service.ResolveContextAsync("alice", created.Id);
        var renamed = await _service.RenameAsync("alice", created.Id, created.Revision, "Renamed");
        var current = await _service.ResolveContextAsync("alice", created.Id);
        Assert.Equal(created.Id, current.ProjectId);
        Assert.Equal("Local project", original.ProjectName);
        Assert.Equal("Renamed", current.ProjectName);
        Assert.Equal(renamed.Revision, current.ProjectRevision);
        Assert.Equal(new ProjectContextExecutionTarget("device", "workspace", "repo"), current.ExecutionTarget);
    }

    [Fact]
    public async Task WorkspaceReconfiguredAfterDialogConfirmationCannotRedirectCreation()
    {
        await _runtime.ReplaceAsync(_runtime.Snapshot.State! with
        {
            Workspaces = [new("workspace", "Workspace", Path.Combine(_root, "repo"), "changed-fingerprint")],
        });
        await Assert.ThrowsAsync<InvalidOperationException>(() => _service.CreateAsync("alice", new("Project", "workspace"), _root));
        Assert.Empty(await _registry.ListAsync("alice"));
    }

    [Fact]
    public async Task WrongAccountCannotCreateOrResolve()
    {
        var created = await _service.CreateAsync("alice", new("Project", "workspace"), _root);
        Assert.Null(await _service.GetDeviceIdAsync("bob"));
        await Assert.ThrowsAsync<InvalidOperationException>(() => _service.CreateAsync("bob", new("Project", "workspace"), _root));
        await Assert.ThrowsAsync<InvalidOperationException>(() => _service.ResolveContextAsync("bob", created.Id));
        await Assert.ThrowsAsync<ProjectRegistryException>(() => _service.RenameAsync("bob", created.Id, 1, "Wrong"));
        Assert.Empty(await _registry.ListAsync("bob"));
    }

    [Theory]
    [InlineData("workspace", "missing")]
    [InlineData("unknown", "")]
    [InlineData("workspace", "../escape")]
    [InlineData("workspace", "repo/../repo")]
    [InlineData("workspace", "/repo")]
    [InlineData("workspace", "state.db")]
    public async Task InvalidOrUnavailableDirectoryNeverCreatesRecord(string workspace, string relative)
    {
        await Assert.ThrowsAnyAsync<Exception>(() => _service.CreateAsync("alice", new("Project", workspace, relative), _root));
        Assert.Empty(await _registry.ListAsync("alice"));
    }

    [Fact]
    public async Task RejectsSymlinkDirectory()
    {
        Directory.CreateSymbolicLink(Path.Combine(_root, "link"), Path.Combine(_root, "repo"));
        await Assert.ThrowsAsync<RelayRequestException>(() => _service.CreateAsync("alice", new("Project", "workspace", "link"), _root));
        Assert.Empty(await _registry.ListAsync("alice"));
    }

    [Fact]
    public async Task RemovalPreservesFilesAndPreventsContextResolution()
    {
        var created = await _service.CreateAsync("alice", new("Project", "workspace", "repo"), _root);
        await _service.RemoveAsync("alice", created.Id, created.Revision);
        Assert.True(Directory.Exists(Path.Combine(_root, "repo")));
        Assert.Empty(await _registry.ListAsync("alice"));
        Assert.Equal(LocalProjectStatus.Removed, (await _registry.GetAsync("alice", created.Id))!.Status);
        await Assert.ThrowsAsync<ProjectRegistryException>(() => _service.ResolveContextAsync("alice", created.Id));
        await Assert.ThrowsAsync<ProjectRegistryException>(() => _service.RenameAsync("alice", created.Id, 2, "Revive"));
    }

    [Fact]
    public async Task RevokedWorkspaceCannotProduceContextButCanBeRemovedOffline()
    {
        var created = await _service.CreateAsync("alice", new("Project", "workspace", "repo"), _root);
        await _runtime.ReplaceAsync(_runtime.Snapshot.State! with { Workspaces = [] });
        await Assert.ThrowsAsync<InvalidOperationException>(() => _service.ResolveContextAsync("alice", created.Id));
        await _runtime.ReplaceAsync(null);
        await _service.RenameAsync("alice", created.Id, 1, "Offline rename");
        await _service.RemoveAsync("alice", created.Id, 2);
        Assert.Empty(await _registry.ListAsync("alice"));
    }

    [Fact]
    public async Task StaleRenameOrRemovalCannotOverwriteNewRevision()
    {
        var created = await _service.CreateAsync("alice", new("Project", "workspace"), _root);
        await _service.RenameAsync("alice", created.Id, 1, "Current");
        var rename = await Assert.ThrowsAsync<ProjectRegistryException>(() => _service.RenameAsync("alice", created.Id, 1, "Stale"));
        var remove = await Assert.ThrowsAsync<ProjectRegistryException>(() => _service.RemoveAsync("alice", created.Id, 1));
        Assert.Equal(ProjectRegistryError.RevisionConflict, rename.Code);
        Assert.Equal(ProjectRegistryError.RevisionConflict, remove.Code);
        Assert.Equal("Current", (await _registry.GetAsync("alice", created.Id))!.Draft.Name);
    }

    private sealed class NoTokenStore : IConnectorAccessTokenStore
    {
        public ValueTask<string?> GetAccessTokenAsync(CancellationToken cancellationToken = default) => ValueTask.FromResult<string?>(null);
        public ValueTask SetAccessTokenAsync(string token, CancellationToken cancellationToken = default) => throw new NotSupportedException();
        public ValueTask ClearAsync(CancellationToken cancellationToken = default) => ValueTask.CompletedTask;
    }
}
