using System.Formats.Tar;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Plugins;

namespace ChatOS.Connector.Tests;

public sealed class WindowsPluginPackageInstallerTests : IDisposable
{
    private readonly string _directory = Path.Combine(
        Path.GetTempPath(),
        $"chatos-plugin-installer-tests-{Guid.NewGuid():N}");

    [Fact]
    public async Task InstallsVerifiedWindowsPackageAndReplacesPreviousRelease()
    {
        Directory.CreateDirectory(_directory);
        var installer = new WindowsPluginPackageInstaller(Path.Combine(_directory, "plugins"));
        var first = CreatePackage("1.0.0", extraFiles: new Dictionary<string, byte[]>
        {
            ["package/first.txt"] = Encoding.UTF8.GetBytes("first"),
        });
        var firstRecord = await installer.InstallAsync(Source("1.0.0", first), first.Path);

        Assert.True(File.Exists(Path.Combine(firstRecord.InstallationPath, "bin", "test-plugin")));
        Assert.True(File.Exists(Path.Combine(firstRecord.InstallationPath, "first.txt")));
        Assert.Contains("process.spawn", firstRecord.DeclaredPermissions);

        var second = CreatePackage("1.1.0", extraFiles: new Dictionary<string, byte[]>
        {
            ["package/second.txt"] = Encoding.UTF8.GetBytes("second"),
        });
        var secondRecord = await installer.InstallAsync(Source("1.1.0", second), second.Path);

        Assert.False(Directory.Exists(firstRecord.InstallationPath));
        Assert.True(File.Exists(Path.Combine(secondRecord.InstallationPath, "second.txt")));
        Assert.DoesNotContain(Directory.EnumerateDirectories(Path.Combine(_directory, "plugins")),
            path => Path.GetFileName(path).StartsWith(".backup-", StringComparison.Ordinal));
    }

    [Fact]
    public async Task RejectsHashMismatchBeforeExtraction()
    {
        Directory.CreateDirectory(_directory);
        var installer = new WindowsPluginPackageInstaller(Path.Combine(_directory, "plugins"));
        var package = CreatePackage("1.0.0");
        var source = Source("1.0.0", package) with
        {
            Release = Source("1.0.0", package).Release with
            {
                ArtifactSha256 = new string('0', 64),
            },
        };

        var error = await Assert.ThrowsAsync<PluginPackageException>(() =>
            installer.InstallAsync(source, package.Path));

        Assert.Contains("SHA-256", error.Message, StringComparison.Ordinal);
        Assert.Empty(Directory.Exists(Path.Combine(_directory, "plugins"))
            ? Directory.EnumerateDirectories(Path.Combine(_directory, "plugins"))
                .Where(path => !Path.GetFileName(path).StartsWith(".", StringComparison.Ordinal))
            : Array.Empty<string>());
    }

