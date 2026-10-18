using ChatOS.Connector.Relay;

namespace ChatOS.Connector.Workspaces;

public sealed class WorkspaceRequestException(
    int statusCode,
    string code,
    string message) : RelayRequestException(statusCode, message)
{
    public string Code { get; } = code;
}
