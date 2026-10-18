using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Core.State;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentWorkspaceService(
    WindowsLocalAgentConversationClient conversations) : IWorkspaceRelationsService
{
    internal const string MainContactId = "jiguli";
    internal const string PluginInstallerContactId = "plugin-installer";
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

    public static WorkspaceContact PluginInstallerContact { get; } = new(
        PluginInstallerContactId,
        PluginInstallerContactId,
        "插件安装助手",
        "active");

    private static IReadOnlyList<WorkspaceContact> SystemContacts { get; } =
        [MainContact, PluginInstallerContact];

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
        foreach (var contact in SystemContacts)
        {
            var contactBinding = new WindowsLocalConversationResourceBinding(
                ContactResourceKind,
                contact.Id);
            if (records.Any(value => value.Resource == contactBinding)) continue;
            try
            {
                var conversationId = NewConversationId();
                var created = await conversations.CreateAsync(
                    ownerUserId,
                    conversationId,
                    contact.Name,
                    contactBinding,
                    cancellationToken).ConfigureAwait(false);
                records.Add(RequireCreatedBinding(
                    created.Conversation,
                    ownerUserId,
                    conversationId,
                    contactBinding));
            }
            catch (LocalAgentHostRequestException exception) when (exception.Code == "conflict")
            {
                records = await ListAllAsync(ownerUserId, cancellationToken).ConfigureAwait(false);
                if (!records.Any(value => value.Resource == contactBinding)) throw;
            }
        }

        return new WorkspaceRelationsSnapshot(
            SystemContacts,
            records.Select(MapConversation).OfType<WorkspaceConversation>().ToArray());
    }

    internal async Task<List<WindowsLocalConversationRecord>> ListAllAsync(
        string ownerUserId,
        CancellationToken cancellationToken)
    {
        var records = new List<WindowsLocalConversationRecord>();
        var conversationIds = new HashSet<string>(StringComparer.Ordinal);
        var cursors = new HashSet<(long UpdatedAtUnixMs, string ConversationId)>();
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
            foreach (var record in page.Conversations)
            {
                if (!string.Equals(record.OwnerUserId, ownerUserId, StringComparison.Ordinal))
                {
                    throw new InvalidDataException(
                        "Local Agent Host returned a conversation for a different owner.");
                }
                if (string.IsNullOrWhiteSpace(record.ConversationId) ||
                    !conversationIds.Add(record.ConversationId))
                {
                    throw new InvalidDataException(
                        "Local Agent Host returned a missing or duplicate conversation identity.");
                }
                records.Add(record);
            }
            beforeUpdatedAtUnixMs = page.NextBeforeUpdatedAtUnixMs;
            beforeConversationId = page.NextBeforeConversationId;
            if (beforeUpdatedAtUnixMs.HasValue != (beforeConversationId is not null))
            {
                throw new InvalidDataException(
                    "Local Agent Host returned an incomplete conversation cursor.");
            }
            if (beforeUpdatedAtUnixMs is long updatedAtUnixMs &&
                !cursors.Add((updatedAtUnixMs, beforeConversationId!)))
            {
                throw new InvalidDataException(
                    "Local Agent Host returned a repeated conversation cursor.");
            }
        } while (beforeUpdatedAtUnixMs is not null);
        return records;
    }

    internal static string NewConversationId() => $"conversation_{Guid.NewGuid():N}";

    internal static WindowsLocalConversationRecord RequireCreatedBinding(
        WindowsLocalConversationRecord conversation,
        string ownerUserId,
        string conversationId,
        WindowsLocalConversationResourceBinding binding)
    {
        if (!string.Equals(conversation.OwnerUserId, ownerUserId, StringComparison.Ordinal) ||
            !string.Equals(conversation.ConversationId, conversationId, StringComparison.Ordinal) ||
            conversation.Resource != binding)
        {
            throw new InvalidDataException(
                "Local Agent Host returned an invalid conversation binding.");
        }
        return conversation;
    }

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
            isContact ? resource.ResourceId : MainContact.AgentId,
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
