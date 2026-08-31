using ChatOS.Core.Domain;

namespace ChatOS.Desktop.AppShell;

public sealed record ShellResourceViewModel(
    string Id,
    WorkspaceResourceKind Kind,
    string Title,
    string Subtitle,
    string Glyph,
    string? ConversationId = null,
    string? WorkspaceId = null,
    string? AbsoluteRoot = null);
