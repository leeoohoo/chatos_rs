using System.Formats.Tar;
using System.IO.Compression;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text.Json;
using ChatOS.Connector.Gateway;

namespace ChatOS.Connector.Plugins;

public sealed class WindowsPluginPackageInstaller
{
    internal const long MaximumPackageBytes = 256L * 1024 * 1024;
    internal const int MaximumEntries = 8_192;
    internal const long MaximumFileBytes = 128L * 1024 * 1024;
    internal const long MaximumUnpackedBytes = 768L * 1024 * 1024;
    private const int MaximumPathBytes = 512;
    private const int MaximumPathDepth = 48;
    private const int MaximumPackageJsonBytes = 1024 * 1024;
    private const int MaximumManifestBytes = 4 * 1024 * 1024;
    private static readonly StringComparer PathComparer = StringComparer.OrdinalIgnoreCase;
    private static readonly HashSet<string> ReservedWindowsNames = new(StringComparer.OrdinalIgnoreCase)
    {
        "CON", "PRN", "AUX", "NUL", "CLOCK$",
        "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
        "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    };
    private static readonly JsonSerializerOptions JsonOptions = new(JsonSerializerDefaults.Web)
    {
        PropertyNameCaseInsensitive = true,
    };

    private readonly string _rootPath;

    public WindowsPluginPackageInstaller()
        : this(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ChatOS",
            "WindowsClient",
            "Plugins"))
    {
    }

    internal WindowsPluginPackageInstaller(string rootPath)
    {
        _rootPath = Path.GetFullPath(rootPath);
    }

    public async Task<InstalledPluginRecord> InstallAsync(
        ConnectorPluginSource source,
        string archivePath,
        CancellationToken cancellationToken = default)
    {
        var version = Required(source.Release.Version, "Plugin Release is missing a version.");
        var expectedSha256 = NormalizeSha256(source.Release.ArtifactSha256);
        var npm = source.Release.NpmPackage
            ?? throw new PluginPackageException("Plugin Release is missing npm package integrity metadata.");
        if (!string.Equals(npm.Version, version, StringComparison.Ordinal))
        {
            throw new PluginPackageException("npm package version does not match the Plugin Release.");
        }

        ValidateSupportedPlatform(source.Release.SupportedPlatforms);
        Directory.CreateDirectory(_rootPath);
        var fullArchivePath = Path.GetFullPath(archivePath);
        var archiveInfo = new FileInfo(fullArchivePath);
        if (!archiveInfo.Exists || archiveInfo.Length > MaximumPackageBytes)
        {
            throw new PluginPackageException("Plugin package is missing or exceeds the 256 MB limit.");
        }

        var actualSha256 = await HashFileHexAsync(HashAlgorithmName.SHA256, fullArchivePath, cancellationToken)
            .ConfigureAwait(false);
        if (!CryptographicOperations.FixedTimeEquals(
                Convert.FromHexString(expectedSha256),
                Convert.FromHexString(actualSha256)))
        {
            throw new PluginPackageException("Plugin package SHA-256 verification failed.");
        }

        await VerifyNpmIntegrityAsync(npm.Integrity, fullArchivePath, cancellationToken).ConfigureAwait(false);

        var stagingRoot = Path.Combine(_rootPath, ".staging", Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(stagingRoot);
        try
        {
            var extraction = await ExtractVerifiedAsync(fullArchivePath, stagingRoot, cancellationToken)
                .ConfigureAwait(false);
            var packageRoot = Path.Combine(stagingRoot, "package");
            if (!Directory.Exists(packageRoot))
            {
                throw new PluginPackageException("npm package does not contain the package directory.");
            }

            var package = await ReadPackageJsonAsync(packageRoot, cancellationToken).ConfigureAwait(false);
            if (!string.Equals(package.Name, npm.Name, StringComparison.Ordinal) ||
                !string.Equals(package.Version, version, StringComparison.Ordinal))
            {
                throw new PluginPackageException("package.json identity does not match the signed Plugin Release.");
            }

            ValidatePackagePlatform(package);
            var manifest = await ReadManifestAsync(packageRoot, cancellationToken).ConfigureAwait(false);
            ValidateManifest(source, manifest, version);
            ValidateDeclaredBins(package, manifest, extraction.Paths);

            var pluginDirectory = Path.Combine(_rootPath, Sha256Text(source.Catalog.Id));
            var finalPath = Path.Combine(pluginDirectory, SafeVersionDirectory(version));
            var backupPath = Path.Combine(
                _rootPath,
                $".backup-{Path.GetFileName(pluginDirectory)}-{Guid.NewGuid():N}");
            var hadPrevious = Directory.Exists(pluginDirectory);
            if (hadPrevious)
            {
                Directory.Move(pluginDirectory, backupPath);
            }

            try
            {
                Directory.CreateDirectory(pluginDirectory);
                Directory.Move(packageRoot, finalPath);
                if (hadPrevious)
                {
                    Directory.Delete(backupPath, recursive: true);
                }
            }
            catch
            {
                TryDeleteDirectory(pluginDirectory);
                if (hadPrevious && Directory.Exists(backupPath))
                {
                    Directory.Move(backupPath, pluginDirectory);
                }

                throw;
            }

            return new InstalledPluginRecord(
                source.Catalog.Id,
                source.Release.Id,
                version,
                expectedSha256,
                finalPath,
                DateTimeOffset.UtcNow,
                manifest.Permissions.Select(static permission => permission.Permission).Distinct().Order().ToArray(),
                extraction.FileSha256);
        }
        catch (PluginPackageException)
        {
            throw;
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            throw new PluginPackageException("Plugin package installation failed.", exception);
        }
        finally
        {
            TryDeleteDirectory(stagingRoot);
        }
    }

    public Task UninstallAsync(string pluginId, CancellationToken cancellationToken = default)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var directory = Path.Combine(_rootPath, Sha256Text(pluginId));
        EnsureChildPath(directory);
        TryDeleteDirectory(directory);
        return Task.CompletedTask;
    }

