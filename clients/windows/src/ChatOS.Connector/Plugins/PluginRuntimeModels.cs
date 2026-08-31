using System.Text.Json;

namespace ChatOS.Connector.Plugins;

public sealed class PluginRuntimeException : Exception
{
    public PluginRuntimeException(string message)
        : base(message)
    {
    }

    public PluginRuntimeException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}

internal sealed record PreparedPluginLaunch(
    InstalledPluginRecord Record,
    string ComponentKey,
    PluginMcpServer Server,
    string ExecutablePath,
    IReadOnlyList<string> Arguments,
    IReadOnlyDictionary<string, string> Environment,
    string InstallationPath,
    string VisualSessionPath,
    string ArtifactPath,
    string DisplayName,
    string Transport = "stdio",
    Uri? HttpEndpoint = null,
    IReadOnlyDictionary<string, PluginCredentialTemplate>? DeclaredHttpHeaderTemplates = null,
    PluginCredentialBinding? CredentialBinding = null,
    PluginOAuthTokenBinding? OAuthBinding = null)
{
    public IReadOnlyDictionary<string, PluginCredentialTemplate> HttpHeaderTemplates { get; } =
        DeclaredHttpHeaderTemplates ??
        new Dictionary<string, PluginCredentialTemplate>(StringComparer.OrdinalIgnoreCase);
}

public sealed record PluginMcpInitialization(
    string? Instructions,
    IReadOnlyList<JsonElement> Tools);

internal sealed record PluginRuntimeIdentity(
    string RunId,
    string PluginId,
    string ReleaseId,
    string Version,
    string ArtifactSha256,
    string ComponentKey,
    string AdapterSessionId,
    string? WorkspaceId);
