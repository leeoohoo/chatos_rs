using System.Formats.Tar;
using System.IO.Compression;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text.Json;
using ChatOS.Connector.Gateway;

namespace ChatOS.Connector.Plugins;
public sealed partial class WindowsPluginPackageInstaller
{
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
