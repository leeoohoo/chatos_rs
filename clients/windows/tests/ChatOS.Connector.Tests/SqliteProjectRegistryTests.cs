using System.Text.Json;
using ChatOS.Connector.Persistence;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;

namespace ChatOS.Connector.Tests;

public sealed class SqliteProjectRegistryTests : IAsyncLifetime
{
    private readonly string _directory = Path.Combine(Path.GetTempPath(), "chatos-project-registry-tests", Guid.NewGuid().ToString("N"));
    private readonly LocalProjectDraft _draft = new("项目", "workspace-1", "apps/example");
    private LocalStateDatabase _database = null!;
    private SqliteProjectRegistry _registry = null!;

    public async Task InitializeAsync()
    {
        _database = new LocalStateDatabase(Path.Combine(_directory, "projects.db"));
        await _database.InitializeAsync();
        _registry = new(_database);
    }

    public Task DisposeAsync()
    {
        if (Directory.Exists(_directory)) Directory.Delete(_directory, recursive: true);
        return Task.CompletedTask;
    }

    private LocalProjectRecord Legacy(string id = "legacy-id", string owner = "alice") =>
        new(id, owner, _draft, 1, LocalProjectStatus.Active, 1, 2);

    [Fact]
    public async Task OfflineCreateSurvivesReopenAndIsAccountScoped()
    {
        var project = await _registry.CreateAsync("alice", _draft);
        var reopened = new SqliteProjectRegistry(new LocalStateDatabase(Path.Combine(_directory, "projects.db")));
        Assert.Equal(project, Assert.Single(await reopened.ListAsync("alice")));
        Assert.Empty(await reopened.ListAsync("bob"));
        Assert.Null(await reopened.GetAsync("bob", project.Id));
    }

    [Fact]
    public async Task RenameRebindAndRepairKeepIdentityAndFrozenSnapshot()
    {
        var project = await _registry.CreateAsync("alice", _draft);
        var frozen = ProjectContextSnapshot.FromRecord(project, "old-device");
        var changed = await _registry.UpdateAsync("alice", project.Id, 1, new("renamed", "workspace-2", "moved"), LocalProjectStatus.Active);
        var current = ProjectContextSnapshot.FromRecord(changed, "new-device");
        Assert.Equal(project.Id, changed.Id);
        Assert.Equal(2, changed.Revision);
        Assert.Equal("apps/example", frozen.ExecutionTarget.RelativeRoot);
        Assert.Equal("old-device", frozen.ExecutionTarget.DeviceId);
        Assert.Equal("new-device", current.ExecutionTarget.DeviceId);
        using var json = JsonDocument.Parse(JsonSerializer.Serialize(frozen));
        Assert.Equal(new[] { "executionTarget", "projectId", "projectName", "projectRevision", "schemaVersion" },
            json.RootElement.EnumerateObject().Select(property => property.Name).Order(StringComparer.Ordinal));
    }

    [Fact]
    public async Task RevisionConflictAndArchiveRestore()
    {
        var project = await _registry.CreateAsync("alice", _draft);
        var second = new SqliteProjectRegistry(_database);
        await _registry.UpdateAsync("alice", project.Id, 1, _draft, LocalProjectStatus.Archived);
        var error = await Assert.ThrowsAsync<ProjectRegistryException>(() => second.UpdateAsync("alice", project.Id, 1, _draft, LocalProjectStatus.Active));
        Assert.Equal(ProjectRegistryError.RevisionConflict, error.Code);
        Assert.Empty(await _registry.ListAsync("alice"));
        var restored = await _registry.UpdateAsync("alice", project.Id, 2, _draft, LocalProjectStatus.Active);
        Assert.Equal(3, restored.Revision);
    }

    [Fact]
    public async Task ImportIsIdempotentAndNeverResurrectsTombstones()
    {
        var original = Legacy();
        var imported = await _registry.ImportAsync("alice", "export-1", [original]);
        var replay = await _registry.ImportAsync("alice", "export-1", [original]);
        Assert.Equal(new[] { original.Id }, imported.InsertedIds);
        Assert.Equal(imported.InsertedIds, replay.InsertedIds);
        await _registry.UpdateAsync("alice", original.Id, 1, _draft, LocalProjectStatus.Removed);
        var later = await _registry.ImportAsync("alice", "export-2", [original]);
        Assert.Equal(new[] { original.Id }, later.SkippedIds);
        Assert.Empty(await _registry.ListAsync("alice"));
        Assert.Equal(LocalProjectStatus.Removed, (await _registry.GetAsync("alice", original.Id))!.Status);
        var error = await Assert.ThrowsAsync<ProjectRegistryException>(() => _registry.UpdateAsync("alice", original.Id, 2, _draft, LocalProjectStatus.Active));
        Assert.Equal(ProjectRegistryError.Removed, error.Code);
    }

    [Fact]
    public async Task ImportRejectsWrongOwnerDuplicatesAndChangedReceipt()
    {
        await Assert.ThrowsAsync<ProjectRegistryException>(() => _registry.ImportAsync("alice", "invalid", [Legacy(), Legacy("bad", "bob")]));
        await Assert.ThrowsAsync<ProjectRegistryException>(() => _registry.ImportAsync("alice", "invalid", [Legacy(), Legacy()]));
        Assert.Empty(await _registry.ListAsync("alice", includeInactive: true));
        await _registry.ImportAsync("alice", "export", [Legacy()]);
        var error = await Assert.ThrowsAsync<ProjectRegistryException>(() => _registry.ImportAsync("alice", "export", [Legacy("another")]));
        Assert.Equal(ProjectRegistryError.ImportSourceConflict, error.Code);
        Assert.Null(await _registry.GetAsync("alice", "another"));
    }

