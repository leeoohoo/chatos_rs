using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Plugins;
internal sealed partial class PluginManifestLoader
{
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

    private static string ValidateHealthPath(string? value)
    {
        var path = value ?? "/api/health";
        if (!path.StartsWith('/') || path.Length > 2_048 || path.Contains("..", StringComparison.Ordinal) ||
            path.Contains('?') || path.Contains('#') || path.Contains('\0'))
        {
            throw new PluginRuntimeException("Plugin application health path is invalid.");
        }
        return path;
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
