using System.Text.Json;
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

    [JsonPropertyName("description")]
    public string Description { get; init; } = string.Empty;

    [JsonPropertyName("skills")]
    public IReadOnlyList<PluginPathReference> Skills { get; init; } = Array.Empty<PluginPathReference>();

    [JsonPropertyName("mcpServers")]
    public IReadOnlyDictionary<string, PluginMcpServer> McpServers { get; init; } =
        new Dictionary<string, PluginMcpServer>();

    [JsonPropertyName("permissions")]
    public IReadOnlyList<PluginPermission> Permissions { get; init; } = Array.Empty<PluginPermission>();

    [JsonPropertyName("apps")]
    public IReadOnlyList<PluginConnectedApp> Apps { get; init; } = Array.Empty<PluginConnectedApp>();

    [JsonPropertyName("ui")]
    public IReadOnlyList<PluginUiContribution> Ui { get; init; } = Array.Empty<PluginUiContribution>();

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

[JsonConverter(typeof(PluginPathReferenceConverter))]
internal sealed record PluginPathReference
{
    [JsonPropertyName("path")]
    public string? Path { get; init; }
}

internal sealed class PluginPathReferenceConverter : JsonConverter<PluginPathReference>
{
    public override PluginPathReference Read(
        ref Utf8JsonReader reader,
        Type typeToConvert,
        JsonSerializerOptions options)
    {
        if (reader.TokenType == JsonTokenType.String)
        {
            return new PluginPathReference { Path = reader.GetString() };
        }
        if (reader.TokenType != JsonTokenType.StartObject)
        {
            throw new JsonException("Plugin path reference must be a string or object.");
        }
        using var document = JsonDocument.ParseValue(ref reader);
        return new PluginPathReference
        {
            Path = document.RootElement.TryGetProperty("path", out var path) &&
                path.ValueKind == JsonValueKind.String
                    ? path.GetString()
                    : null,
        };
    }

    public override void Write(
        Utf8JsonWriter writer,
        PluginPathReference value,
        JsonSerializerOptions options) => writer.WriteStringValue(value.Path);
}

internal sealed record PluginUiContribution
{
    [JsonPropertyName("componentKey")]
    public required string ComponentKey { get; init; }

    [JsonPropertyName("source")]
    public required PluginPathReference Source { get; init; }

    [JsonPropertyName("title")]
    public string? Title { get; init; }

    [JsonPropertyName("surface")]
    public string? Surface { get; init; }

    [JsonPropertyName("assets")]
    public IReadOnlyList<string> Assets { get; init; } = Array.Empty<string>();

    [JsonPropertyName("bridgeCapabilities")]
    public IReadOnlyList<string> BridgeCapabilities { get; init; } = Array.Empty<string>();

    [JsonPropertyName("runtime")]
    public PluginUiRuntime? Runtime { get; init; }
}

internal sealed record PluginUiRuntime
{
    [JsonPropertyName("type")]
    public required string Type { get; init; }

    [JsonPropertyName("bin")]
    public required string Bin { get; init; }

    [JsonPropertyName("args")]
    public IReadOnlyList<string> Arguments { get; init; } = Array.Empty<string>();

    [JsonPropertyName("healthPath")]
    public string? HealthPath { get; init; }

    [JsonPropertyName("launchTimeoutMs")]
    public int? LaunchTimeoutMilliseconds { get; init; }
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

    [JsonPropertyName("shortDescription")]
    public string? ShortDescription { get; init; }

    [JsonPropertyName("longDescription")]
    public string? LongDescription { get; init; }

    [JsonPropertyName("brandColor")]
    public string? BrandColor { get; init; }

    [JsonPropertyName("logo")]
    public PluginPathReference? Logo { get; init; }

    [JsonPropertyName("logoDark")]
    public PluginPathReference? LogoDark { get; init; }
}
