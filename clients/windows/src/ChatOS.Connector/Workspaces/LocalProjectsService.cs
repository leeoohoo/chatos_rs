using ChatOS.Connector.Runtime;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Workspaces;

public sealed class LocalProjectsService(IProjectRegistry registry, ConnectorRuntimeContext runtime) : ILocalProjectsService
{
    public async Task<string?> GetDeviceIdAsync(string ownerUserId, CancellationToken cancellationToken = default)
    {
        ProjectRegistryValidation.Identifier(ownerUserId, nameof(ownerUserId));
        await runtime.InitializeAsync(cancellationToken).ConfigureAwait(false);
        var state = runtime.Snapshot.State;
        if (state is null || state.User.Id != ownerUserId) return null;
        ProjectRegistryValidation.RouteIdentifier(state.DeviceId, "deviceId");
        return state.DeviceId;
    }

    public async Task<LocalProjectRecord> CreateAsync(
        string ownerUserId, LocalProjectDraft draft, string expectedWorkspaceRoot, CancellationToken cancellationToken = default)
    {
        draft.Validate();
        var state = await RequireStateAsync(ownerUserId, cancellationToken).ConfigureAwait(false);
        var workspace = state.Workspaces.FirstOrDefault(value => value.Id == draft.WorkspaceId);
        var comparison = OperatingSystem.IsWindows() ? StringComparison.OrdinalIgnoreCase : StringComparison.Ordinal;
        // The user confirmed this local root in the creation dialog. A reconfigured workspace
        // must not silently redirect the new project to another directory with the same ID.
        if (workspace is null || !Path.IsPathFullyQualified(expectedWorkspaceRoot) ||
            !string.Equals(Path.TrimEndingDirectorySeparator(Path.GetFullPath(workspace.AbsoluteRoot)),
                Path.TrimEndingDirectorySeparator(Path.GetFullPath(expectedWorkspaceRoot)), comparison))
            throw new InvalidOperationException("The selected workspace directory changed. Please reopen the project dialog.");
        ValidateDirectory(state, draft);
        RequireUnchanged(state);
        return await registry.CreateAsync(ownerUserId, draft, cancellationToken).ConfigureAwait(false);
    }

    public async Task<LocalProjectRecord> RenameAsync(
        string ownerUserId, string projectId, long expectedRevision, string name, CancellationToken cancellationToken = default)
    {
        var record = await RequireActiveAsync(ownerUserId, projectId, cancellationToken).ConfigureAwait(false);
        return await registry.UpdateAsync(ownerUserId, projectId, expectedRevision,
            record.Draft with { Name = name }, LocalProjectStatus.Active, cancellationToken).ConfigureAwait(false);
    }

    public async Task RemoveAsync(
        string ownerUserId, string projectId, long expectedRevision, CancellationToken cancellationToken = default)
    {
        var record = await RequireActiveAsync(ownerUserId, projectId, cancellationToken).ConfigureAwait(false);
        // Tombstone only: never delete the directory, Git repository, conversations or plugin data.
        await registry.UpdateAsync(ownerUserId, projectId, expectedRevision,
            record.Draft, LocalProjectStatus.Removed, cancellationToken).ConfigureAwait(false);
    }

    public async Task<ProjectContextSnapshot> ResolveContextAsync(
        string ownerUserId, string projectId, CancellationToken cancellationToken = default)
    {
        var state = await RequireStateAsync(ownerUserId, cancellationToken).ConfigureAwait(false);
        var record = await RequireActiveAsync(ownerUserId, projectId, cancellationToken).ConfigureAwait(false);
        ValidateDirectory(state, record.Draft);
        var current = await RequireActiveAsync(ownerUserId, projectId, cancellationToken).ConfigureAwait(false);
        if (current.Revision != record.Revision)
            throw new ProjectRegistryException(ProjectRegistryError.RevisionConflict, "Project changed while resolving context.");
        RequireUnchanged(state);
        return ProjectContextSnapshot.FromRecord(record, state.DeviceId);
    }

    private async Task<ConnectorPersistentState> RequireStateAsync(string ownerUserId, CancellationToken cancellationToken)
    {
        if (await GetDeviceIdAsync(ownerUserId, cancellationToken).ConfigureAwait(false) is null)
            throw new InvalidOperationException("The local connector is not configured for the current account.");
        var state = runtime.Snapshot.State;
        if (state?.User.Id != ownerUserId)
            throw new InvalidOperationException("The local connector account changed.");
        return state;
    }

    private void RequireUnchanged(ConnectorPersistentState state)
    {
        if (!ReferenceEquals(runtime.Snapshot.State, state))
            throw new InvalidOperationException("The local connector configuration changed. Please retry.");
    }

    private static void ValidateDirectory(ConnectorPersistentState state, LocalProjectDraft draft)
    {
        var workspace = state.Workspaces.FirstOrDefault(value => value.Id == draft.WorkspaceId)
            ?? throw new InvalidOperationException("The project workspace is not authorized on this device.");
        var path = new WorkspacePathGuard(workspace.AbsoluteRoot).ResolveExisting(
            draft.RelativeRoot.Length == 0 ? "." : draft.RelativeRoot);
        if (!Directory.Exists(path)) throw new InvalidOperationException("The project root must be an existing directory.");
    }

    private async Task<LocalProjectRecord> RequireActiveAsync(string ownerUserId, string projectId, CancellationToken cancellationToken)
    {
        var record = await registry.GetAsync(ownerUserId, projectId, cancellationToken).ConfigureAwait(false);
        if (record is null || record.Status != LocalProjectStatus.Active)
            throw new ProjectRegistryException(ProjectRegistryError.NotFound, "The local project is unavailable.");
        return record;
    }
}