    private static async Task<VerifiedExtraction> ExtractVerifiedAsync(
        string archivePath,
        string stagingRoot,
        CancellationToken cancellationToken)
    {
        var seen = new HashSet<string>(PathComparer);
        var fileSha256 = new Dictionary<string, string>(PathComparer);
        long unpackedBytes = 0;
        var entries = 0;
        await using var archive = new FileStream(
            archivePath,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read,
            64 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        await using var gzip = new GZipStream(archive, CompressionMode.Decompress, leaveOpen: false);
        using var reader = new TarReader(gzip, leaveOpen: false);
        while (await reader.GetNextEntryAsync(copyData: false, cancellationToken).ConfigureAwait(false) is { } entry)
        {
            entries++;
            if (entries > MaximumEntries)
            {
                throw new PluginPackageException("Plugin package contains too many entries.");
            }

            if (entry.EntryType is not (TarEntryType.RegularFile or TarEntryType.V7RegularFile or TarEntryType.Directory))
            {
                throw new PluginPackageException("Plugin package contains a link or special file.");
            }

            var relative = NormalizeArchivePath(entry.Name, entry.EntryType is TarEntryType.Directory);
            if (relative.Length == 0)
            {
                continue;
            }

            if (!seen.Add(relative))
            {
                throw new PluginPackageException($"Plugin package contains a duplicate or case-colliding path: {relative}");
            }

            var outputPath = Path.GetFullPath(Path.Combine(stagingRoot, relative.Replace('/', Path.DirectorySeparatorChar)));
            EnsureChildPath(stagingRoot, outputPath);
            if (entry.EntryType is TarEntryType.Directory)
            {
                Directory.CreateDirectory(outputPath);
                continue;
            }

            if (entry.Length < 0 || entry.Length > MaximumFileBytes)
            {
                throw new PluginPackageException($"Plugin package entry exceeds the 128 MB limit: {relative}");
            }

            unpackedBytes = checked(unpackedBytes + entry.Length);
            if (unpackedBytes > MaximumUnpackedBytes)
            {
                throw new PluginPackageException("Plugin package exceeds the 768 MB unpacked size limit.");
            }

            Directory.CreateDirectory(Path.GetDirectoryName(outputPath)!);
            await using var output = new FileStream(
                outputPath,
                FileMode.CreateNew,
                FileAccess.Write,
                FileShare.None,
                64 * 1024,
                FileOptions.Asynchronous | FileOptions.SequentialScan);
            var input = entry.DataStream
                ?? throw new PluginPackageException($"Plugin package entry has no data: {relative}");
            var copied = await CopyBoundedAsync(input, output, entry.Length, cancellationToken).ConfigureAwait(false);
            if (copied.Length != entry.Length)
            {
                throw new PluginPackageException($"Plugin package entry size does not match its header: {relative}");
            }

            fileSha256[relative["package/".Length..]] = copied.Sha256;
        }

        if (entries == 0)
        {
            throw new PluginPackageException("Plugin package is empty.");
        }

        return new VerifiedExtraction(seen, fileSha256);
    }

    private static async Task<CopiedFile> CopyBoundedAsync(
        Stream source,
        Stream destination,
        long declaredLength,
        CancellationToken cancellationToken)
    {
        var buffer = new byte[64 * 1024];
        long total = 0;
        using var hash = IncrementalHash.CreateHash(HashAlgorithmName.SHA256);
        while (true)
        {
            var read = await source.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (read == 0)
            {
                break;
            }

            total = checked(total + read);
            if (total > declaredLength || total > MaximumFileBytes)
            {
                throw new PluginPackageException("Plugin package entry expanded beyond its declared size.");
            }

            await destination.WriteAsync(buffer.AsMemory(0, read), cancellationToken).ConfigureAwait(false);
            hash.AppendData(buffer, 0, read);
        }

        return new CopiedFile(
            total,
            Convert.ToHexString(hash.GetHashAndReset()).ToLowerInvariant());
    }

    private static string NormalizeArchivePath(string rawPath, bool directory)
    {
        var path = rawPath.Replace('\\', '/');
        while (path.StartsWith("./", StringComparison.Ordinal))
        {
            path = path[2..];
        }

        path = path.TrimEnd('/');
        if (path.Length == 0)
        {
            return string.Empty;
        }

        if (path[0] == '/' ||
            path.Contains('\0') ||
            System.Text.Encoding.UTF8.GetByteCount(path) > MaximumPathBytes)
        {
            throw new PluginPackageException("Plugin package contains an invalid path.");
        }

        var components = path.Split('/', StringSplitOptions.None);
        if (components.Length > MaximumPathDepth ||
            !string.Equals(components[0], "package", StringComparison.Ordinal))
        {
            throw new PluginPackageException($"Plugin package entry is outside package/: {rawPath}");
        }

        foreach (var component in components)
        {
            ValidateWindowsPathComponent(component);
        }

        if (!directory && components.Length == 1)
        {
            throw new PluginPackageException("Plugin package root must be a directory.");
        }

        return string.Join('/', components);
    }

    private static void ValidateWindowsPathComponent(string component)
    {
        if (component.Length == 0 || component is "." or ".." ||
            component.EndsWith(' ') || component.EndsWith('.') ||
            component.IndexOfAny(['<', '>', ':', '"', '|', '?', '*']) >= 0 ||
            component.Any(static character => char.IsControl(character)))
        {
            throw new PluginPackageException("Plugin package contains a Windows-unsafe path.");
        }

        var stem = component.Split('.')[0];
        if (ReservedWindowsNames.Contains(stem))
        {
            throw new PluginPackageException("Plugin package contains a reserved Windows path.");
        }
    }

    private static async Task<NpmPackageJson> ReadPackageJsonAsync(
        string packageRoot,
        CancellationToken cancellationToken)
    {
        var path = Path.Combine(packageRoot, "package.json");
        var bytes = await ReadBoundedFileAsync(path, MaximumPackageJsonBytes, cancellationToken).ConfigureAwait(false);
        try
        {
            return JsonSerializer.Deserialize<NpmPackageJson>(bytes, JsonOptions)
                ?? throw new JsonException("package.json is empty.");
        }
        catch (JsonException exception)
        {
            throw new PluginPackageException("Plugin package.json is invalid.", exception);
        }
    }

    private static async Task<PluginManifest> ReadManifestAsync(
        string packageRoot,
        CancellationToken cancellationToken)
    {
        var path = Path.Combine(packageRoot, "chatos.plugin.json");
        var bytes = await ReadBoundedFileAsync(path, MaximumManifestBytes, cancellationToken).ConfigureAwait(false);
        try
        {
            return JsonSerializer.Deserialize<PluginManifest>(bytes, JsonOptions)
                ?? throw new JsonException("Plugin manifest is empty.");
        }
        catch (JsonException exception)
        {
            throw new PluginPackageException("Plugin manifest is invalid.", exception);
        }
    }

    private static async Task<byte[]> ReadBoundedFileAsync(
        string path,
        int maximumBytes,
        CancellationToken cancellationToken)
    {
        var info = new FileInfo(path);
        if (!info.Exists || info.Length > maximumBytes)
        {
            throw new PluginPackageException($"Required Plugin metadata is missing or exceeds {maximumBytes} bytes.");
        }

        return await File.ReadAllBytesAsync(path, cancellationToken).ConfigureAwait(false);
    }

    private static void ValidateManifest(
        ConnectorPluginSource source,
        PluginManifest manifest,
        string version)
    {
        if (manifest.SchemaVersion != 3 ||
            !string.Equals(manifest.Version, version, StringComparison.Ordinal) ||
            string.IsNullOrWhiteSpace(manifest.Name))
        {
            throw new PluginPackageException("Plugin manifest identity does not match the Release.");
        }

        if (!string.IsNullOrWhiteSpace(source.Catalog.Name) &&
            !string.Equals(source.Catalog.Name, manifest.Name, StringComparison.Ordinal))
        {
            throw new PluginPackageException("Plugin manifest name does not match the catalog.");
        }

        if (manifest.McpServers.Count == 0 && manifest.Skills.Count == 0)
        {
            throw new PluginPackageException("Plugin manifest has no runnable component.");
        }

        var componentKeys = manifest.McpServers.Keys.ToHashSet(StringComparer.Ordinal);
        foreach (var pair in manifest.McpServers)
        {
            if (string.IsNullOrWhiteSpace(pair.Key) || pair.Key.Length > 128)
            {
                throw new PluginPackageException("Plugin MCP component key is invalid.");
            }

            if (pair.Value.EffectiveTransport is not ("stdio" or "http"))
            {
                throw new PluginPackageException($"Plugin MCP transport is unsupported: {pair.Key}");
            }

            if (pair.Value.EffectiveTransport == "stdio" && !SafeExecutableName(pair.Value.Bin))
            {
                throw new PluginPackageException($"Plugin MCP executable name is invalid: {pair.Key}");
            }
        }

        var permissions = new HashSet<string>(StringComparer.Ordinal);
        foreach (var permission in manifest.Permissions)
        {
            if (!SafePermissionName(permission.Permission) || !permissions.Add(permission.Permission))
            {
                throw new PluginPackageException("Plugin permission declaration is invalid or duplicated.");
            }

            if (permission.Components.Any(component => !componentKeys.Contains(component)))
            {
                throw new PluginPackageException("Plugin permission references an unknown component.");
            }
        }

        if (manifest.McpServers.Values.Any(server => server.EffectiveTransport == "stdio") &&
            !manifest.Permissions.Any(permission => permission.Required && permission.Permission == "process.spawn"))
        {
            throw new PluginPackageException("stdio MCP plugins must require process.spawn permission.");
        }

        if (manifest.Dependencies.SupportedPlatforms.Count > 0 &&
            !PlatformSetsEqual(manifest.Dependencies.SupportedPlatforms, source.Release.SupportedPlatforms))
        {
            throw new PluginPackageException("Plugin manifest platform constraints do not match the Release.");
        }
    }

    private static void ValidateDeclaredBins(
        NpmPackageJson package,
        PluginManifest manifest,
        HashSet<string> extractedFiles)
    {
        var bins = package.Bins();
        if (bins.Count == 0)
        {
            throw new PluginPackageException("package.json does not publish a Plugin executable.");
        }

        foreach (var server in manifest.McpServers.Values.Where(server => server.EffectiveTransport == "stdio"))
        {
            var bin = server.Bin!;
            if (!bins.TryGetValue(bin, out var target))
            {
                throw new PluginPackageException($"package.json does not publish MCP bin: {bin}");
            }

            var normalizedTarget = NormalizePackageRelativePath(target);
            if (!extractedFiles.Contains($"package/{normalizedTarget}"))
            {
                throw new PluginPackageException($"Published MCP bin is missing from the package: {bin}");
            }
        }
    }

    private static string NormalizePackageRelativePath(string value)
    {
        var normalized = value.Replace('\\', '/').Trim();
        while (normalized.StartsWith("./", StringComparison.Ordinal))
        {
            normalized = normalized[2..];
        }

        var components = normalized.Split('/', StringSplitOptions.None);
        if (components.Length == 0 || components.Any(component => component.Length == 0 || component is "." or ".."))
        {
            throw new PluginPackageException("package.json contains an invalid executable path.");
        }

        foreach (var component in components)
        {
            ValidateWindowsPathComponent(component);
        }

        return string.Join('/', components);
    }

    private static void ValidateSupportedPlatform(IReadOnlyList<string> supportedPlatforms)
    {
        if (supportedPlatforms.Count == 0)
        {
            return;
        }

        var current = CurrentPlatformAliases();
        if (!supportedPlatforms.Any(platform => current.Contains(platform.Trim())))
        {
            throw new PluginPackageException(
                $"Plugin Release does not support this Windows architecture ({RuntimeInformation.ProcessArchitecture}).");
        }
    }

    private static void ValidatePackagePlatform(NpmPackageJson package)
    {
        if (!AllowsNpmConstraint(package.OperatingSystems, ["win32", "windows"]))
        {
            throw new PluginPackageException("npm package does not support Windows.");
        }

        var architectures = RuntimeInformation.ProcessArchitecture switch
        {
            Architecture.X64 => new[] { "x64", "x86_64" },
            Architecture.Arm64 => new[] { "arm64", "aarch64" },
            Architecture.X86 => new[] { "x86", "ia32", "i686" },
            _ => new[] { RuntimeInformation.ProcessArchitecture.ToString().ToLowerInvariant() },
        };
        if (!AllowsNpmConstraint(package.Cpu, architectures))
        {
            throw new PluginPackageException("npm package does not support this Windows architecture.");
        }
    }

    private static bool AllowsNpmConstraint(JsonElement value, IReadOnlyCollection<string> aliases)
    {
        var constraints = StringValues(value);
        if (constraints.Count == 0)
        {
            return true;
        }

        if (constraints.Any(item => item.StartsWith('!') && aliases.Contains(item[1..], StringComparer.OrdinalIgnoreCase)))
        {
            return false;
        }

        var positive = constraints.Where(item => !item.StartsWith('!')).ToArray();
        return positive.Length == 0 || positive.Any(item => aliases.Contains(item, StringComparer.OrdinalIgnoreCase));
    }

    private static IReadOnlyList<string> StringValues(JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.String)
        {
            return [value.GetString() ?? string.Empty];
        }

        if (value.ValueKind == JsonValueKind.Array)
        {
            return value.EnumerateArray()
                .Where(item => item.ValueKind == JsonValueKind.String)
                .Select(item => item.GetString() ?? string.Empty)
                .ToArray();
        }

        return Array.Empty<string>();
    }

