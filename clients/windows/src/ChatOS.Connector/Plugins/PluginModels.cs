using System.Text.Json.Serialization;

namespace ChatOS.Connector.Plugins;

public sealed record InstalledPluginRecord(
    string PluginId,
    string ReleaseId,
    string Version,
    string ArtifactSha256,
    string InstallationPath,
    DateTimeOffset InstalledAt,
    IReadOnlyList<string> DeclaredPermissions,
    IReadOnlyDictionary<string, string>? PackageFileSha256 = null);

public interface IInstalledPluginStore
{
    Task<IReadOnlyList<InstalledPluginRecord>> ListAsync(
        CancellationToken cancellationToken = default);

    Task<InstalledPluginRecord?> GetAsync(
        string pluginId,
        CancellationToken cancellationToken = default);

    Task SaveAsync(
        InstalledPluginRecord record,
        CancellationToken cancellationToken = default);

    Task DeleteAsync(
        string pluginId,
        CancellationToken cancellationToken = default);
}

public sealed class PluginPackageException : IOException
{
    public PluginPackageException(string message)
        : base(message)
    {
    }

    public PluginPackageException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}

internal sealed record PluginManifest
{
    [JsonPropertyName("schemaVersion")]
    public required int SchemaVersion { get; init; }

    [JsonPropertyName("name")]
    public required string Name { get; init; }

    [JsonPropertyName("version")]
    public required string Version { get; init; }

    [JsonPropertyName("skills")]
    public IReadOnlyList<PluginPathReference> Skills { get; init; } = Array.Empty<PluginPathReference>();

    [JsonPropertyName("mcpServers")]
    public IReadOnlyDictionary<string, PluginMcpServer> McpServers { get; init; } =
        new Dictionary<string, PluginMcpServer>();

    [JsonPropertyName("permissions")]
    public IReadOnlyList<PluginPermission> Permissions { get; init; } = Array.Empty<PluginPermission>();

    [JsonPropertyName("apps")]
    public IReadOnlyList<PluginConnectedApp> Apps { get; init; } = Array.Empty<PluginConnectedApp>();

    [JsonPropertyName("dependencies")]
    public PluginDependencies Dependencies { get; init; } = new();

    [JsonPropertyName("interface")]
    public PluginInterface? Interface { get; init; }

    [JsonPropertyName("runtimeContext")]
    public PluginRuntimeContext? RuntimeContext { get; init; }
}

internal sealed record PluginRuntimeContext
{
    [JsonPropertyName("scope")]
    public string Scope { get; init; } = "device";

    [JsonPropertyName("components")]
    public IReadOnlyList<string> Components { get; init; } = Array.Empty<string>();

    [JsonPropertyName("required")]
    public IReadOnlyList<string> Required { get; init; } = Array.Empty<string>();

    [JsonPropertyName("optional")]
    public IReadOnlyList<string> Optional { get; init; } = Array.Empty<string>();

    [JsonPropertyName("storageIsolation")]
    public string StorageIsolation { get; init; } = "plugin";

    [JsonPropertyName("missingContext")]
    public string MissingContext { get; init; } = "reject";

    public bool AppliesTo(string componentKey) => Components.Contains(componentKey, StringComparer.Ordinal);
}

internal sealed record PluginPathReference
{
    [JsonPropertyName("path")]
    public string? Path { get; init; }
}

internal sealed record PluginMcpServer
{
    [JsonPropertyName("type")]
    public string? Type { get; init; }

    [JsonPropertyName("transport")]
    public string? Transport { get; init; }

    [JsonPropertyName("bin")]
    public string? Bin { get; init; }

    [JsonPropertyName("url")]
    public string? Url { get; init; }

    [JsonPropertyName("args")]
    public IReadOnlyList<string> Arguments { get; init; } = Array.Empty<string>();

    [JsonPropertyName("env")]
    public IReadOnlyDictionary<string, string> Environment { get; init; } =
        new Dictionary<string, string>();

    [JsonPropertyName("headers")]
    public IReadOnlyDictionary<string, string> Headers { get; init; } =
        new Dictionary<string, string>();

    [JsonPropertyName("oauthResource")]
    public string? OAuthResource { get; init; }

    [JsonPropertyName("connectTimeoutMs")]
    public int? ConnectTimeoutMilliseconds { get; init; }

    [JsonPropertyName("requiresExclusiveExecution")]
    public bool RequiresExclusiveExecution { get; init; }

    [JsonIgnore]
    public string EffectiveTransport =>
        (Transport ?? Type)?.Trim().ToLowerInvariant() ??
        (Bin is not null && Url is null ? "stdio" : Url is not null && Bin is null ? "http" : string.Empty);
}

internal sealed record PluginPermission
{
    [JsonPropertyName("permission")]
    public required string Permission { get; init; }

    [JsonPropertyName("required")]
    public bool Required { get; init; }

    [JsonPropertyName("reason")]
    public string? Reason { get; init; }

    [JsonPropertyName("components")]
    public IReadOnlyList<string> Components { get; init; } = Array.Empty<string>();
}

internal sealed record PluginDependencies
{
    [JsonPropertyName("supportedPlatforms")]
    public IReadOnlyList<string> SupportedPlatforms { get; init; } = Array.Empty<string>();
}

internal sealed record PluginConnectedApp
{
    [JsonPropertyName("component_key")]
    public string? ComponentKeySnake { get; init; }

    [JsonPropertyName("componentKey")]
    public string? ComponentKeyCamel { get; init; }

    [JsonPropertyName("manifest")]
    public required PluginPathReference Manifest { get; init; }

    [JsonIgnore]
    public string ComponentKey => ComponentKeySnake ?? ComponentKeyCamel ?? string.Empty;
}

internal sealed record PluginInterface
{
    [JsonPropertyName("displayName")]
    public string? DisplayName { get; init; }
}
