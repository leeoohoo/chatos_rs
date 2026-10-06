using System.Text.Json;
using ChatOS.Connector.Sandbox;

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
    string? WorkspaceRoot = null,
    IReadOnlySet<string>? PermissionSnapshot = null,
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

internal sealed record PluginProcessLaunchRequest(
    InstalledPluginRecord Record,
    string ComponentKey,
    string ExecutablePath,
    IReadOnlyList<string> Arguments,
    IReadOnlyDictionary<string, string> Environment,
    string InstallationPath,
    string? WorkspaceRoot,
    IReadOnlySet<string> PermissionSnapshot,
    ConnectorSandboxNetworkAccess NetworkAccess)
{
    public static PluginProcessLaunchRequest From(PreparedPluginLaunch launch) => new(
        launch.Record,
        launch.ComponentKey,
        launch.ExecutablePath,
        launch.Arguments,
        launch.Environment,
        launch.InstallationPath,
        launch.WorkspaceRoot,
        launch.PermissionSnapshot ?? new HashSet<string>(StringComparer.Ordinal),
        ConnectorSandboxNetworkAccess.Disabled);

    public static PluginProcessLaunchRequest From(PreparedPluginApplication launch) => new(
        launch.Record,
        launch.Application.ComponentKey,
        launch.ExecutablePath ?? throw new PluginRuntimeException("Plugin application has no executable."),
        launch.Arguments,
        launch.Environment,
        launch.InstallationPath,
        launch.WorkspaceRoot,
        launch.PermissionSnapshot ?? new HashSet<string>(StringComparer.Ordinal),
        ConnectorSandboxNetworkAccess.Loopback);
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
    string? WorkspaceId,
    string? ProjectId = null);

public sealed record LocalPluginApplication(
    string PluginId,
    string ComponentKey,
    string DisplayName,
    string Description,
    string? BrandColor,
    bool RequiresLocalRuntime,
    string? ContextScope,
    string? MissingContext,
    IReadOnlyList<string> BridgeCapabilities,
    string? IconPath = null)
{
    public string Id => $"{PluginId}:{ComponentKey}";
}

public sealed record LocalPluginApplicationLaunch(
    LocalPluginApplication Application,
    Uri Url,
    string ReleaseId,
    string Version,
    string ArtifactSha256);

internal sealed record PreparedPluginApplication(
    LocalPluginApplication Application,
    InstalledPluginRecord Record,
    string ContextKey,
    string InstallationPath,
    string SourcePath,
    string? ExecutablePath,
    IReadOnlyList<string> Arguments,
    IReadOnlyDictionary<string, string> Environment,
    string HealthPath,
    int LaunchTimeoutMilliseconds,
    string? WorkspaceRoot = null,
    IReadOnlySet<string>? PermissionSnapshot = null);
