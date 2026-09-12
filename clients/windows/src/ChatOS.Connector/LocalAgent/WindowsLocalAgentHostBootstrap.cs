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
    public required Uri ModelGatewayBaseUri { get; init; }
    public required Uri MemoryEngineBaseUri { get; init; }
    public string MemorySourceId { get; init; } = "local-agent";
    public required WindowsLocalAgentStorageBootstrap Storage { get; init; }
}

public sealed class WindowsLocalAgentHostBootstrapBuilder
{
    private readonly IWindowsLocalAgentCredentialStore _credentials;

    public WindowsLocalAgentHostBootstrapBuilder(WindowsLocalAgentCredentialStore credentials)
        : this((IWindowsLocalAgentCredentialStore)credentials)
    {
    }

    internal WindowsLocalAgentHostBootstrapBuilder(IWindowsLocalAgentCredentialStore credentials)
    {
        _credentials = credentials;
    }

    public const string ModelAccessTokenReference = "model-access-token";
    public const string ProviderContextKeyReference = "provider-context-key";

    public async Task<WindowsLocalAgentHostLaunchConfiguration> BuildAsync(
        WindowsLocalAgentHostBootstrapSettings settings,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(settings);
        Validate(settings);
        EnsurePrivateDirectory(settings.AttachmentGrantDirectory);
        var launchId = $"launch-{Guid.NewGuid():N}";
        var workerId = $"worker-{Guid.NewGuid():N}";
        var pipeName = $"chatos-local-agent-{Guid.NewGuid():N}";
        var modelToken = await RequiredCredentialAsync(
            settings.AccountId,
            ModelAccessTokenReference,
            cancellationToken).ConfigureAwait(false);
        var providerKey = await RequiredDeviceKeyAsync(
            settings.AccountId,
            ProviderContextKeyReference,
            cancellationToken).ConfigureAwait(false);
        if (providerKey.Length != 32)
        {
            CryptographicOperations.ZeroMemory(providerKey);
            throw new InvalidDataException("Local Agent provider context key must be 32 bytes.");
        }

        Dictionary<string, object> storageProfile;
        Dictionary<string, object> storageCredentials;
        try
        {
            switch (settings.Storage)
            {
                case WindowsLocalAgentSqliteBootstrap sqlite:
                    ValidateIdentity(sqlite.EncryptionSecretReference, nameof(sqlite.EncryptionSecretReference));
                    if (!Path.IsPathFullyQualified(sqlite.DatabasePath))
                    {
                        throw new ArgumentException("SQLite database path must be absolute.");
                    }

                    var key = await RequiredDeviceKeyAsync(
                        settings.AccountId,
                        sqlite.EncryptionSecretReference,
                        cancellationToken).ConfigureAwait(false);
                    try
                    {
                        if (key.Length != 32)
                        {
                            throw new InvalidDataException("Local Agent SQLite key must be 32 bytes.");
                        }

                        storageProfile = new Dictionary<string, object>
                        {
                            ["backend"] = "sqlite",
                            ["database_path"] = sqlite.DatabasePath,
                            ["encryption_secret"] = sqlite.EncryptionSecretReference,
                        };
                        storageCredentials = new Dictionary<string, object>
                        {
                            ["backend"] = "sqlite",
                            ["encryption_secret_reference"] = sqlite.EncryptionSecretReference,
                            ["encryption_key_base64"] = Convert.ToBase64String(key),
                        };
                    }
                    finally
                    {
                        CryptographicOperations.ZeroMemory(key);
                    }
                    break;
                case WindowsLocalAgentPostgresBootstrap postgres:
                    ValidateIdentity(postgres.ConnectionSecretReference, nameof(postgres.ConnectionSecretReference));
                    var envelopeJson = await RequiredCredentialAsync(
                        settings.AccountId,
                        postgres.ConnectionSecretReference,
                        cancellationToken).ConfigureAwait(false);
                    var envelope = JsonSerializer.Deserialize<WindowsLocalAgentPostgresCredential>(envelopeJson)
                        ?? throw new InvalidDataException("Local Agent PostgreSQL credential is invalid.");
                    Validate(envelope);
                    storageProfile = new Dictionary<string, object>
                    {
                        ["backend"] = "postgres",
                        ["connection_secret"] = postgres.ConnectionSecretReference,
                    };
                    storageCredentials = new Dictionary<string, object>
                    {
                        ["backend"] = "postgres",
                        ["connection_secret_reference"] = postgres.ConnectionSecretReference,
                        ["host"] = envelope.Host,
                        ["port"] = envelope.Port,
                        ["database"] = envelope.Database,
                        ["tls_mode"] = envelope.TlsMode,
                        ["username"] = envelope.Username,
                        ["password"] = envelope.Password,
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
                ["model_gateway_base_url"] = settings.ModelGatewayBaseUri.AbsoluteUri.TrimEnd('/'),
                ["memory_engine_base_url"] = settings.MemoryEngineBaseUri.AbsoluteUri.TrimEnd('/'),
                ["memory_source_id"] = settings.MemorySourceId,
                ["storage_profile"] = storageProfile,
                ["credentials"] = new Dictionary<string, object>
                {
                    ["model_access_token"] = modelToken,
                    ["provider_context_key_base64"] = Convert.ToBase64String(providerKey),
                    ["storage"] = storageCredentials,
                },
            };
            var requestJson = JsonSerializer.SerializeToUtf8Bytes(request);
            try
            {
                return new WindowsLocalAgentHostLaunchConfiguration
                {
                    ExecutablePath = settings.ExecutablePath,
                    ExpectedExecutableSha256 = settings.ExpectedExecutableSha256,
                    LaunchId = launchId,
                    ExpectedClientEndpoint = pipeName,
                    LaunchMaterial = new WindowsLocalAgentHostLaunchMaterial(requestJson),
                };
            }
            finally
            {
                CryptographicOperations.ZeroMemory(requestJson);
            }
        }
        finally
        {
            CryptographicOperations.ZeroMemory(providerKey);
        }
    }

    private async Task<string> RequiredCredentialAsync(
        string accountId,
        string reference,
        CancellationToken cancellationToken)
    {
        var value = await _credentials.LoadCredentialAsync(accountId, reference, cancellationToken)
            .ConfigureAwait(false);
        if (string.IsNullOrEmpty(value) || value != value.Trim())
        {
            throw new InvalidDataException($"Required Local Agent credential {reference} is unavailable.");
        }
        return value;
    }

    private async Task<byte[]> RequiredDeviceKeyAsync(
        string accountId,
        string reference,
        CancellationToken cancellationToken)
    {
        var value = await _credentials.LoadDeviceKeyAsync(accountId, reference, cancellationToken)
            .ConfigureAwait(false);
        return value is { Length: > 0 }
            ? value
            : throw new InvalidDataException($"Required Local Agent device key {reference} is unavailable.");
    }

    private static void Validate(WindowsLocalAgentHostBootstrapSettings settings)
    {
        ValidateIdentity(settings.AccountId, nameof(settings.AccountId));
        ValidateIdentity(settings.DeviceId, nameof(settings.DeviceId));
        ValidateIdentity(settings.MemorySourceId, nameof(settings.MemorySourceId));
        ValidateServiceUri(settings.ModelGatewayBaseUri, nameof(settings.ModelGatewayBaseUri));
        ValidateServiceUri(settings.MemoryEngineBaseUri, nameof(settings.MemoryEngineBaseUri));
        if (!Path.IsPathFullyQualified(settings.ExecutablePath)
            || !Path.IsPathFullyQualified(settings.AttachmentGrantDirectory))
        {
            throw new ArgumentException("Local Agent Host paths must be absolute.");
        }
    }

    private static void Validate(WindowsLocalAgentPostgresCredential credential)
    {
        ValidateIdentity(credential.Host, nameof(credential.Host));
        ValidateIdentity(credential.Database, nameof(credential.Database));
        ValidateIdentity(credential.Username, nameof(credential.Username));
        if (credential.Port == 0
            || !string.Equals(credential.TlsMode, "verify_full", StringComparison.Ordinal)
            || string.IsNullOrEmpty(credential.Password)
            || credential.Password != credential.Password.Trim())
        {
            throw new InvalidDataException("Local Agent PostgreSQL credential is invalid.");
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
