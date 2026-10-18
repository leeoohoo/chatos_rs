using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentProjectConversationService(
    WindowsLocalAgentConversationClient conversations,
    WindowsLocalAgentWorkspaceService workspace) : IProjectConversationService
{
    private readonly object _ownerGate = new();
    private string? _ownerUserId;

    public void Configure(string ownerUserId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(ownerUserId);
        lock (_ownerGate) _ownerUserId = ownerUserId;
    }

    public void Reset()
    {
        lock (_ownerGate) _ownerUserId = null;
    }

    public async Task<string> EnsureConversationAsync(
        WorkspaceProject project,
        WorkspaceContact contact,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(project);
        ArgumentNullException.ThrowIfNull(contact);
        if (project.ProjectContext is null ||
            !string.Equals(project.ProjectContext.ProjectId, project.Id, StringComparison.Ordinal))
        {
            throw new InvalidOperationException("The client project context is required.");
        }
        var ownerUserId = RequireOwner();
        var binding = new WindowsLocalConversationResourceBinding(
            WindowsLocalAgentWorkspaceService.ProjectResourceKind,
            project.Id);
        var existing = await FindAsync(ownerUserId, binding, cancellationToken).ConfigureAwait(false);
        if (existing is not null) return existing.ConversationId;

        try
        {
            var conversationId = WindowsLocalAgentWorkspaceService.NewConversationId();
            var created = await conversations.CreateAsync(
                ownerUserId,
                conversationId,
                project.Name,
                binding,
                cancellationToken).ConfigureAwait(false);
            return WindowsLocalAgentWorkspaceService.RequireCreatedBinding(
                created.Conversation,
                ownerUserId,
                conversationId,
                binding).ConversationId;
        }
        catch (LocalAgentHostRequestException exception) when (exception.Code == "conflict")
        {
            existing = await FindAsync(ownerUserId, binding, cancellationToken).ConfigureAwait(false);
            if (existing is null) throw;
            return existing.ConversationId;
        }
    }

    private async Task<WindowsLocalConversationRecord?> FindAsync(
        string ownerUserId,
        WindowsLocalConversationResourceBinding binding,
        CancellationToken cancellationToken)
    {
        var matches = (await workspace.ListAllAsync(ownerUserId, cancellationToken)
            .ConfigureAwait(false))
            .Where(value => value.Resource == binding)
            .Take(2)
            .ToArray();
        if (matches.Length > 1)
        {
            throw new InvalidOperationException(
                "The project is bound to more than one Local Agent conversation.");
        }
        return matches.SingleOrDefault();
    }

    private string RequireOwner()
    {
        lock (_ownerGate)
        {
            return _ownerUserId ?? throw new InvalidOperationException(
                "Local Agent project conversations are not configured for an authenticated account.");
        }
    }
}
