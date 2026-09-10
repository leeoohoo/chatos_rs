using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Core.State;

public sealed record WorkspaceRelationsSnapshot(
    IReadOnlyList<WorkspaceContact> Contacts, IReadOnlyList<WorkspaceConversation> Conversations);

public sealed record ClientOwnedWorkspaceLoadResult(WorkspaceSnapshot Snapshot, string? RemoteError);

// Construct per authenticated account. Publish LoadLocalAsync before awaiting RefreshAsync.
// Never imports or merges remote project entities and never hides local database failures.
// Callers must discard stale authentication/refresh generations before publishing either result.
public sealed class ClientOwnedWorkspaceLoader
{
    private readonly IProjectRegistry _registry;
    private readonly IWorkspaceRelationsService _remote;
    private readonly string _ownerUserId;

    public ClientOwnedWorkspaceLoader(IProjectRegistry registry, IWorkspaceRelationsService remote, string ownerUserId)
    {
        ProjectRegistryValidation.Identifier(ownerUserId, nameof(ownerUserId));
        _registry = registry;
        _remote = remote;
        _ownerUserId = ownerUserId;
    }

    public Task<WorkspaceSnapshot> LoadLocalAsync(string? deviceId, CancellationToken cancellationToken = default) =>
        ComposeAsync(deviceId, new([], []), cancellationToken);

    public async Task<ClientOwnedWorkspaceLoadResult> RefreshAsync(
        string? deviceId, CancellationToken cancellationToken = default)
    {
        WorkspaceRelationsSnapshot relations;
        string? remoteError = null;
        try { relations = await _remote.FetchWorkspaceRelationsAsync(cancellationToken).ConfigureAwait(false); }
        catch (Exception error) when (error is not OperationCanceledException && !cancellationToken.IsCancellationRequested)
        {
            relations = new([], []);
            remoteError = error.Message;
        }
        // Read after the network await so stale remote responses cannot resurrect deleted projects.
        return new(await ComposeAsync(deviceId, relations, cancellationToken).ConfigureAwait(false), remoteError);
    }

    private async Task<WorkspaceSnapshot> ComposeAsync(
        string? deviceId, WorkspaceRelationsSnapshot relations, CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var records = await _registry.ListAsync(_ownerUserId, cancellationToken: cancellationToken).ConfigureAwait(false);
        var conversations = relations.Conversations.Where(c => !c.IsArchived)
            .OrderByDescending(c => c.UpdatedAt).ThenBy(c => c.Id, StringComparer.Ordinal).ToArray();
        var projects = records.Select(record => new WorkspaceProject(
            record.Id, record.Draft.Name, LocalRootUri(record, deviceId), null,
            conversations.FirstOrDefault(c => c.ProjectId == record.Id)?.Id,
            deviceId is null ? null : ProjectContextSnapshot.FromRecord(record, deviceId))).ToArray();
        return new(projects, relations.Contacts, relations.Conversations);
    }

    public static string? LocalRootUri(LocalProjectRecord record, string? deviceId)
    {
        record.Validate();
        if (deviceId is null) return null;
        foreach (var value in new[] { deviceId, record.Draft.WorkspaceId })
        {
            ProjectRegistryValidation.RouteIdentifier(value, "executionTarget");
        }
        var root = $"local://connector/{Uri.EscapeDataString(deviceId)}/{Uri.EscapeDataString(record.Draft.WorkspaceId)}";
        return record.Draft.RelativeRoot.Length == 0 ? root :
            root + "/" + string.Join("/", record.Draft.RelativeRoot.Split('/').Select(Uri.EscapeDataString));
    }
}