    private static HashSet<string> CurrentPlatformAliases()
    {
        var values = new HashSet<string>(StringComparer.OrdinalIgnoreCase) { "windows" };
        switch (RuntimeInformation.ProcessArchitecture)
        {
            case Architecture.X64:
                values.UnionWith(["windows-x86_64", "windows-x64"]);
                break;
            case Architecture.Arm64:
                values.UnionWith(["windows-aarch64", "windows-arm64"]);
                break;
            case Architecture.X86:
                values.UnionWith(["windows-i686", "windows-x86"]);
                break;
        }

        return values;
    }

    private static bool PlatformSetsEqual(IReadOnlyList<string> left, IReadOnlyList<string> right) =>
        left.ToHashSet(StringComparer.OrdinalIgnoreCase).SetEquals(right);

    private static bool SafeExecutableName(string? value) =>
        !string.IsNullOrWhiteSpace(value) &&
        value.Length <= 128 &&
        value is not "." and not ".." &&
        value.IndexOfAny(['/', '\\', ':']) < 0 &&
        !ReservedWindowsNames.Contains(Path.GetFileNameWithoutExtension(value));

    private static bool SafePermissionName(string value) =>
        !string.IsNullOrWhiteSpace(value) &&
        value.Length <= 128 &&
        value.All(static character =>
            char.IsAsciiLetterOrDigit(character) || character is '.' or '-' or '_' or ':');

