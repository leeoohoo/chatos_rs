using System.Buffers;
using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace ChatOS.Connector.LocalAgent;

public sealed record WindowsLocalAgentPostgresCredential(
    [property: JsonPropertyName("host")] string Host,
    [property: JsonPropertyName("port")] ushort Port,
    [property: JsonPropertyName("database")] string Database,
    [property: JsonPropertyName("tls_mode")] string TlsMode,
    [property: JsonPropertyName("username")] string Username,
    [property: JsonPropertyName("password")] string Password)
{
    public override string ToString() =>
        $"WindowsLocalAgentPostgresCredential {{ Host = [REDACTED], Port = {Port}, Database = [REDACTED], TlsMode = {TlsMode}, Username = [REDACTED], Password = [REDACTED] }}";
}

public abstract record WindowsLocalAgentStorageBootstrap;

public sealed record WindowsLocalAgentSqliteBootstrap(
    string DatabasePath,
    string EncryptionSecretReference) : WindowsLocalAgentStorageBootstrap;

public sealed record WindowsLocalAgentPostgresBootstrap(
    string ConnectionSecretReference) : WindowsLocalAgentStorageBootstrap;

public sealed record WindowsLocalAgentHostBootstrapSettings
{
    public required string ExecutablePath { get; init; }
    public required string ExpectedExecutableSha256 { get; init; }
    public required string AccountId { get; init; }
    public required string DeviceId { get; init; }
    public required string AttachmentGrantDirectory { get; init; }
    public required string PlatformStateDirectory { get; init; }
    public required Uri ModelGatewayBaseUri { get; init; }
    public required Uri MemoryEngineBaseUri { get; init; }
    public string MemorySourceId { get; init; } = "local-agent";
    public required WindowsLocalAgentStorageBootstrap Storage { get; init; }
}

public sealed class WindowsLocalAgentHostBootstrapBuilder
{
    public const string ModelAccessTokenReference = "model-access-token";
    public const string ProviderContextKeyReference = "provider-context-key";

    public Task<WindowsLocalAgentHostLaunchConfiguration> BuildAsync(
        WindowsLocalAgentHostBootstrapSettings settings,
        IReadOnlyDictionary<string, ReadOnlyMemory<byte>> credentialValues,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(settings);
        ArgumentNullException.ThrowIfNull(credentialValues);
        cancellationToken.ThrowIfCancellationRequested();
        Validate(settings);
        EnsurePrivateDirectory(settings.AttachmentGrantDirectory);
        EnsurePrivateDirectory(settings.PlatformStateDirectory);
        var launchId = $"launch-{Guid.NewGuid():N}";
        var workerId = $"worker-{Guid.NewGuid():N}";
        var pipeName = $"chatos-local-agent-{Guid.NewGuid():N}";

        Dictionary<string, object> storageProfile;
        switch (settings.Storage)
        {
            case WindowsLocalAgentSqliteBootstrap sqlite:
                ValidateIdentity(sqlite.EncryptionSecretReference, nameof(sqlite.EncryptionSecretReference));
                if (!Path.IsPathFullyQualified(sqlite.DatabasePath))
                {
                    throw new ArgumentException("SQLite database path must be absolute.");
                }
                storageProfile = new Dictionary<string, object>
                {
                    ["backend"] = "sqlite",
                    ["database_path"] = sqlite.DatabasePath,
                    ["encryption_secret"] = sqlite.EncryptionSecretReference,
                };
                break;
            case WindowsLocalAgentPostgresBootstrap postgres:
                ValidateIdentity(postgres.ConnectionSecretReference, nameof(postgres.ConnectionSecretReference));
                storageProfile = new Dictionary<string, object>
                {
                    ["backend"] = "postgres",
                    ["connection_secret"] = postgres.ConnectionSecretReference,
                };
                break;
            default:
                throw new ArgumentException("Local Agent storage profile is invalid.");
        }

        var request = new Dictionary<string, object>
        {
            ["protocol_version"] = LocalAgentHostLaunchProtocol.Version,
            ["launch_id"] = launchId,
            ["owner_user_id"] = settings.AccountId,
            ["device_id"] = settings.DeviceId,
            ["worker_id"] = workerId,
            ["ipc_endpoint"] = new Dictionary<string, object>
            {
                ["transport"] = "windows_named_pipe",
                ["pipe_name"] = $@"\\.\pipe\{pipeName}",
            },
            ["attachment_grant_directory"] = Path.GetFullPath(settings.AttachmentGrantDirectory),
            ["platform_state_directory"] = Path.GetFullPath(settings.PlatformStateDirectory),
            ["model_gateway_base_url"] = settings.ModelGatewayBaseUri.AbsoluteUri.TrimEnd('/'),
            ["memory_engine_base_url"] = settings.MemoryEngineBaseUri.AbsoluteUri.TrimEnd('/'),
            ["memory_source_id"] = settings.MemorySourceId,
            ["storage_profile"] = storageProfile,
            ["credential_references"] = new Dictionary<string, object>
            {
                ["model_access_token_reference"] = ModelAccessTokenReference,
                ["provider_context_key_reference"] = ProviderContextKeyReference,
            },
        };
        var storageSecretReference = settings.Storage switch
        {
            WindowsLocalAgentSqliteBootstrap sqlite => sqlite.EncryptionSecretReference,
            WindowsLocalAgentPostgresBootstrap postgres => postgres.ConnectionSecretReference,
            _ => throw new ArgumentException("Local Agent storage profile is invalid."),
        };
        var requiredReferences = new HashSet<string>(StringComparer.Ordinal)
        {
            ModelAccessTokenReference,
            ProviderContextKeyReference,
            storageSecretReference,
        };
        if (!requiredReferences.SetEquals(credentialValues.Keys)
            || credentialValues.Values.Any(value => value.IsEmpty || value.Length > 64 * 1024))
        {
            throw new ArgumentException("Local Agent Host credential values are invalid.");
        }

        var requestJson = JsonSerializer.SerializeToUtf8Bytes(request);
        var secretJson = EncodeSecretFrame(launchId, credentialValues);
        var configuration = new WindowsLocalAgentHostLaunchConfiguration
        {
            ExecutablePath = settings.ExecutablePath,
            ExpectedExecutableSha256 = settings.ExpectedExecutableSha256,
            LaunchId = launchId,
            ExpectedClientEndpoint = pipeName,
            LaunchMaterial = new WindowsLocalAgentHostLaunchMaterial(requestJson),
            SecretMaterial = new WindowsLocalAgentHostLaunchMaterial(secretJson),
        };
        CryptographicOperations.ZeroMemory(requestJson);
        CryptographicOperations.ZeroMemory(secretJson);
        return Task.FromResult(configuration);
    }

