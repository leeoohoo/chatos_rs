using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Plugins;

internal sealed partial class PluginManifestLoader
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

    internal async Task<PreparedPluginApplication> PrepareApplicationAsync(
        InstalledPluginRecord record,
        string requestedComponentKey,
        IReadOnlySet<string> permissionSnapshot,
        string ownerUserId,
        string deviceId,
        string? workspaceId = null,
        string? workspaceRoot = null,
        string? projectId = null,
        string? projectName = null,
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
        var contribution = manifest.Ui.FirstOrDefault(value =>
            string.Equals(value.ComponentKey, componentKey, StringComparison.Ordinal) &&
            string.Equals(value.Surface, "workbench", StringComparison.Ordinal))
            ?? throw new PluginRuntimeException("The requested Plugin application was not found.");
        var requiredPermissions = manifest.Permissions
            .Where(permission => permission.Required &&
                (permission.Components.Count == 0 ||
                 permission.Components.Contains(componentKey, StringComparer.Ordinal)))
            .Select(permission => permission.Permission)
            .ToArray();
        if (requiredPermissions.Any(permission => !permissionSnapshot.Contains(permission)))
        {
            throw new PluginRuntimeException("Plugin application permissions have not been granted.");
        }

        var context = ResolveRuntimeContext(
            manifest,
            componentKey,
            record.PluginId,
            ownerUserId,
            deviceId,
            workspaceId,
            workspaceRoot,
            projectId,
            projectName);
        Directory.CreateDirectory(context.DataPath);
        Directory.CreateDirectory(context.CachePath);
        var sourcePath = ResolveRegularFile(installationPath, contribution.Source.Path!);
        VerifyFileHash(record, installationPath, contribution.Source.Path!);
        foreach (var asset in contribution.Assets)
        {
            _ = ResolveRegularFile(installationPath, asset);
            VerifyFileHash(record, installationPath, asset);
        }
        var iconPath = ResolveOptionalInterfaceAsset(record, manifest.Interface?.Logo, installationPath);

        var declaration = manifest.RuntimeContext;
        var application = new LocalPluginApplication(
            record.PluginId,
            componentKey,
            string.IsNullOrWhiteSpace(contribution.Title)
                ? string.IsNullOrWhiteSpace(manifest.Interface?.DisplayName)
                    ? manifest.Name
                    : manifest.Interface!.DisplayName!.Trim()
                : contribution.Title.Trim(),
            ApplicationDescription(manifest),
            manifest.Interface?.BrandColor,
            contribution.Runtime is not null,
            declaration?.AppliesTo(componentKey) == true ? declaration.Scope : null,
            declaration?.AppliesTo(componentKey) == true ? declaration.MissingContext : null,
            contribution.BridgeCapabilities.ToArray(),
            iconPath);
        var contextKey = context.Environment.TryGetValue("CHATOS_CONTEXT_SCOPE_ID", out var scopeId)
            ? scopeId
            : Sha256($"device:{deviceId}");
        var environment = new Dictionary<string, string>(context.Environment, StringComparer.OrdinalIgnoreCase)
        {
            ["CHATOS_PLUGIN_ROOT"] = installationPath,
            ["CHATOS_PLUGIN_DATA_DIR"] = context.DataPath,
            ["CHATOS_PLUGIN_CACHE_DIR"] = context.CachePath,
            ["CHATOS_PLUGIN_ID"] = record.PluginId,
            ["CHATOS_PLUGIN_COMPONENT_KEY"] = componentKey,
            ["CHATOS_PLUGIN_RELEASE_ID"] = record.ReleaseId,
            ["CHATOS_PLUGIN_VERSION"] = record.Version,
            ["CHATOS_PLUGIN_ARTIFACT_SHA256"] = record.ArtifactSha256,
        };
        if (contribution.Runtime is not { } runtime)
        {
            return new PreparedPluginApplication(
                application, record, contextKey, installationPath, sourcePath, null,
                Array.Empty<string>(), environment, "/api/health", 15_000);
        }
        if (!permissionSnapshot.Contains("process.spawn"))
        {
            throw new PluginRuntimeException("Plugin application requires process.spawn permission.");
        }
        ValidateArguments(runtime.Arguments);
        VerifyFileHash(record, installationPath, "package.json");
        var package = await ReadJsonAsync<NpmLaunchPackage>(
            Path.Combine(installationPath, "package.json"),
            MaximumPackageJsonBytes,
            cancellationToken).ConfigureAwait(false);
        var bins = package.Bins();
        if (!bins.TryGetValue(runtime.Bin, out var relativeBin))
        {
            throw new PluginRuntimeException("Installed npm package does not publish the Plugin application bin.");
        }
        var binPath = ResolveRegularFile(installationPath, relativeBin);
        VerifyFileHash(record, installationPath, NormalizeRelativePath(relativeBin));
        var (executable, prefixArguments) = ResolveExecutable(binPath);
        var healthPath = ValidateHealthPath(runtime.HealthPath);
        return new PreparedPluginApplication(
            application,
            record,
            contextKey,
            installationPath,
            sourcePath,
            executable,
            prefixArguments.Concat(runtime.Arguments).ToArray(),
            environment,
            healthPath,
            Math.Clamp(runtime.LaunchTimeoutMilliseconds ?? 15_000, 100, 120_000));
    }

    internal async Task<IReadOnlyList<LocalPluginApplication>> ListApplicationsAsync(
        InstalledPluginRecord record,
        CancellationToken cancellationToken = default)
    {
        var installationPath = Path.GetFullPath(record.InstallationPath);
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
        return manifest.Ui
            .Where(value => string.Equals(value.Surface, "workbench", StringComparison.Ordinal))
            .Select(contribution =>
            {
                var declaration = manifest.RuntimeContext;
                var iconPath = ResolveOptionalInterfaceAsset(record, manifest.Interface?.Logo, installationPath);
                return new LocalPluginApplication(
                    record.PluginId,
                    contribution.ComponentKey,
                    string.IsNullOrWhiteSpace(contribution.Title)
                        ? string.IsNullOrWhiteSpace(manifest.Interface?.DisplayName)
                            ? manifest.Name
                            : manifest.Interface!.DisplayName!.Trim()
                        : contribution.Title.Trim(),
                    ApplicationDescription(manifest),
                    manifest.Interface?.BrandColor,
                    contribution.Runtime is not null,
                    declaration?.AppliesTo(contribution.ComponentKey) == true ? declaration.Scope : null,
                    declaration?.AppliesTo(contribution.ComponentKey) == true ? declaration.MissingContext : null,
                    contribution.BridgeCapabilities.ToArray(),
                    iconPath);
            })
            .ToArray();
    }

    internal async Task<IReadOnlyList<string>> ListMcpComponentsAsync(
        InstalledPluginRecord record,
        CancellationToken cancellationToken = default)
    {
        var installationPath = Path.GetFullPath(record.InstallationPath);
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

        return manifest.McpServers.Keys.Order(StringComparer.Ordinal).ToArray();
    }

    private static string ApplicationDescription(PluginManifest manifest) =>
        !string.IsNullOrWhiteSpace(manifest.Interface?.ShortDescription)
            ? manifest.Interface.ShortDescription.Trim()
            : manifest.Description;

    private static string? ResolveOptionalInterfaceAsset(
        InstalledPluginRecord record,
        PluginPathReference? reference,
        string installationPath)
    {
        if (string.IsNullOrWhiteSpace(reference?.Path)) return null;
        var path = ResolveRegularFile(installationPath, reference.Path);
        VerifyFileHash(record, installationPath, reference.Path);
        return path;
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
        string? workspaceId = null,
        string? projectId = null,
        string? projectName = null,
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
        var userHash = Sha256(ownerUserId);
        var visualPath = Path.Combine(
            _runtimeRoot,
            "visual-sessions",
            "users",
            userHash,
            pluginHash,
            releaseHash,
            sessionHash);
        var context = ResolveRuntimeContext(
            manifest,
            componentKey,
            record.PluginId,
            ownerUserId,
            deviceId,
            workspaceId,
            workspaceRoot,
            projectId,
            projectName);
        var dataPath = context.DataPath;
        var cachePath = context.CachePath;
        var artifactPath = Path.Combine(_runtimeRoot, "artifacts", "users", userHash, sessionHash);
        var grantPath = Path.Combine(_runtimeRoot, "file-grants", "users", userHash, sessionHash);
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
        foreach (var (name, value) in context.Environment)
        {
            environment[name] = value;
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

    private RuntimeContextResolution ResolveRuntimeContext(
        PluginManifest manifest,
        string componentKey,
        string pluginId,
        string ownerUserId,
        string deviceId,
        string? workspaceId,
        string? workspaceRoot,
        string? projectId,
        string? projectName)
    {
        var pluginHash = Sha256(pluginId);
        var userHash = Sha256(ownerUserId);
        var userDataPath = Path.Combine(_runtimeRoot, "data", "users", userHash, pluginHash);
        var userCachePath = Path.Combine(_runtimeRoot, "cache", "users", userHash, pluginHash);
        var declaration = manifest.RuntimeContext;
        if (declaration is null || !declaration.AppliesTo(componentKey))
        {
            return new RuntimeContextResolution(
                userDataPath,
                userCachePath,
                new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase));
        }

        var requested = declaration.Required.Concat(declaration.Optional)
            .ToHashSet(StringComparer.Ordinal);
        foreach (var field in declaration.Required)
        {
            var available = field switch
            {
                "project.id" => !string.IsNullOrWhiteSpace(projectId),
                "workspace.id" => !string.IsNullOrWhiteSpace(workspaceId),
                "workspace.root" => !string.IsNullOrWhiteSpace(workspaceRoot),
                _ => false,
            };
            if (!available)
            {
                throw new PluginRuntimeException($"Plugin runtime is missing required context: {field}.");
            }
        }

        var (scopeKind, scopeIdentity) = declaration.Scope switch
        {
            "device" => ("device", $"device:{deviceId}"),
            "workspace" when !string.IsNullOrWhiteSpace(workspaceId) =>
                ("workspace", $"workspace:{workspaceId!.Trim()}"),
            "project" when !string.IsNullOrWhiteSpace(projectId) =>
                ("project", $"project:{projectId!.Trim()}"),
            _ when declaration.MissingContext == "device" => ("device", $"device:{deviceId}"),
            _ => throw new PluginRuntimeException("Plugin requires project or workspace context."),
        };
        var storageKind = scopeKind;
        var storageIdentity = scopeIdentity;
        if (declaration.StorageIsolation == "workspace" && !string.IsNullOrWhiteSpace(workspaceId))
        {
            storageKind = "workspace";
            storageIdentity = $"workspace:{workspaceId!.Trim()}";
        }
        else if (declaration.StorageIsolation == "project" && !string.IsNullOrWhiteSpace(projectId))
        {
            storageKind = "project";
            storageIdentity = $"project:{projectId!.Trim()}";
        }
        else if (declaration.StorageIsolation != "plugin" && declaration.MissingContext == "device")
        {
            storageKind = "device";
            storageIdentity = $"device:{deviceId}";
        }
        var dataPath = declaration.StorageIsolation == "plugin"
            ? userDataPath
            : Path.Combine(userDataPath, "scopes", storageKind, Sha256(storageIdentity));
        var cachePath = declaration.StorageIsolation == "plugin"
            ? userCachePath
            : Path.Combine(userCachePath, "scopes", storageKind, Sha256(storageIdentity));
        var environment = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["CHATOS_CONTEXT_SCOPE"] = scopeKind,
            ["CHATOS_CONTEXT_SCOPE_ID"] = Sha256(scopeIdentity),
        };
        if (requested.Contains("project.id") && !string.IsNullOrWhiteSpace(projectId))
        {
            environment["CHATOS_PROJECT_ID"] = projectId.Trim();
        }
        if (!string.IsNullOrWhiteSpace(projectName))
        {
            environment["CHATOS_PROJECT_NAME"] = projectName.Trim();
        }
        if (requested.Contains("workspace.id") && !string.IsNullOrWhiteSpace(workspaceId))
        {
            environment["CHATOS_WORKSPACE_ID"] = workspaceId.Trim();
        }
        if (requested.Contains("workspace.root") && !string.IsNullOrWhiteSpace(workspaceRoot))
        {
            environment["CHATOS_WORKSPACE"] = Path.GetFullPath(workspaceRoot);
        }
        return new RuntimeContextResolution(dataPath, cachePath, environment);
    }

    private sealed record RuntimeContextResolution(
        string DataPath,
        string CachePath,
        IReadOnlyDictionary<string, string> Environment);

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

}
