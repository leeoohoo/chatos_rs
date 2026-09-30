using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentWorkspaceService(
    WindowsLocalAgentConversationClient conversations) : IWorkspaceRelationsService
{
    internal const string MainContactId = "jiguli";
    internal const string ContactResourceKind = "contact";
    internal const string ProjectResourceKind = "project";
    private const uint PageSize = 200;
    private readonly object _ownerGate = new();
    private string? _ownerUserId;

    public static WorkspaceContact MainContact { get; } = new(
        MainContactId,
        MainContactId,
        "叽咕狸",
        "active");

    public void Configure(string ownerUserId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(ownerUserId);
        lock (_ownerGate) _ownerUserId = ownerUserId;
    }

    public void Reset()
    {
        lock (_ownerGate) _ownerUserId = null;
    }

    public async Task<WorkspaceRelationsSnapshot> FetchWorkspaceRelationsAsync(
        CancellationToken cancellationToken = default)
    {
        var ownerUserId = RequireOwner();
        var records = await ListAllAsync(ownerUserId, cancellationToken).ConfigureAwait(false);
        var contactBinding = new WindowsLocalConversationResourceBinding(
            ContactResourceKind,
            MainContactId);
        if (!records.Any(value => value.Resource == contactBinding))
        {
            try
            {
                var created = await conversations.CreateAsync(
                    ownerUserId,
                    NewConversationId(),
                    MainContact.Name,
                    contactBinding,
                    cancellationToken).ConfigureAwait(false);
                records.Add(created.Conversation);
            }
            catch (LocalAgentHostRequestException exception) when (exception.Code == "conflict")
            {
                records = await ListAllAsync(ownerUserId, cancellationToken).ConfigureAwait(false);
                if (!records.Any(value => value.Resource == contactBinding)) throw;
            }
        }

        return new WorkspaceRelationsSnapshot(
            [MainContact],
            records.Select(MapConversation).OfType<WorkspaceConversation>().ToArray());
    }

    internal async Task<List<WindowsLocalConversationRecord>> ListAllAsync(
        string ownerUserId,
        CancellationToken cancellationToken)
    {
        var records = new List<WindowsLocalConversationRecord>();
        long? beforeUpdatedAtUnixMs = null;
        string? beforeConversationId = null;
        do
        {
            var page = await conversations.ListAsync(
                ownerUserId,
                beforeUpdatedAtUnixMs,
                beforeConversationId,
                PageSize,
                cancellationToken).ConfigureAwait(false);
            records.AddRange(page.Conversations);
            beforeUpdatedAtUnixMs = page.NextBeforeUpdatedAtUnixMs;
            beforeConversationId = page.NextBeforeConversationId;
            if (beforeUpdatedAtUnixMs.HasValue != (beforeConversationId is not null))
            {
                throw new InvalidDataException(
                    "Local Agent Host returned an incomplete conversation cursor.");
            }
        } while (beforeUpdatedAtUnixMs is not null);
        return records;
    }

    internal static string NewConversationId() => $"conversation_{Guid.NewGuid():N}";

    private static WorkspaceConversation? MapConversation(WindowsLocalConversationRecord record)
    {
        var resource = record.Resource;
        if (resource is null) return null;
        var isProject = string.Equals(
            resource.Kind,
            ProjectResourceKind,
            StringComparison.Ordinal);
        var isContact = string.Equals(
            resource.Kind,
            ContactResourceKind,
            StringComparison.Ordinal);
        if (!isProject && !isContact) return null;
        return new WorkspaceConversation(
            record.ConversationId,
            record.Title,
            isProject ? resource.ResourceId : null,
            isContact ? resource.ResourceId : MainContact.Id,
            MainContact.AgentId,
            0,
            DateTimeOffset.FromUnixTimeMilliseconds(record.UpdatedAtUnixMs),
            false);
    }

    private string RequireOwner()
    {
        lock (_ownerGate)
        {
            return _ownerUserId ?? throw new InvalidOperationException(
                "Local Agent workspace is not configured for an authenticated account.");
        }
    }
}
