namespace ChatOS.Core.Domain;

public sealed record LocalConnectorWorkspaceStatus(
    string Id,
    string Alias,
    string AbsoluteRoot);

public sealed record LocalConnectorStatus(
    bool IsPaired,
    string ConnectionPhase,
    string? Username,
    string? DeviceId,
    string? DeviceName,
    string? GatewayBaseUrl,
    DateTimeOffset? ConnectedAt,
    DateTimeOffset? LastPongAt,
    string? LastError,
    IReadOnlyList<LocalConnectorWorkspaceStatus> Workspaces);

public sealed record LocalConnectorWorkspaceDraft(
    string AbsoluteRoot,
    string? Alias = null);

public sealed record LocalConnectorPairingDraft(
    string GatewayBaseUrl,
    string DeviceName,
    IReadOnlyList<LocalConnectorWorkspaceDraft> Workspaces);
