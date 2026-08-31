using System.Security.Cryptography;
using System.Text.Json;
using System.Text.Json.Serialization;
using ChatOS.Connector.Runtime;

namespace ChatOS.Connector.Plugins;

public sealed record PluginPermissionConfiguration(
    string Permission,
    bool Required,
    string? Reason);

public sealed record PluginCredentialConfiguration(
    string ComponentKey,
    string SecretName,
    bool Configured,
    DateTimeOffset? UpdatedAt);

public sealed record PluginMcpComponentConfiguration(
    string ComponentKey,
    string Transport,
    string? OAuthResource,
    IReadOnlyList<PluginPermissionConfiguration> Permissions,
    IReadOnlyList<PluginCredentialConfiguration> Credentials);

public sealed record PluginOAuthAppConfiguration(
    string ComponentKey,
    string Provider,
    string Resource,
    IReadOnlyList<string> Scopes,
    PluginOAuthConnection? Connection);

public sealed record PluginConfigurationSnapshot(
    string PluginId,
    string ReleaseId,
    string Version,
    IReadOnlyList<PluginMcpComponentConfiguration> Components,
    IReadOnlyList<PluginOAuthAppConfiguration> OAuthApps);

public interface IPluginConfigurationService
{
    Task<PluginConfigurationSnapshot> GetAsync(
        string pluginId,
        CancellationToken cancellationToken = default);

    Task SetCredentialAsync(
        string pluginId,
        string componentKey,
        string secretName,
        string value,
        CancellationToken cancellationToken = default);

    Task DeleteCredentialAsync(
        string pluginId,
        string componentKey,
        string secretName,
        CancellationToken cancellationToken = default);

    Task<PluginOAuthAuthorizationStart> BeginOAuthAsync(
        string pluginId,
        string componentKey,
        CancellationToken cancellationToken = default);

    Task DisconnectOAuthAsync(
        string connectionId,
        CancellationToken cancellationToken = default);
}