    [Fact]
    public async Task RejectsLinksAndCaseCollidingPaths()
    {
        Directory.CreateDirectory(_directory);
        var installer = new WindowsPluginPackageInstaller(Path.Combine(_directory, "plugins"));
        var linked = CreatePackage("1.0.0", symbolicLink: true);
        await Assert.ThrowsAsync<PluginPackageException>(() =>
            installer.InstallAsync(Source("1.0.0", linked), linked.Path));

        var colliding = CreatePackage("1.0.0", extraFiles: new Dictionary<string, byte[]>
        {
            ["package/Readme.txt"] = Encoding.UTF8.GetBytes("one"),
            ["package/README.TXT"] = Encoding.UTF8.GetBytes("two"),
        });
        var error = await Assert.ThrowsAsync<PluginPackageException>(() =>
            installer.InstallAsync(Source("1.0.0", colliding), colliding.Path));

        Assert.Contains("case-colliding", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task RejectsUnsupportedPlatformAndPermissionMismatch()
    {
        Directory.CreateDirectory(_directory);
        var installer = new WindowsPluginPackageInstaller(Path.Combine(_directory, "plugins"));
        var package = CreatePackage("1.0.0");
        var unsupported = Source("1.0.0", package) with
        {
            Release = Source("1.0.0", package).Release with
            {
                SupportedPlatforms = ["macos-arm64"],
            },
        };
        await Assert.ThrowsAsync<PluginPackageException>(() =>
            installer.InstallAsync(unsupported, package.Path));

        var invalidPermissions = CreatePackage("1.0.0", includeProcessSpawn: false);
        var error = await Assert.ThrowsAsync<PluginPackageException>(() =>
            installer.InstallAsync(Source("1.0.0", invalidPermissions), invalidPermissions.Path));
        Assert.Contains("process.spawn", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task UninstallOnlyRemovesHashedManagedDirectory()
    {
        Directory.CreateDirectory(_directory);
        var pluginRoot = Path.Combine(_directory, "plugins");
        var installer = new WindowsPluginPackageInstaller(pluginRoot);
        var package = CreatePackage("1.0.0");
        var record = await installer.InstallAsync(Source("1.0.0", package), package.Path);
        var unrelated = Path.Combine(_directory, "keep.txt");
        await File.WriteAllTextAsync(unrelated, "keep");

        await installer.UninstallAsync("plugin-1");

        Assert.False(Directory.Exists(Path.GetDirectoryName(record.InstallationPath)));
        Assert.True(File.Exists(unrelated));
    }

    public void Dispose()
    {
        try
        {
            if (Directory.Exists(_directory))
            {
                Directory.Delete(_directory, recursive: true);
            }
        }
        catch (IOException)
        {
        }
    }

    private PackageFixture CreatePackage(
        string version,
        IReadOnlyDictionary<string, byte[]>? extraFiles = null,
        bool symbolicLink = false,
        bool includeProcessSpawn = true)
    {
        Directory.CreateDirectory(_directory);
        var path = Path.Combine(_directory, $"package-{Guid.NewGuid():N}.tgz");
        var permissions = includeProcessSpawn
            ? """[{"permission":"process.spawn","required":true,"components":["main"]}]"""
            : "[]";
        var files = new Dictionary<string, byte[]>(StringComparer.Ordinal)
        {
            ["package/package.json"] = Encoding.UTF8.GetBytes(
                $"{{\"name\":\"test-plugin\",\"version\":\"{version}\",\"bin\":{{\"test-plugin\":\"bin/test-plugin\"}},\"os\":[\"win32\"],\"cpu\":[]}}"),
            ["package/chatos.plugin.json"] = Encoding.UTF8.GetBytes(
                $"{{\"schemaVersion\":3,\"name\":\"test-plugin\",\"version\":\"{version}\",\"mcpServers\":{{\"main\":{{\"type\":\"stdio\",\"bin\":\"test-plugin\"}}}},\"permissions\":{permissions},\"dependencies\":{{\"supportedPlatforms\":[\"windows\"]}}}}"),
            ["package/bin/test-plugin"] = Encoding.UTF8.GetBytes("test executable"),
        };
        if (extraFiles is not null)
        {
            foreach (var pair in extraFiles)
            {
                files.Add(pair.Key, pair.Value);
            }
        }

        using (var output = File.Create(path))
        using (var gzip = new GZipStream(output, CompressionLevel.SmallestSize))
        using (var writer = new TarWriter(gzip, leaveOpen: false))
        {
            foreach (var pair in files)
            {
                var entry = new PaxTarEntry(TarEntryType.RegularFile, pair.Key)
                {
                    DataStream = new MemoryStream(pair.Value, writable: false),
                };
                writer.WriteEntry(entry);
            }

            if (symbolicLink)
            {
                writer.WriteEntry(new PaxTarEntry(TarEntryType.SymbolicLink, "package/link")
                {
                    LinkName = "../outside",
                });
            }
        }

        var bytes = File.ReadAllBytes(path);
        return new PackageFixture(
            path,
            Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant(),
            "sha512-" + Convert.ToBase64String(SHA512.HashData(bytes)));
    }

    private static ConnectorPluginSource Source(string version, PackageFixture package) => new(
        new ConnectorPluginCatalog(
            "plugin-1",
            "Test Plugin",
            "test-plugin",
            "Tests plugins",
            "ChatOS",
            "Tools",
            "ChatOS"),
        new ConnectorPluginRelease(
            "release-1",
            version,
            package.Sha256,
            new ConnectorPluginNpmPackage("test-plugin", version, package.Integrity),
            ["windows"]),
        new ConnectorPluginPreference(true));

    private sealed record PackageFixture(string Path, string Sha256, string Integrity);
}
