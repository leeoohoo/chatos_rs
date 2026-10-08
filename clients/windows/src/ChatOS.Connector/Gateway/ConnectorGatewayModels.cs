using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.Relay;

namespace ChatOS.Connector.Gateway;

public sealed record ConnectorGatewayLogin(
    string Token,
    ConnectorGatewayUser User);

public sealed record ConnectorGatewayUser(
    string Id,
    string Username,
    string? DisplayName,
    string Role);

public sealed record ConnectorGatewayDevice(
    string Id,
    string? OwnerUserId,
    string DisplayName,
    string PublicKey,
    string Status);

public sealed record ConnectorGatewayWorkspace(
    string Id,
    string DeviceId,
    string LocalPathAlias,
    string LocalPathFingerprint);

public sealed record ConnectorControlledNetworkReadiness(
    bool Available,
    string State,
    string? PermissionProfile,
    int AllowedHostCount);

public sealed record NativeAgentRuntimeSettings(
    int MaximumModelCalls,
    int MaximumRequestRetries,
    int RequestTimeoutSeconds,
    int RunTimeoutSeconds,
    int MaximumNoProgressRounds,
    int ContextWindowTokens,
    int OutputReserveTokens)
{
    public static NativeAgentRuntimeSettings Default { get; } = new(
        600, 2, 180, 7_200, 8, 2_000_000, 30_000);

    public void Validate()
    {
        if (MaximumModelCalls is < 1 or > 10_000 ||
            MaximumRequestRetries is < 0 or > 10 ||
            RequestTimeoutSeconds is < 5 or > 1_800 ||
            RunTimeoutSeconds is < 10 or > 86_400 ||
            MaximumNoProgressRounds is < 1 or > 100 ||
            ContextWindowTokens is < 2_048 or > 2_000_000 ||
            OutputReserveTokens < 256 || OutputReserveTokens >= ContextWindowTokens)
            throw new InvalidDataException("Managed Agent runtime settings are invalid.");
    }
}

public sealed record ConnectorManagedRuntimeConfig(
    NativeAgentRuntimeSettings NativeAgentRuntimeSettings,
    LocalTaskExecutionSettings LocalTaskExecutionSettings,
    RemoteControlTrust RemoteControlTrust);

public sealed record LocalTaskExecutionSettings(int MaxIterations)
{
    public static LocalTaskExecutionSettings Default { get; } = new(600);

    public void Validate()
    {
        if (MaxIterations is < 2 or > 10_000)
            throw new InvalidDataException("Managed Local Agent task execution settings are invalid.");
    }
}

public sealed record ConnectorPluginSource(
    ConnectorPluginCatalog Catalog,
    ConnectorPluginRelease Release,
    ConnectorPluginPreference? Preference);

public sealed record ConnectorPluginCatalog(
    string Id,
    string? PluginKey,
    string? DisplayName,
    string? Name,
    string? Description,
    string? PublisherName,
    string? Category,
    string? DeveloperName);

public sealed record ConnectorPluginRelease(
    string Id,
    string? Version,
    string? ArtifactSha256,
    ConnectorPluginNpmPackage? NpmPackage,
    IReadOnlyList<string> SupportedPlatforms);

public sealed record ConnectorPluginNpmPackage(
    string Name,
    string Version,
    string Integrity);

public sealed record ConnectorPluginPreference(bool Enabled);

public sealed record ConnectorGatewayModelConfig(
    string Id,
    string Name,
    string Provider,
    string? PromptVendor,
    string Model,
    string? BaseUrl,
    string? ApiKey,
    bool Enabled,
    bool TaskEnabled,
    bool SupportsResponses,
    string? ThinkingLevel,
    double? Temperature,
    int? MaxOutputTokens);

public sealed record ConnectorAgentPromptBundle(
    long BundleVersion,
    DateTimeOffset UpdatedAt,
    IReadOnlyList<ConnectorAgentPrompt> Prompts);

public sealed record ConnectorAgentPrompt(
    string AgentKey,
    string Vendor,
    string Content,
    long Revision,
    string Checksum,
    DateTimeOffset PublishedAt);

public sealed record ConnectorAgentCapability(
    string AgentKey,
    string OwnerUserId,
    string PolicyRevision,
    bool AgentEnabled,
    IReadOnlyList<ConnectorResolvedMcp>? Mcps = null,
    IReadOnlyList<ConnectorResolvedPlugin>? Plugins = null);

public sealed record ConnectorCapabilityBinding(bool Enabled, bool Required);

public sealed record ConnectorMcpRuntime(
    string Kind,
    string? BuiltinKind,
    string? ServerName,
    string? Url,
    IReadOnlyDictionary<string, string> Headers);

public sealed record ConnectorMcpResource(
    string Id,
    string Name,
    string DisplayName,
    string? Description,
    bool Enabled,
    ConnectorMcpRuntime Runtime);

public sealed record ConnectorResolvedMcp(
    ConnectorMcpResource Resource,
    ConnectorCapabilityBinding Binding,
    bool Available,
    string Status,
    IReadOnlyList<JsonElement> ToolSnapshot);

public sealed record ConnectorPluginCapabilityCatalog(
    string Id,
    string PluginKey,
    string DisplayName,
    string Description);

public sealed record ConnectorResolvedPlugin(
    ConnectorPluginCapabilityCatalog Catalog,
    ConnectorCapabilityBinding Binding,
    bool Available,
    string Status);

public interface IConnectorGatewayClient
{
    Task<ConnectorGatewayLogin> ExchangeTicketAsync(
        Uri gatewayBaseUri,
        string ticket,
        string deviceName,
        CancellationToken cancellationToken = default);