    private static async Task VerifyNpmIntegrityAsync(
        string integrity,
        string archivePath,
        CancellationToken cancellationToken)
    {
        const string prefix = "sha512-";
        if (!integrity.StartsWith(prefix, StringComparison.Ordinal))
        {
            throw new PluginPackageException("npm integrity must use sha512.");
        }

        byte[] expected;
        try
        {
            expected = Convert.FromBase64String(integrity[prefix.Length..]);
        }
        catch (FormatException exception)
        {
            throw new PluginPackageException("npm integrity is invalid.", exception);
        }

        var actual = await HashFileAsync(HashAlgorithmName.SHA512, archivePath, cancellationToken)
            .ConfigureAwait(false);
        if (expected.Length != actual.Length || !CryptographicOperations.FixedTimeEquals(expected, actual))
        {
            throw new PluginPackageException("npm sha512 integrity verification failed.");
        }
    }

    private static async Task<string> HashFileHexAsync(
        HashAlgorithmName algorithm,
        string path,
        CancellationToken cancellationToken) =>
        Convert.ToHexString(await HashFileAsync(algorithm, path, cancellationToken).ConfigureAwait(false))
            .ToLowerInvariant();

    private static async Task<byte[]> HashFileAsync(
        HashAlgorithmName algorithm,
        string path,
        CancellationToken cancellationToken)
    {
        await using var stream = new FileStream(
            path,
            FileMode.Open,
            FileAccess.Read,
            FileShare.Read,
            64 * 1024,
            FileOptions.Asynchronous | FileOptions.SequentialScan);
        using var hash = IncrementalHash.CreateHash(algorithm);
        var buffer = new byte[64 * 1024];
        while (true)
        {
            var read = await stream.ReadAsync(buffer, cancellationToken).ConfigureAwait(false);
            if (read == 0)
            {
                break;
            }

            hash.AppendData(buffer, 0, read);
        }

        return hash.GetHashAndReset();
    }

