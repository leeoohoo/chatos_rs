using System.Security.Cryptography;
using System.Text;
using ChatOS.Api.Http;
using Microsoft.Extensions.Configuration;

namespace ChatOS.Connector.LocalAgent;

public interface IWindowsLocalAgentRuntimeConfiguration
{
    WindowsLocalAgentHostBootstrapSettings Create(string accountId, string deviceId);
}

public sealed record WindowsLocalAgentRuntimeOptions
{
    public string? HostExecutablePath { get; init; }
    public string? HostExecutableSha256 { get; init; }
    public string? StateRootDirectory { get; init; }
    public string? ModelGatewayBaseUrl { get; init; }
    public string? MemoryEngineBaseUrl { get; init; }
    public string MemorySourceId { get; init; } = "local-agent";
}

/// Resolves the Windows system boundary for the shared Rust Local Agent Host.
/// It does not contain an Agent loop or storage implementation.
public sealed class WindowsLocalAgentRuntimeConfiguration : IWindowsLocalAgentRuntimeConfiguration
{
    private readonly WindowsLocalAgentRuntimeOptions _options;
    private readonly string _apiBaseUrl;

    public WindowsLocalAgentRuntimeConfiguration(IConfiguration configuration)
        : this(
            CreateOptions(configuration),
            configuration[$"{ChatOSApiOptions.SectionName}:BaseUrl"]
                ?? Environment.GetEnvironmentVariable("CHATOS_API_BASE_URL")
                ?? "http://127.0.0.1:9080/api/chatos/")
    {
    }

    private static WindowsLocalAgentRuntimeOptions CreateOptions(IConfiguration configuration)
    {
        ArgumentNullException.ThrowIfNull(configuration);
        const string prefix = "ChatOS:LocalAgent:";
        return new WindowsLocalAgentRuntimeOptions
        {
            HostExecutablePath = configuration[$"{prefix}HostExecutablePath"],
            HostExecutableSha256 = configuration[$"{prefix}HostExecutableSha256"],
            StateRootDirectory = configuration[$"{prefix}StateRootDirectory"],
            ModelGatewayBaseUrl = configuration[$"{prefix}ModelGatewayBaseUrl"],
            MemoryEngineBaseUrl = configuration[$"{prefix}MemoryEngineBaseUrl"],
            MemorySourceId = configuration[$"{prefix}MemorySourceId"] ?? "local-agent",
        };
    }

    internal WindowsLocalAgentRuntimeConfiguration(
        WindowsLocalAgentRuntimeOptions options,
        string apiBaseUrl)
    {
        _options = options ?? throw new ArgumentNullException(nameof(options));
        _apiBaseUrl = apiBaseUrl;
    }

    public WindowsLocalAgentHostBootstrapSettings Create(string accountId, string deviceId)
    {
        ValidateIdentity(accountId, nameof(accountId));
        ValidateIdentity(deviceId, nameof(deviceId));
        var executablePath = Path.GetFullPath(FirstNonEmpty(
            _options.HostExecutablePath,
            Environment.GetEnvironmentVariable("CHATOS_LOCAL_AGENT_HOST_EXECUTABLE"),
            Path.Combine(AppContext.BaseDirectory, "LocalAgent", "chatos_local_agent_host.exe"))!);
        var executableSha256 = FirstNonEmpty(
            _options.HostExecutableSha256,
            Environment.GetEnvironmentVariable("CHATOS_LOCAL_AGENT_HOST_SHA256"));
        if (!IsSha256(executableSha256))
        {
            throw new InvalidOperationException(
                "The signed Local Agent Host SHA-256 is missing or invalid.");
        }

        var root = Path.GetFullPath(FirstNonEmpty(
            _options.StateRootDirectory,
            Environment.GetEnvironmentVariable("CHATOS_LOCAL_AGENT_STATE_ROOT"),
            Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "ChatOS",
                "LocalAgent"))!);
        var accountDirectory = Path.Combine(root, "Accounts", AccountDirectoryName(accountId));
        var serviceRoot = ServiceRoot(_apiBaseUrl);
        var modelGateway = ServiceUri(
            FirstNonEmpty(
                _options.ModelGatewayBaseUrl,
                Environment.GetEnvironmentVariable("CHATOS_MODEL_GATEWAY_BASE_URL"))
                ?? serviceRoot.AbsoluteUri,
            "Model Gateway");
        var memoryEngine = ServiceUri(
            FirstNonEmpty(
                _options.MemoryEngineBaseUrl,
                Environment.GetEnvironmentVariable("CHATOS_MEMORY_ENGINE_BASE_URL"))
                ?? serviceRoot.AbsoluteUri,
            "Memory Engine");

        return new WindowsLocalAgentHostBootstrapSettings
        {
            ExecutablePath = executablePath,
            ExpectedExecutableSha256 = executableSha256!,
            AccountId = accountId,
            DeviceId = deviceId,
            AttachmentGrantDirectory = Path.Combine(accountDirectory, "AttachmentGrants"),
            PlatformStateDirectory = Path.Combine(accountDirectory, "PlatformState"),
            ModelGatewayBaseUri = modelGateway,
            MemoryEngineBaseUri = memoryEngine,
            MemorySourceId = _options.MemorySourceId,
            Storage = new WindowsLocalAgentSqliteBootstrap(
                Path.Combine(accountDirectory, "Client.sqlite3"),
                WindowsLocalAgentAccountSession.SqliteEncryptionKeyReference),
        };
    }

    private static string? FirstNonEmpty(params string?[] values) => values
        .Select(value => value?.Trim())
        .FirstOrDefault(value => !string.IsNullOrEmpty(value));

    private static Uri ServiceRoot(string value)
    {
        var uri = ServiceUri(value, "ChatOS API");
        var builder = new UriBuilder(uri);
        const string suffix = "/api/chatos";
        var path = builder.Path.TrimEnd('/');
        if (path.EndsWith(suffix, StringComparison.Ordinal))
        {
            path = path[..^suffix.Length];
        }
        builder.Path = string.IsNullOrEmpty(path) ? "/" : path;
        builder.Query = string.Empty;
        builder.Fragment = string.Empty;
        return builder.Uri;
    }

    private static Uri ServiceUri(string value, string name)
    {
        if (!Uri.TryCreate(value.Trim(), UriKind.Absolute, out var uri)
            || (uri.Scheme != Uri.UriSchemeHttp && uri.Scheme != Uri.UriSchemeHttps)
            || !string.IsNullOrEmpty(uri.UserInfo)
            || !string.IsNullOrEmpty(uri.Query)
            || !string.IsNullOrEmpty(uri.Fragment))
        {
            throw new InvalidOperationException($"The {name} URL is invalid.");
        }
        return uri;
    }

    private static string AccountDirectoryName(string accountId) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(accountId)))
            .ToLowerInvariant();

    private static bool IsSha256(string? value)
    {
        if (value is null
            || !value.StartsWith("sha256:", StringComparison.Ordinal)
            || value.Length != "sha256:".Length + 64)
        {
            return false;
        }
        return value["sha256:".Length..].All(character =>
            char.IsAsciiDigit(character) || character is >= 'a' and <= 'f');
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
}