    Task<ConnectorGatewayDevice> CreateDeviceAsync(
        Uri gatewayBaseUri,
        string token,
        string displayName,
        string publicKey,
        CancellationToken cancellationToken = default);

    Task<ConnectorGatewayDevice?> GetDeviceAsync(
        Uri gatewayBaseUri,
        string token,
        string deviceId,
        CancellationToken cancellationToken = default);

    Task DisconnectDeviceAsync(
        Uri gatewayBaseUri,
        string token,
        string deviceId,
        CancellationToken cancellationToken = default);

    Task<ConnectorControlledNetworkReadiness> GetControlledNetworkReadinessAsync(
        Uri gatewayBaseUri,
        string token,
        string deviceId,
        CancellationToken cancellationToken = default) =>
        throw new NotSupportedException("Controlled network readiness is not supported by this gateway client.");

    Task<IReadOnlyList<ConnectorGatewayWorkspace>> ListWorkspacesAsync(
        Uri gatewayBaseUri,
        string token,
        CancellationToken cancellationToken = default);

    Task<ConnectorGatewayWorkspace> CreateWorkspaceAsync(
        Uri gatewayBaseUri,
        string token,
        string deviceId,
        string alias,
        string fingerprint,
        CancellationToken cancellationToken = default);

    Task<ConnectorGatewayWorkspace> MoveWorkspaceAsync(
        Uri gatewayBaseUri,
        string token,
        string workspaceId,
        string deviceId,
        CancellationToken cancellationToken = default);

    Task<RemoteControlTrust> GetRemoteControlTrustAsync(
        Uri gatewayBaseUri,
        string token,
        CancellationToken cancellationToken = default);

    async Task<ConnectorManagedRuntimeConfig> GetManagedRuntimeConfigAsync(
        Uri gatewayBaseUri,
        string token,
        CancellationToken cancellationToken = default) => new(
            NativeAgentRuntimeSettings.Default,
            LocalTaskExecutionSettings.Default,
            await GetRemoteControlTrustAsync(
                gatewayBaseUri, token, cancellationToken).ConfigureAwait(false));

    Task<IReadOnlyList<ConnectorPluginSource>> ListPluginSourcesAsync(
        Uri gatewayBaseUri,
        string token,
        CancellationToken cancellationToken = default) =>
        throw new NotSupportedException("Plugin catalog is not supported by this gateway client.");

    Task UpdatePluginPreferenceAsync(
        Uri gatewayBaseUri,
        string token,
        string pluginId,
        string deviceId,
        bool enabled,
        CancellationToken cancellationToken = default) =>
        throw new NotSupportedException("Plugin preferences are not supported by this gateway client.");

    Task DownloadPluginArtifactAsync(
        Uri gatewayBaseUri,
        string token,
        string pluginId,
        string releaseId,
        Stream destination,
        CancellationToken cancellationToken = default) =>
        throw new NotSupportedException("Plugin artifacts are not supported by this gateway client.");

    Task<IReadOnlyList<ConnectorGatewayModelConfig>> ListModelConfigsAsync(
        Uri gatewayBaseUri,
        string token,
        CancellationToken cancellationToken = default) =>
        throw new NotSupportedException("Model configuration is not supported by this gateway client.");

    Task<ConnectorGatewayModelConfig> GetModelConfigAsync(
        Uri gatewayBaseUri,
        string token,
        string modelConfigId,
        bool includeSecret,
        CancellationToken cancellationToken = default) =>
        throw new NotSupportedException("Model configuration is not supported by this gateway client.");

    Task<ConnectorAgentPromptBundle> GetAgentPromptBundleAsync(
        Uri gatewayBaseUri,
        string token,
        CancellationToken cancellationToken = default) =>
        throw new NotSupportedException("Agent Prompt bundles are not supported by this gateway client.");

    Task<ConnectorAgentCapability> GetAgentCapabilityAsync(
        Uri gatewayBaseUri,
        string token,
        string agentKey,
        CancellationToken cancellationToken = default) =>
        throw new NotSupportedException("Agent capabilities are not supported by this gateway client.");
}

internal sealed record GatewayTicketExchangeRequest(
    [property: JsonPropertyName("ticket")] string Ticket,
    [property: JsonPropertyName("device_name")] string DeviceName,
    [property: JsonPropertyName("client_version")] string ClientVersion);

internal sealed record GatewayCreateDeviceRequest(
    [property: JsonPropertyName("display_name")] string DisplayName,
    [property: JsonPropertyName("public_key")] string PublicKey,
    [property: JsonPropertyName("client_version")] string ClientVersion,
    [property: JsonPropertyName("os")] string OperatingSystem);

internal sealed record GatewayCreateWorkspaceRequest(
    [property: JsonPropertyName("device_id")] string DeviceId,
    [property: JsonPropertyName("display_name")] string DisplayName,
    [property: JsonPropertyName("local_path_alias")] string LocalPathAlias,
    [property: JsonPropertyName("local_path_fingerprint")] string LocalPathFingerprint,
    [property: JsonPropertyName("capabilities")] IReadOnlyList<string> Capabilities);

internal sealed record GatewayMoveWorkspaceRequest(
    [property: JsonPropertyName("device_id")] string DeviceId,
    [property: JsonPropertyName("status")] string Status);

internal sealed record GatewayPluginPreferenceRequest(
    [property: JsonPropertyName("device_id")] string DeviceId,
    [property: JsonPropertyName("enabled")] bool Enabled);