    private static string NormalizeSha256(string? value)
    {
        var normalized = Required(value, "Plugin Release is missing SHA-256 metadata.").ToLowerInvariant();
        if (normalized.Length != 64 || !normalized.All(Uri.IsHexDigit))
        {
            throw new PluginPackageException("Plugin Release SHA-256 metadata is invalid.");
        }

        return normalized;
    }

    private static string Required(string? value, string message)
    {
        var trimmed = value?.Trim();
        return string.IsNullOrEmpty(trimmed) ? throw new PluginPackageException(message) : trimmed;
    }

    private static string Sha256Text(string value) =>
        Convert.ToHexString(SHA256.HashData(System.Text.Encoding.UTF8.GetBytes(value))).ToLowerInvariant();

    private static string SafeVersionDirectory(string version)
    {
        var digest = Sha256Text(version)[..12];
        var safe = new string(version.Select(character =>
            char.IsAsciiLetterOrDigit(character) || character is '.' or '-' or '_' ? character : '-').ToArray());
        safe = safe.Trim('.', ' ', '-');
        if (safe.Length > 64)
        {
            safe = safe[..64];
        }

        return string.IsNullOrEmpty(safe) ? digest : $"{safe}-{digest}";
    }

    private void EnsureChildPath(string path) => EnsureChildPath(_rootPath, path);