    [Theory]
    [InlineData("/absolute")]
    [InlineData("../escape")]
    [InlineData("a/../b")]
    [InlineData("a/./b")]
    [InlineData("a//b")]
    [InlineData("a/")]
    [InlineData("C:/repo")]
    [InlineData("a\\b")]
    [InlineData("a\0b")]
    [InlineData(" leading")]
    [InlineData("trailing ")]
    public async Task InvalidPortablePathsAreRejected(string path)
    {
        await Assert.ThrowsAsync<ProjectRegistryException>(() => _registry.CreateAsync("alice", _draft with { RelativeRoot = path }));
        Assert.Empty(await _registry.ListAsync("alice"));
    }

    [Fact]
    public async Task OfflineRelationsDoNotHideLocalProjects()
    {
        var project = await _registry.CreateAsync("alice", _draft);
        var loader = new ClientOwnedWorkspaceLoader(_registry, new OfflineRelations(), "alice");
        var local = await loader.LoadLocalAsync(null);
        Assert.Equal(project.Id, Assert.Single(local.Projects).Id);
        Assert.Null(local.Projects[0].RootPath);
        var refreshed = await loader.RefreshAsync("device");
        Assert.NotNull(refreshed.RemoteError);
        Assert.Equal("local://connector/device/workspace-1/apps/example", Assert.Single(refreshed.Snapshot.Projects).RootPath);
    }

    [Fact]
    public async Task RelationsCannotCreateProjectsAndChooseLatestActiveConversation()
    {
        var project = await _registry.CreateAsync("alice", _draft);
        var remote = new FixedRelations(new([], [
            Conversation("archived", project.Id, 30, archived: true),
            Conversation("new", project.Id, 20),
            Conversation("old", project.Id, 10),
            Conversation("orphan", "remote-only", 40),
        ]));
        var result = await new ClientOwnedWorkspaceLoader(_registry, remote, "alice").RefreshAsync("device");
        Assert.Null(result.RemoteError);
        Assert.Equal("项目", Assert.Single(result.Snapshot.Projects).Name);
        Assert.Equal("new", result.Snapshot.Projects[0].LatestConversationId);
        Assert.Equal(4, result.Snapshot.Conversations.Count);
        var escaped = ClientOwnedWorkspaceLoader.LocalRootUri(project with { Draft = _draft with { RelativeRoot = "目录/a%20b #x" } }, "device")!;
        Assert.Equal("/device/workspace-1/目录/a%20b #x", Uri.UnescapeDataString(new Uri(escaped).AbsolutePath));
    }

    [Fact]
    public async Task CancelledRefreshIsNotReportedAsOfflineSuccess()
    {
        var loader = new ClientOwnedWorkspaceLoader(_registry, new CancelledRelations(), "alice");
        await Assert.ThrowsAsync<OperationCanceledException>(() => loader.RefreshAsync(null));
    }

    [Fact]
    public async Task DeletionDuringRemoteRefreshCannotResurrectProject()
    {
        var project = await _registry.CreateAsync("alice", _draft);
        var loader = new ClientOwnedWorkspaceLoader(_registry, new DeletingRelations(_registry, project), "alice");
        Assert.Empty((await loader.RefreshAsync("device")).Snapshot.Projects);
    }

    [Fact]
    public async Task ImportRollsBackBothRecordsAndReceiptOnDatabaseFailure()
    {
        await using (var connection = await _database.OpenConnectionAsync())
        {
            using var command = connection.CreateCommand();
            command.CommandText = """
                CREATE TRIGGER fail_import BEFORE INSERT ON local_project_records
                WHEN NEW.id = 'b' BEGIN SELECT RAISE(ABORT, 'injected failure'); END;
                """;
            await command.ExecuteNonQueryAsync();
        }
        await Assert.ThrowsAsync<Microsoft.Data.Sqlite.SqliteException>(() => _registry.ImportAsync("alice", "export", [Legacy("a"), Legacy("b")]));
        Assert.Empty(await _registry.ListAsync("alice", includeInactive: true));
        // Reusing the same source with a different payload succeeds only if its receipt rolled back.
        var result = await _registry.ImportAsync("alice", "export", [Legacy("a")]);
        Assert.Equal(new[] { "a" }, result.InsertedIds);
    }

    private sealed class OfflineRelations : IWorkspaceRelationsService
    {
        public Task<WorkspaceRelationsSnapshot> FetchWorkspaceRelationsAsync(CancellationToken cancellationToken = default) =>
            throw new HttpRequestException("offline");
    }

    private static WorkspaceConversation Conversation(string id, string project, int time, bool archived = false) =>
        new(id, id, project, null, null, 1, DateTimeOffset.FromUnixTimeSeconds(time), archived);

    private sealed class FixedRelations(WorkspaceRelationsSnapshot snapshot) : IWorkspaceRelationsService
    {
        public Task<WorkspaceRelationsSnapshot> FetchWorkspaceRelationsAsync(CancellationToken cancellationToken = default) =>
            Task.FromResult(snapshot);
    }

    private sealed class CancelledRelations : IWorkspaceRelationsService
    {
        public Task<WorkspaceRelationsSnapshot> FetchWorkspaceRelationsAsync(CancellationToken cancellationToken = default) =>
            throw new OperationCanceledException();
    }

    private sealed class DeletingRelations(IProjectRegistry registry, LocalProjectRecord project) : IWorkspaceRelationsService
    {
        public async Task<WorkspaceRelationsSnapshot> FetchWorkspaceRelationsAsync(CancellationToken cancellationToken = default)
        {
            await registry.UpdateAsync(project.OwnerUserId, project.Id, project.Revision, project.Draft,
                LocalProjectStatus.Removed, cancellationToken);
            return new([], []);
        }
    }
}
