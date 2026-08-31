using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Plugins;

internal sealed class PluginManifestLoader
{
    private const int MaximumManifestBytes = 4 * 1024 * 1024;
    private const int MaximumPackageJsonBytes = 1024 * 1024;
    private readonly string _runtimeRoot;
    private readonly PluginCredentialVault? _credentials;
    private readonly PluginOAuthBroker? _oauth;

    public PluginManifestLoader(PluginCredentialVault credentials, PluginOAuthBroker oauth)
        : this(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ChatOS",
            "WindowsClient",
            "PluginRuntime"), credentials, oauth)
    {
    }

    internal PluginManifestLoader(
        string runtimeRoot,
        PluginCredentialVault? credentials = null,
        PluginOAuthBroker? oauth = null)
    {
        _runtimeRoot = Path.GetFullPath(runtimeRoot);
        _credentials = credentials;
        _oauth = oauth;
    }

    public async Task<PreparedPluginLaunch> PrepareAsync(
        InstalledPluginRecord record,
        string requestedComponentKey,
        string? serverKey,
        string adapterSessionId,
        string? workspaceRoot,
        IReadOnlySet<string> permissionSnapshot,
        string ownerUserId,
        string deviceId,
        CancellationToken cancellationToken = default)
    {
        var installationPath = Path.GetFullPath(record.InstallationPath);
        if (!Directory.Exists(installationPath))
        {
            throw new PluginRuntimeException("Installed Plugin directory is unavailable.");
        }

        VerifyFileHash(record, installationPath, "chatos.plugin.json");
        var manifest = await ReadJsonAsync<PluginManifest>(
            Path.Combine(installationPath, "chatos.plugin.json"),
            MaximumManifestBytes,
            cancellationToken).ConfigureAwait(false);
        if (manifest.SchemaVersion != 3 ||
            !string.Equals(manifest.Version, record.Version, StringComparison.Ordinal))
        {
            throw new PluginRuntimeException("Plugin manifest does not match the installed Release.");
        }

        var componentKey = requestedComponentKey.Trim();
        if (!string.IsNullOrWhiteSpace(serverKey) &&
            !string.Equals(serverKey.Trim(), componentKey, StringComparison.Ordinal))
        {
            throw new PluginRuntimeException("Plugin MCP server_key must match component_key.");
        }

        if (!manifest.McpServers.TryGetValue(componentKey, out var server) ||
            server.EffectiveTransport is not ("stdio" or "http"))
        {
            throw new PluginRuntimeException("The requested MCP component was not found.");
        }

        var requiredPermissions = manifest.Permissions
            .Where(permission =>
                permission.Required &&
                (permission.Components.Count == 0 || permission.Components.Contains(componentKey, StringComparer.Ordinal)))
            .Select(permission => permission.Permission)
            .ToArray();
        if (requiredPermissions.Any(permission => !permissionSnapshot.Contains(permission)))
        {
            throw new PluginRuntimeException("Plugin required permissions have not been granted.");
        }

        if (server.EffectiveTransport == "http")
        {
            return await PrepareHttpAsync(
                manifest,
                record,
                componentKey,
                server,
                workspaceRoot,
                permissionSnapshot,
                ownerUserId,
                deviceId,
                cancellationToken).ConfigureAwait(false);
        }

        if (!permissionSnapshot.Contains("process.spawn"))
        {
            throw new PluginRuntimeException("Plugin stdio MCP requires process.spawn permission.");
        }

        VerifyFileHash(record, installationPath, "package.json");
        ValidateArguments(server.Arguments);
        var environmentTemplates = server.Environment.Values
            .Select(PluginCredentialTemplate.Parse)
            .ToArray();
        var credentialBinding = await PluginCredentialBinding.PrepareAsync(
            _credentials,
            ownerUserId,
            deviceId,
            record,
            componentKey,
            environmentTemplates.Where(value => value.SecretName is not null).Select(value => value.SecretName!),
            cancellationToken).ConfigureAwait(false);
        var resolvedEnvironment = await ResolveEnvironmentAsync(
            manifest,
            record,
            componentKey,
            server.Environment,
            permissionSnapshot,
            ownerUserId,
            deviceId,
            cancellationToken).ConfigureAwait(false);
        var package = await ReadJsonAsync<NpmLaunchPackage>(
            Path.Combine(installationPath, "package.json"),
            MaximumPackageJsonBytes,
            cancellationToken).ConfigureAwait(false);
        var bins = package.Bins();
        if (server.Bin is null || !bins.TryGetValue(server.Bin, out var relativeBin))
        {
            throw new PluginRuntimeException("Installed npm package does not publish the requested MCP bin.");
        }

        var binPath = ResolveRegularFile(installationPath, relativeBin);
        VerifyFileHash(record, installationPath, NormalizeRelativePath(relativeBin));
        var (executable, prefixArguments) = ResolveExecutable(binPath);
        var arguments = prefixArguments.Concat(server.Arguments).ToArray();

        var pluginHash = Sha256(record.PluginId);
        var releaseHash = Sha256(record.ReleaseId);
        var sessionHash = Sha256(adapterSessionId);
        var visualPath = Path.Combine(_runtimeRoot, "visual-sessions", pluginHash, releaseHash, sessionHash);
        var dataPath = Path.Combine(_runtimeRoot, "data", pluginHash);
        var cachePath = Path.Combine(_runtimeRoot, "cache", pluginHash);
        var artifactPath = Path.Combine(_runtimeRoot, "artifacts", sessionHash);
        var grantPath = Path.Combine(_runtimeRoot, "file-grants", sessionHash);
        foreach (var path in new[] { visualPath, dataPath, cachePath, artifactPath, grantPath })
        {
            Directory.CreateDirectory(path);
        }

        var host = JsonSerializer.SerializeToUtf8Bytes(new
        {
            protocol_version = 1,
            adapter_session_id = adapterSessionId,
            plugin_id = record.PluginId,
            component_key = componentKey,
        }, new JsonSerializerOptions(JsonSerializerDefaults.Web) { WriteIndented = true });
        await File.WriteAllBytesAsync(Path.Combine(visualPath, "host.json"), host, cancellationToken)
            .ConfigureAwait(false);

        var environment = new Dictionary<string, string>(resolvedEnvironment, StringComparer.OrdinalIgnoreCase)
        {
            ["CHATOS_PLUGIN_ROOT"] = installationPath,
            ["CHATOS_PLUGIN_DATA_DIR"] = dataPath,
            ["CHATOS_PLUGIN_CACHE_DIR"] = cachePath,
            ["CHATOS_PLUGIN_ARTIFACT_DIR"] = artifactPath,
            ["CHATOS_PLUGIN_FILE_GRANT_DIR"] = grantPath,
            ["CHATOS_PLUGIN_VISUAL_SESSION_DIR"] = visualPath,
            ["CHATOS_PLUGIN_ID"] = record.PluginId,
            ["CHATOS_PLUGIN_COMPONENT_KEY"] = componentKey,
        };
        if (!string.IsNullOrWhiteSpace(workspaceRoot))
        {
            environment["CHATOS_WORKSPACE"] = Path.GetFullPath(workspaceRoot);
        }
        else
        {
            environment.Remove("CHATOS_WORKSPACE");
        }

        return new PreparedPluginLaunch(
            record,
            componentKey,
            server,
            executable,
            arguments,
            environment,
            installationPath,
            visualPath,
            artifactPath,
            string.IsNullOrWhiteSpace(manifest.Interface?.DisplayName)
                ? manifest.Name
                : manifest.Interface.DisplayName.Trim(),
            Transport: "stdio",
            CredentialBinding: credentialBinding);
    }

    private async Task<PreparedPluginLaunch> PrepareHttpAsync(
        PluginManifest manifest,
        InstalledPluginRecord record,
        string componentKey,
        PluginMcpServer server,
        string? workspaceRoot,
        IReadOnlySet<string> permissionSnapshot,
        string ownerUserId,
        string deviceId,
        CancellationToken cancellationToken)
    {
        if (!string.IsNullOrWhiteSpace(workspaceRoot))
        {
            throw new PluginRuntimeException("Plugin HTTP MCP cannot receive a local workspace binding.");
        }

        var endpoint = ValidateHttpEndpoint(server.Url);
        var networkPermission = $"network.domain:{endpoint.Host.ToLowerInvariant()}";
        var declaredPermissions = manifest.Permissions
            .Where(permission => permission.Components.Count == 0 ||
                permission.Components.Contains(componentKey, StringComparer.Ordinal))
            .Select(permission => permission.Permission)
            .ToHashSet(StringComparer.Ordinal);
        if (!declaredPermissions.Contains(networkPermission) || !permissionSnapshot.Contains(networkPermission))
        {
            throw new PluginRuntimeException($"Plugin HTTP MCP requires permission: {networkPermission}.");
        }

        var templates = ParseHttpHeaderTemplates(server.Headers);
        var secretNames = templates.Values
            .Where(value => value.SecretName is not null)
            .Select(value => value.SecretName!)
            .ToArray();
        if (secretNames.Length > 0)
        {
            var credentialPermissions = declaredPermissions.Where(permission =>
                    permission == "credential.use" ||
                    permission.StartsWith("credential.use:", StringComparison.Ordinal))
                .ToArray();
            if (credentialPermissions.Length == 0 || !credentialPermissions.Any(permissionSnapshot.Contains))
            {
                throw new PluginRuntimeException(
                    "Plugin HTTP MCP credential templates require a declared credential.use permission.");
            }
        }

        var credentialBinding = await PluginCredentialBinding.PrepareAsync(
            _credentials,
            ownerUserId,
            deviceId,
            record,
            componentKey,
            secretNames,
            cancellationToken).ConfigureAwait(false);
        PluginOAuthTokenBinding? oauthBinding = null;
        if (!string.IsNullOrWhiteSpace(server.OAuthResource))
        {
            if (templates.ContainsKey("authorization"))
            {
                throw new PluginRuntimeException(
                    "Plugin HTTP MCP cannot combine oauthResource with an Authorization header template.");
            }

            oauthBinding = await (_oauth
                ?? throw new PluginRuntimeException("Plugin OAuth Broker is unavailable."))
                .PrepareTokenBindingAsync(
                    ownerUserId,
                    deviceId,
                    record.PluginId,
                    record.ReleaseId,
                    server.OAuthResource.Trim(),
                    cancellationToken).ConfigureAwait(false);
            foreach (var scope in oauthBinding.Scopes)
            {
                var permission = $"oauth.scope:{oauthBinding.Provider}:{scope}";
                if (!declaredPermissions.Contains(permission) || !permissionSnapshot.Contains(permission))
                {
                    throw new PluginRuntimeException($"Plugin OAuth MCP requires permission: {permission}.");
                }
            }
        }

        return new PreparedPluginLaunch(
            record,
            componentKey,
            server,
            string.Empty,
            Array.Empty<string>(),
            new Dictionary<string, string>(),
            Path.GetFullPath(record.InstallationPath),
            string.Empty,
            string.Empty,
            string.IsNullOrWhiteSpace(manifest.Interface?.DisplayName)
                ? manifest.Name
                : manifest.Interface.DisplayName.Trim(),
            Transport: "http",
            HttpEndpoint: endpoint,
            DeclaredHttpHeaderTemplates: templates,
            CredentialBinding: credentialBinding,
            OAuthBinding: oauthBinding);
    }

    private static Uri ValidateHttpEndpoint(string? value)
    {
        if (!Uri.TryCreate(value?.Trim(), UriKind.Absolute, out var endpoint) ||
            string.IsNullOrWhiteSpace(endpoint.Host) ||
            !string.IsNullOrEmpty(endpoint.UserInfo) ||
            !string.IsNullOrEmpty(endpoint.Fragment))
        {
            throw new PluginRuntimeException("Plugin HTTP MCP endpoint is invalid.");
        }

        var loopback = endpoint.Host.Equals("localhost", StringComparison.OrdinalIgnoreCase) ||
            System.Net.IPAddress.TryParse(endpoint.Host.Trim('[', ']'), out var address) &&
            System.Net.IPAddress.IsLoopback(address);
        if (!endpoint.Scheme.Equals(Uri.UriSchemeHttps, StringComparison.OrdinalIgnoreCase) &&
            !(endpoint.Scheme.Equals(Uri.UriSchemeHttp, StringComparison.OrdinalIgnoreCase) && loopback))
        {
            throw new PluginRuntimeException(
                "Plugin HTTP MCP requires HTTPS except for loopback development servers.");
        }

        return endpoint;
    }

    private static IReadOnlyDictionary<string, PluginCredentialTemplate> ParseHttpHeaderTemplates(
        IReadOnlyDictionary<string, string> headers)
    {
        if (headers.Count > 64 || headers.Sum(value =>
                Encoding.UTF8.GetByteCount(value.Key) + Encoding.UTF8.GetByteCount(value.Value)) > 32 * 1024)
        {
            throw new PluginRuntimeException("Plugin HTTP MCP headers exceed the configured limit.");
        }

        var forbidden = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            "host", "content-length", "transfer-encoding", "connection",
            "proxy-authorization", "proxy-authenticate", "te", "trailer", "upgrade",
        };
        var literalAllowed = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            "accept", "accept-language", "content-type", "mcp-protocol-version",
            "user-agent", "x-plugin-client",
        };
        var result = new Dictionary<string, PluginCredentialTemplate>(StringComparer.OrdinalIgnoreCase);
        foreach (var pair in headers)
        {
            var name = pair.Key.Trim().ToLowerInvariant();
            if (name.Length == 0 || name.Any(character =>
                    !(char.IsAsciiLetterOrDigit(character) || character is '-' or '_')) ||
                forbidden.Contains(name) || !result.TryAdd(name, PluginCredentialTemplate.Parse(pair.Value)))
            {
                throw new PluginRuntimeException("Plugin HTTP MCP contains an unsafe or duplicate header.");
            }

            var template = result[name];
            if (template.SecretName is null && !literalAllowed.Contains(name))
            {
                throw new PluginRuntimeException(
                    $"Plugin HTTP MCP custom header must use a Credential Vault template: {name}.");
            }
        }

        return result;
    }

    private static async Task<T> ReadJsonAsync<T>(
        string path,
        int maximumBytes,
        CancellationToken cancellationToken)
    {
        var info = new FileInfo(path);
        if (!info.Exists || info.Length <= 0 || info.Length > maximumBytes || IsReparsePoint(path))
        {
            throw new PluginRuntimeException("Plugin runtime metadata is missing or unsafe.");
        }

        try
        {
            await using var stream = new FileStream(
                path,
                FileMode.Open,
                FileAccess.Read,
                FileShare.Read,
                16 * 1024,
                FileOptions.Asynchronous | FileOptions.SequentialScan);
            return await JsonSerializer.DeserializeAsync<T>(
                stream,
                new JsonSerializerOptions(JsonSerializerDefaults.Web) { PropertyNameCaseInsensitive = true },
                cancellationToken).ConfigureAwait(false)
                ?? throw new JsonException("JSON document is empty.");
        }
        catch (JsonException exception)
        {
            throw new PluginRuntimeException("Plugin runtime metadata is invalid.", exception);
        }
    }

    private static string ResolveRegularFile(string root, string relativePath)
    {
        var normalized = NormalizeRelativePath(relativePath);

        var parts = normalized.Split('/', StringSplitOptions.None);
        if (parts.Length == 0 || parts.Any(part => part.Length == 0 || part is "." or ".."))
        {
            throw new PluginRuntimeException("Plugin MCP bin path is invalid.");
        }

        var path = Path.GetFullPath(Path.Combine(root, normalized.Replace('/', Path.DirectorySeparatorChar)));
        var rootPrefix = Path.TrimEndingDirectorySeparator(Path.GetFullPath(root)) + Path.DirectorySeparatorChar;
        if (!path.StartsWith(rootPrefix, StringComparison.OrdinalIgnoreCase) ||
            !File.Exists(path) ||
            IsReparsePoint(path))
        {
            throw new PluginRuntimeException("Plugin MCP bin is not a safe regular file.");
        }

        return path;
    }

    private static string NormalizeRelativePath(string relativePath)
    {
        var normalized = relativePath.Replace('\\', '/').Trim();
        while (normalized.StartsWith("./", StringComparison.Ordinal))
        {
            normalized = normalized[2..];
        }

        return normalized;
    }

    private static void VerifyFileHash(
        InstalledPluginRecord record,
        string installationPath,
        string relativePath)
    {
        if (record.PackageFileSha256 is null)
        {
            return;
        }

        var normalized = NormalizeRelativePath(relativePath);
        if (!record.PackageFileSha256.TryGetValue(normalized, out var expected))
        {
            throw new PluginRuntimeException("Plugin runtime file is not covered by the installation checksums.");
        }

        var path = Path.Combine(installationPath, normalized.Replace('/', Path.DirectorySeparatorChar));
        var actual = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path))).ToLowerInvariant();
        if (!CryptographicOperations.FixedTimeEquals(
                Convert.FromHexString(expected),
                Convert.FromHexString(actual)))
        {
            throw new PluginRuntimeException("Plugin runtime file checksum changed after installation.");
        }
    }

    private static (string Executable, IReadOnlyList<string> PrefixArguments) ResolveExecutable(string binPath)
    {
        var extension = Path.GetExtension(binPath);
        var nodeLauncher = extension.Equals(".js", StringComparison.OrdinalIgnoreCase) ||
            extension.Equals(".cjs", StringComparison.OrdinalIgnoreCase) ||
            extension.Equals(".mjs", StringComparison.OrdinalIgnoreCase) ||
            FirstLine(binPath).Contains("node", StringComparison.OrdinalIgnoreCase);
        if (!nodeLauncher)
        {
            if (OperatingSystem.IsWindows() &&
                !extension.Equals(".exe", StringComparison.OrdinalIgnoreCase) &&
                !extension.Equals(".com", StringComparison.OrdinalIgnoreCase))
            {
                throw new PluginRuntimeException("Windows native Plugin MCP bin must be an .exe or .com file.");
            }

            return (binPath, Array.Empty<string>());
        }

        return (ResolveFromPath(OperatingSystem.IsWindows() ? "node.exe" : "node"), [binPath]);
    }

    private static string FirstLine(string path)
    {
        using var stream = File.OpenRead(path);
        var buffer = new byte[Math.Min(256, checked((int)Math.Min(stream.Length, 256)))];
        _ = stream.Read(buffer, 0, buffer.Length);
        return Encoding.UTF8.GetString(buffer).Split('\n', 2)[0];
    }

    private static string ResolveFromPath(string executable)
    {
        foreach (var rawDirectory in (Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
                     .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            var directory = rawDirectory.Trim('"');
            if (!Path.IsPathFullyQualified(directory))
            {
                continue;
            }

            var candidate = Path.Combine(directory, executable);
            if (File.Exists(candidate) && !IsReparsePoint(candidate))
            {
                return Path.GetFullPath(candidate);
            }
        }

        throw new PluginRuntimeException($"Required Plugin runtime executable was not found: {executable}");
    }

    private static void ValidateArguments(IReadOnlyList<string> arguments)
    {
        if (arguments.Count > 128 || arguments.Any(argument =>
                argument.Length > 8 * 1024 ||
                argument.Contains('\0') ||
                argument is "-c" or "--eval" or "--execute"))
        {
            throw new PluginRuntimeException("Plugin MCP contains an unsafe or oversized argument.");
        }
    }

    private async Task<IReadOnlyDictionary<string, string>> ResolveEnvironmentAsync(
        PluginManifest manifest,
        InstalledPluginRecord record,
        string componentKey,
        IReadOnlyDictionary<string, string> declaredEnvironment,
        IReadOnlySet<string> permissionSnapshot,
        string ownerUserId,
        string deviceId,
        CancellationToken cancellationToken)
    {
        if (declaredEnvironment.Count > 64)
        {
            throw new PluginRuntimeException("Plugin stdio environment contains too many variables.");
        }

        var templates = new Dictionary<string, PluginCredentialTemplate>(StringComparer.OrdinalIgnoreCase);
        foreach (var pair in declaredEnvironment)
        {
            if (string.IsNullOrWhiteSpace(pair.Key) ||
                pair.Key.Contains('=') ||
                pair.Key.Contains('\0') ||
                pair.Key.Length > 256)
            {
                throw new PluginRuntimeException("Plugin stdio environment variable name is invalid.");
            }

            var template = PluginCredentialTemplate.Parse(pair.Value);
            if (template.SecretName is null || template.Prefix.Length != 0 || template.Suffix.Length != 0)
            {
                throw new PluginRuntimeException(
                    "Plugin stdio environment values must be exact Credential Vault templates.");
            }

            templates.Add(pair.Key, template);
        }

        if (templates.Count == 0)
        {
            return new Dictionary<string, string>();
        }

        var declaredCredentialPermissions = manifest.Permissions
            .Where(permission =>
                permission.Components.Count == 0 || permission.Components.Contains(componentKey, StringComparer.Ordinal))
            .Select(permission => permission.Permission)
            .Where(permission =>
                permission == "credential.use" || permission.StartsWith("credential.use:", StringComparison.Ordinal))
            .ToArray();
        if (declaredCredentialPermissions.Length == 0 ||
            !declaredCredentialPermissions.Any(permissionSnapshot.Contains))
        {
            throw new PluginRuntimeException(
                "Plugin credential templates require a declared credential.use permission.");
        }

        var credentials = _credentials
            ?? throw new PluginRuntimeException("Plugin Credential Vault is unavailable.");
        var result = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var pair in templates)
        {
            var scope = new PluginCredentialScope(
                ownerUserId,
                deviceId,
                record.PluginId,
                record.ReleaseId,
                componentKey,
                pair.Value.SecretName!);
            var value = await pair.Value.ResolveAsync(credentials, scope, cancellationToken).ConfigureAwait(false);
            if (value.Contains('\0'))
            {
                throw new PluginRuntimeException("Resolved Plugin credential contains NUL.");
            }

            result[pair.Key] = value;
        }

        return result;
    }

    private static bool IsReparsePoint(string path) =>
        (File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0;

    private static string Sha256(string value) =>
        Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(value))).ToLowerInvariant();

    private sealed record NpmLaunchPackage
    {
        [System.Text.Json.Serialization.JsonPropertyName("name")]
        public required string Name { get; init; }

        [System.Text.Json.Serialization.JsonPropertyName("bin")]
        public JsonElement Bin { get; init; }

        public IReadOnlyDictionary<string, string> Bins()
        {
            if (Bin.ValueKind == JsonValueKind.String)
            {
                var name = Name[(Name.LastIndexOf('/') + 1)..];
                return new Dictionary<string, string>(StringComparer.Ordinal)
                {
                    [name] = Bin.GetString() ?? string.Empty,
                };
            }

            return Bin.ValueKind == JsonValueKind.Object
                ? Bin.EnumerateObject()
                    .Where(property => property.Value.ValueKind == JsonValueKind.String)
                    .ToDictionary(
                        property => property.Name,
                        property => property.Value.GetString() ?? string.Empty,
                        StringComparer.Ordinal)
                : new Dictionary<string, string>();
        }
    }
}
