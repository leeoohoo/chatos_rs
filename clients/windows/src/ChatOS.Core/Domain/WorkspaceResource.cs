namespace ChatOS.Core.Domain;

public enum WorkspaceResourceKind
{
    Contact,
    Project,
    LocalConnector,
    LocalTerminal,
    RemoteConnection,
}

public sealed record WorkspaceResource(
    string Id,
    WorkspaceResourceKind Kind,
    string Title,
    string? Subtitle = null,
    string? ConversationId = null,
    string? ContactName = null);