    private static byte[] EncodeSecretFrame(
        string launchId,
        IReadOnlyDictionary<string, ReadOnlyMemory<byte>> credentialValues)
    {
        var buffer = new ArrayBufferWriter<byte>();
        using (var writer = new Utf8JsonWriter(buffer))
        {
            writer.WriteStartObject();
            writer.WriteNumber("protocol_version", LocalAgentHostLaunchProtocol.Version);
            writer.WriteString("launch_id", launchId);
            writer.WriteStartArray("secrets");
            foreach (var reference in credentialValues.Keys.OrderBy(
                         reference => reference,
                         StringComparer.Ordinal))
            {
                writer.WriteStartObject();
                writer.WriteString("reference", reference);
                writer.WriteBase64String("value_base64", credentialValues[reference].Span);
                writer.WriteEndObject();
            }
            writer.WriteEndArray();
            writer.WriteEndObject();
        }
        return buffer.WrittenSpan.ToArray();
    }

    private static void Validate(WindowsLocalAgentHostBootstrapSettings settings)
    {
        ValidateIdentity(settings.AccountId, nameof(settings.AccountId));
        ValidateIdentity(settings.DeviceId, nameof(settings.DeviceId));
        ValidateIdentity(settings.MemorySourceId, nameof(settings.MemorySourceId));
        ValidateServiceUri(settings.ModelGatewayBaseUri, nameof(settings.ModelGatewayBaseUri));
        ValidateServiceUri(settings.MemoryEngineBaseUri, nameof(settings.MemoryEngineBaseUri));
        if (!Path.IsPathFullyQualified(settings.ExecutablePath)
            || !Path.IsPathFullyQualified(settings.AttachmentGrantDirectory)
            || !Path.IsPathFullyQualified(settings.PlatformStateDirectory))
        {
            throw new ArgumentException("Local Agent Host paths must be absolute.");
        }
    }

    private static void ValidateServiceUri(Uri uri, string parameter)
    {
        if (!uri.IsAbsoluteUri
            || (uri.Scheme != Uri.UriSchemeHttp && uri.Scheme != Uri.UriSchemeHttps)
            || !string.IsNullOrEmpty(uri.UserInfo)
            || !string.IsNullOrEmpty(uri.Query)
            || !string.IsNullOrEmpty(uri.Fragment))
        {
            throw new ArgumentException("Local Agent service URI is invalid.", parameter);
        }
    }

    private static void ValidateIdentity(string value, string parameter)
    {
        if (string.IsNullOrWhiteSpace(value)
            || value != value.Trim()
            || value.Length > 512
            || value.Any(char.IsControl))
        {
            throw new ArgumentException("Local Agent identity is invalid.", parameter);
        }
    }

    private static void EnsurePrivateDirectory(string path)
    {
        var fullPath = Path.GetFullPath(path);
        if (OperatingSystem.IsWindows())
        {
            var localAppData = Path.GetFullPath(Environment.GetFolderPath(
                Environment.SpecialFolder.LocalApplicationData));
            if (!fullPath.StartsWith(
                    $"{localAppData}{Path.DirectorySeparatorChar}",
                    StringComparison.OrdinalIgnoreCase))
            {
                throw new IOException(
                    "Local Agent attachment grants must remain in the current user's local app data.");
            }
        }

        Directory.CreateDirectory(fullPath);
        if (new DirectoryInfo(fullPath).Attributes.HasFlag(FileAttributes.ReparsePoint))
        {
            throw new IOException("Local Agent attachment grant directory is a reparse point.");
        }
    }
}
