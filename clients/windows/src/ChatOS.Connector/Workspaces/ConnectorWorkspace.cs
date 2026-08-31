namespace ChatOS.Connector.Workspaces;

public sealed record ConnectorWorkspace(
    string Id,
    string Alias,
    string AbsoluteRoot,
    string Fingerprint,
    bool? ProjectConfigTrusted = null,
    bool? ProjectConfigTrustStale = null);

public interface IConnectorWorkspaceCatalog
{
    ConnectorWorkspace? Find(string workspaceId);
}

public interface IConnectorWorkspaceContext : IConnectorWorkspaceCatalog
{
    string? DeviceId { get; }

    IReadOnlyList<ConnectorWorkspace> Workspaces { get; }
}