internal sealed class PluginConfigurationService(
    IInstalledPluginStore installed,
    PluginCredentialVault credentials,
    PluginOAuthBroker oauth,
    ConnectorRuntimeContext runtime) : IPluginConfigurationService
{
    private const int MaximumManifestBytes = 4 * 1024 * 1024;
    private const int MaximumAppManifestBytes = 256 * 1024;
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    public async Task<PluginConfigurationSnapshot> GetAsync(
        string pluginId,
        CancellationToken cancellationToken = default)
    {
        var state = await RequireStateAsync(cancellationToken).ConfigureAwait(false);
        var (record, manifest) = await LoadManifestAsync(pluginId, cancellationToken).ConfigureAwait(false);
        var metadata = await credentials.ListAsync(
            state.User.Id,
            state.DeviceId,
            record.PluginId,
            record.ReleaseId,
            cancellationToken).ConfigureAwait(false);
        var configured = metadata.ToDictionary(
            value => $"{value.Scope.ComponentKey}\n{value.Scope.SecretName}",
            StringComparer.Ordinal);
        var components = manifest.McpServers.OrderBy(value => value.Key, StringComparer.Ordinal)
            .Select(value =>
            {
                var names = value.Value.Environment.Values
                    .Concat(value.Value.Headers.Values)
                    .Select(PluginCredentialTemplate.Parse)
                    .Where(template => template.SecretName is not null)
                    .Select(template => template.SecretName!)
                    .Distinct(StringComparer.Ordinal)
                    .Order(StringComparer.Ordinal)
                    .Select(name =>
                    {
                        configured.TryGetValue($"{value.Key}\n{name}", out var existing);
                        return new PluginCredentialConfiguration(
                            value.Key,
                            name,
                            existing is not null,
                            existing?.UpdatedAt);
                    })
                    .ToArray();
                var permissions = manifest.Permissions
                    .Where(permission => permission.Components.Count == 0 ||
                        permission.Components.Contains(value.Key, StringComparer.Ordinal))
                    .OrderByDescending(permission => permission.Required)
                    .ThenBy(permission => permission.Permission, StringComparer.Ordinal)
                    .Select(permission => new PluginPermissionConfiguration(
                        permission.Permission,
                        permission.Required,
                        permission.Reason))
                    .ToArray();
                return new PluginMcpComponentConfiguration(
                    value.Key,
                    value.Value.EffectiveTransport,
                    value.Value.OAuthResource,
                    permissions,
                    names);
            })
            .ToArray();
        var connections = await oauth.ListConnectionsAsync(
            state.User.Id,
            state.DeviceId,
            record.PluginId,
            cancellationToken).ConfigureAwait(false);
        var apps = new List<PluginOAuthAppConfiguration>();
        foreach (var reference in manifest.Apps.OrderBy(value => value.ComponentKey, StringComparer.Ordinal))
        {
            var app = await LoadAppAsync(record, reference, cancellationToken).ConfigureAwait(false);
            apps.Add(new PluginOAuthAppConfiguration(
                reference.ComponentKey,
                app.Provider,
                app.Resource,
                app.Scopes,
                connections.FirstOrDefault(connection =>
                    string.Equals(connection.ComponentKey, reference.ComponentKey, StringComparison.Ordinal) &&
                    string.Equals(connection.Provider, app.Provider, StringComparison.Ordinal))));
        }

        return new PluginConfigurationSnapshot(
            record.PluginId,
            record.ReleaseId,
            record.Version,
            components,
            apps);
    }

    public async Task SetCredentialAsync(
        string pluginId,
        string componentKey,
        string secretName,
        string value,
        CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrEmpty(value))
        {
            throw new ArgumentException("Plugin credential cannot be empty.", nameof(value));
        }

        var state = await RequireStateAsync(cancellationToken).ConfigureAwait(false);
        var (record, manifest) = await LoadManifestAsync(pluginId, cancellationToken).ConfigureAwait(false);
        ValidateDeclaredCredential(manifest, componentKey, secretName);
        await credentials.UpsertAsync(new PluginCredentialScope(
            state.User.Id,
            state.DeviceId,
            record.PluginId,
            record.ReleaseId,
            componentKey,
            secretName), value, cancellationToken).ConfigureAwait(false);
    }

    public async Task DeleteCredentialAsync(
        string pluginId,
        string componentKey,
        string secretName,
        CancellationToken cancellationToken = default)
    {
        var state = await RequireStateAsync(cancellationToken).ConfigureAwait(false);
        var (record, manifest) = await LoadManifestAsync(pluginId, cancellationToken).ConfigureAwait(false);
        ValidateDeclaredCredential(manifest, componentKey, secretName);
        await credentials.DeleteAsync(new PluginCredentialScope(
            state.User.Id,
            state.DeviceId,
            record.PluginId,
            record.ReleaseId,
            componentKey,
            secretName), cancellationToken).ConfigureAwait(false);
    }

    public async Task<PluginOAuthAuthorizationStart> BeginOAuthAsync(
        string pluginId,
        string componentKey,
        CancellationToken cancellationToken = default)
    {
        var state = await RequireStateAsync(cancellationToken).ConfigureAwait(false);
        var (record, manifest) = await LoadManifestAsync(pluginId, cancellationToken).ConfigureAwait(false);
        if (!manifest.Apps.Any(value => string.Equals(
                value.ComponentKey,
                componentKey,
                StringComparison.Ordinal)))
        {
            throw new PluginRuntimeException("Plugin Connected App was not found.");
        }

        return await oauth.BeginAuthorizationAsync(
            state.User.Id,
            state.DeviceId,
            record.PluginId,
            record.ReleaseId,
            componentKey,
            cancellationToken).ConfigureAwait(false);
    }

    public Task DisconnectOAuthAsync(
        string connectionId,
        CancellationToken cancellationToken = default) =>
        oauth.DisconnectAsync(connectionId, cancellationToken);

    private async Task<ConnectorPersistentState> RequireStateAsync(CancellationToken cancellationToken)
    {
        await runtime.InitializeAsync(cancellationToken).ConfigureAwait(false);
        return runtime.Snapshot.State
            ?? throw new InvalidOperationException("Local Connector is not paired.");
    }

    private async Task<(InstalledPluginRecord Record, PluginManifest Manifest)> LoadManifestAsync(
        string pluginId,
        CancellationToken cancellationToken)
    {
        var record = await installed.GetAsync(pluginId, cancellationToken).ConfigureAwait(false)
            ?? throw new KeyNotFoundException("Plugin is not installed.");
        var path = Path.Combine(record.InstallationPath, "chatos.plugin.json");
        VerifyChecksum(record, "chatos.plugin.json", path);
        var manifest = await ReadJsonAsync<PluginManifest>(
            path,
            MaximumManifestBytes,
            cancellationToken).ConfigureAwait(false);
        if (manifest.SchemaVersion != 3 ||
            !string.Equals(manifest.Version, record.Version, StringComparison.Ordinal))
        {
            throw new PluginRuntimeException("Plugin manifest does not match the installed Release.");
        }

        return (record, manifest);
    }

    private static async Task<OAuthAppSummary> LoadAppAsync(
        InstalledPluginRecord record,
        PluginConnectedApp reference,
        CancellationToken cancellationToken)
    {
        var relative = NormalizeRelativePath(reference.Manifest.Path);
        var path = ResolveRegularFile(record.InstallationPath, relative);
        VerifyChecksum(record, relative, path);
        return await ReadJsonAsync<OAuthAppSummary>(
            path,
            MaximumAppManifestBytes,
            cancellationToken).ConfigureAwait(false);
    }

    private static void ValidateDeclaredCredential(
        PluginManifest manifest,
        string componentKey,
        string secretName)
    {
        if (!manifest.McpServers.TryGetValue(componentKey, out var server))
        {
            throw new PluginRuntimeException("Plugin MCP component was not found.");
        }

        var declared = server.Environment.Values.Concat(server.Headers.Values)
            .Select(PluginCredentialTemplate.Parse)
            .Any(value => string.Equals(value.SecretName, secretName, StringComparison.Ordinal));
        if (!declared)
        {
            throw new PluginRuntimeException("Plugin credential is not declared by this component.");
        }
    }

    private static async Task<T> ReadJsonAsync<T>(
        string path,
        int maximumBytes,
        CancellationToken cancellationToken)
    {
        var info = new FileInfo(path);
        if (!info.Exists || info.Length <= 0 || info.Length > maximumBytes || IsReparsePoint(path))
        {
            throw new PluginRuntimeException("Plugin configuration metadata is missing or unsafe.");
        }

        await using var stream = new FileStream(
            path,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read,
            16 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        try
        {
            return await JsonSerializer.DeserializeAsync<T>(stream, JsonOptions, cancellationToken)
                .ConfigureAwait(false) ?? throw new JsonException("Plugin configuration is empty.");
        }
        catch (JsonException exception)
        {
            throw new PluginRuntimeException("Plugin configuration metadata is invalid.", exception);
        }
    }

    private static string ResolveRegularFile(string root, string relative)
    {
        var rootPath = Path.TrimEndingDirectorySeparator(Path.GetFullPath(root));
        var path = Path.GetFullPath(Path.Combine(
            rootPath,
            relative.Replace('/', Path.DirectorySeparatorChar)));
        var comparison = OperatingSystem.IsWindows()
            ? StringComparison.OrdinalIgnoreCase
            : StringComparison.Ordinal;
        if (!path.StartsWith(rootPath + Path.DirectorySeparatorChar, comparison) ||
            !File.Exists(path) || IsReparsePoint(path))
        {
            throw new PluginRuntimeException("Plugin configuration path is unsafe.");
        }

        return path;
    }

    private static string NormalizeRelativePath(string? value)
    {
        var normalized = value?.Replace('\\', '/').Trim() ?? string.Empty;
        while (normalized.StartsWith("./", StringComparison.Ordinal))
        {
            normalized = normalized[2..];
        }

        var parts = normalized.Split('/', StringSplitOptions.None);
        if (parts.Length == 0 || parts.Any(part =>
                part.Length == 0 || part is "." or ".." || part.Contains(':') || part.Contains('\0')))
        {
            throw new PluginRuntimeException("Plugin configuration path is invalid.");
        }

        return string.Join('/', parts);
    }

    private static void VerifyChecksum(
        InstalledPluginRecord record,
        string relative,
        string path)
    {
        if (record.PackageFileSha256 is null ||
            !record.PackageFileSha256.TryGetValue(relative, out var expected) ||
            expected.Length != 64 || expected.Any(value => !Uri.IsHexDigit(value)))
        {
            throw new PluginRuntimeException(
                "Plugin configuration file is not covered by installation checksums.");
        }

        var actual = Convert.ToHexString(SHA256.HashData(File.ReadAllBytes(path))).ToLowerInvariant();
        if (!CryptographicOperations.FixedTimeEquals(
                Convert.FromHexString(expected),
                Convert.FromHexString(actual)))
        {
            throw new PluginRuntimeException("Plugin configuration checksum changed after installation.");
        }
    }

    private static bool IsReparsePoint(string path) =>
        (File.GetAttributes(path) & FileAttributes.ReparsePoint) != 0;

    private sealed record OAuthAppSummary
    {
        [JsonPropertyName("provider")]
        public required string Provider { get; init; }
        [JsonPropertyName("resource")]
        public required string Resource { get; init; }
        [JsonPropertyName("scopes")]
        public IReadOnlyList<string> Scopes { get; init; } = [];
    }
}