    private static void EnsureChildPath(string root, string path)
    {
        var normalizedRoot = Path.GetFullPath(root).TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar;
        var normalizedPath = Path.GetFullPath(path);
        if (!normalizedPath.StartsWith(normalizedRoot, StringComparison.OrdinalIgnoreCase))
        {
            throw new PluginPackageException("Plugin path escapes the managed installation root.");
        }
    }

    private static void TryDeleteDirectory(string path)
    {
        try
        {
            if (Directory.Exists(path))
            {
                Directory.Delete(path, recursive: true);
            }
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    private sealed record NpmPackageJson
    {
        [System.Text.Json.Serialization.JsonPropertyName("name")]
        public required string Name { get; init; }

        [System.Text.Json.Serialization.JsonPropertyName("version")]
        public required string Version { get; init; }

        [System.Text.Json.Serialization.JsonPropertyName("bin")]
        public JsonElement Bin { get; init; }

        [System.Text.Json.Serialization.JsonPropertyName("os")]
        public JsonElement OperatingSystems { get; init; }

        [System.Text.Json.Serialization.JsonPropertyName("cpu")]
        public JsonElement Cpu { get; init; }

        public IReadOnlyDictionary<string, string> Bins()
        {
            if (Bin.ValueKind == JsonValueKind.String)
            {
                return new Dictionary<string, string>(StringComparer.Ordinal)
                {
                    [UnscopedPackageName(Name)] = Bin.GetString() ?? string.Empty,
                };
            }

            if (Bin.ValueKind == JsonValueKind.Object)
            {
                return Bin.EnumerateObject()
                    .Where(property => property.Value.ValueKind == JsonValueKind.String)
                    .ToDictionary(
                        property => property.Name,
                        property => property.Value.GetString() ?? string.Empty,
                        StringComparer.Ordinal);
            }

            return new Dictionary<string, string>();
        }

        private static string UnscopedPackageName(string name)
        {
            var slash = name.LastIndexOf('/');
            return slash >= 0 ? name[(slash + 1)..] : name;
        }
    }

    private sealed record VerifiedExtraction(
        HashSet<string> Paths,
        IReadOnlyDictionary<string, string> FileSha256);

    private sealed record CopiedFile(long Length, string Sha256);
}
